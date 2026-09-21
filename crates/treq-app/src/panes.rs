//! Insomnia 风格三栏渲染：活动栏 / 侧栏 / 请求区 / 响应区 / 状态栏。
//!
//! 拆成了几个子模块（都是 `impl AppModel`，只是分文件放）：popup / tree /
//! editor / editor_auth / editor_kv / response；本文件只留根视图、状态栏、
//! 活动栏和几个格式化小工具。

use crate::model::{
    AppModel, EditorFields, EditorTab, KvWhich, RenameState, ResponseTab, RowKind, RowRef,
    Selection, TreeRow,
};
use crate::theme;
use crate::widgets::{self, PillTone, TextField};
use gpui::{prelude::*, *};
use std::sync::Arc;
use treq_core::{BodyKind, FormField, HistoryEntry, Kv, RequestItem, ResponseData};
mod editor;
mod editor_auth;
mod editor_kv;
mod popup;
mod response;
mod tree;

impl AppModel {
    /// 「5 分钟前」这类相对时间标签。
    pub(crate) fn ago_label(&self, sent_at: i64, now: i64) -> String {
        let diff_min = ((now - sent_at) / 60_000).max(0);
        if diff_min == 0 {
            self.t("history.just_now").to_string()
        } else if diff_min < 60 {
            format!("{}{}", diff_min, self.t("history.min_ago"))
        } else if diff_min < 60 * 24 {
            format!("{}{}", diff_min / 60, self.t("history.hour_ago"))
        } else {
            format!("{}{}", diff_min / (60 * 24), self.t("history.day_ago"))
        }
    }

    /// 操作反馈 toast：右下角浮一条，2.6s 后自动消失。
    pub(crate) fn toast_render(&self, msg: String) -> AnyElement {
        div()
            .id("toast")
            .absolute()
            .bottom(theme::statusbar_h() + px(12.))
            .right(theme::sp6())
            .max_w(px(460.))
            .px(theme::sp4())
            .py(theme::sp3())
            .bg(theme::bg_popup())
            .border_1()
            .border_color(theme::primary())
            .rounded(px(4.))
            .shadow_lg()
            .text_size(px(theme::font_small()))
            .text_color(theme::fg_bright())
            .child(SharedString::from(msg))
            .into_any()
    }

    /// 设置 toast 文本（自动消失）：所有复制/保存反馈都走这里。
    pub(crate) fn toast(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.flash = Some(msg.into());
        self.flash_token = self.flash_token.wrapping_add(1);
        let token = self.flash_token;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(2600))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.flash_token == token {
                    this.flash = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// 窗口底部状态栏：Preferences + 标语（Insomnia 的底栏）。
    pub fn status_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("statusbar")
            .h(theme::statusbar_h())
            .flex_none()
            .flex()
            .items_center()
            .px(theme::sp4())
            .gap(theme::sp3())
            .bg(theme::bg_base())
            .border_t_1()
            .border_color(theme::border())
            .child(widgets::link(
                "prefs",
                format!("⚙ {}", self.t("action.preferences")),
                cx.listener(|this, e: &MouseDownEvent, _w, cx| {
                    this.open_prefs_menu(e.position, cx)
                }),
            ))
            .child(div().flex_1())
            .when_some(self.stream_status(), |d, s| {
                d.child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::primary())
                        .child(SharedString::from(s)),
                )
            })
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("app.tagline")),
            )
    }
}

// ===================== 自由函数 =====================

/// Send 按钮的三种状态：发送 / 发送中（不可点） / 停止正在收的事件流
#[derive(Clone, Copy, PartialEq)]
enum SendAction {
    Send,
    Stop,
    Busy,
}

/// 小的分段切换（Pretty / Raw），激活项高亮。
fn view_switch(
    id: &'static str,
    active: bool,
    label: &'static str,
    cx: &mut Context<AppModel>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("view/{}", id)))
        .flex_none()
        .h(theme::sp5())
        .px(theme::sp3())
        .flex()
        .items_center()
        .rounded(px(3.))
        .cursor_pointer()
        .text_size(px(theme::font_small()))
        .when(active, |d| {
            d.bg(theme::bg_button())
                .text_color(theme::fg_bright())
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!active, |d| {
            d.text_color(theme::fg_dim())
                .hover(|d| d.text_color(theme::fg_normal()))
        })
        .child(label)
        .on_click(cx.listener(move |this, _, _w, cx| {
            if id == "full" {
                this.resp_force_full = !this.resp_force_full;
            } else if id == "fold-all" {
                // 一颗按钮两态：已经折了就是「展开全部」，否则折最外层
                if this.resp_folded.is_empty() {
                    this.fold_all(cx);
                } else {
                    this.unfold_all(cx);
                }
            } else if id == "sse-time" {
                this.toggle_sse_time(cx);
            } else {
                this.response_pretty = id == "pretty";
            }
            cx.notify();
        }))
}

/// 下划线输入容器（Insomnia 的 kv 输入：无框，仅底色下划线）。
fn underline(child: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .w_full()
        .items_center()
        .h(theme::control_h())
        .border_b_1()
        .border_color(theme::border_input())
        .child(child)
}

fn empty_hint(text: &str) -> AnyElement {
    div()
        .p(theme::sp4())
        .text_size(px(theme::font_small() + 1.))
        .text_color(theme::fg_dark())
        .child(SharedString::from(text.to_string()))
        .into_any()
}

fn error_hint(text: &str) -> AnyElement {
    hint_block(text, theme::red())
}

/// 提示类（非报错）说明：大响应跳过渲染之类，用暗色而不是红色。
fn info_hint(text: &str) -> AnyElement {
    hint_block(text, theme::fg_dim())
}

fn hint_block(text: &str, color: gpui::Rgba) -> AnyElement {
    div()
        .p(theme::sp4())
        .text_size(px(theme::font_small() + 1.))
        .text_color(color)
        .child(SharedString::from(text.to_string()))
        .into_any()
}

/// 认证方式 → i18n key（标签文案）。
pub(crate) fn auth_kind_key(kind: &str) -> &'static str {
    match kind {
        "bearer" => "auth.kind.bearer",
        "basic" => "auth.kind.basic",
        "apikey" => "auth.kind.apikey",
        _ => "auth.kind.none",
    }
}

pub(crate) fn kv_prefix(which: KvWhich) -> &'static str {
    match which {
        KvWhich::Params => "P",
        KvWhich::Headers => "H",
        KvWhich::FormData => "FD",
    }
}

fn body_kind_label(model: &AppModel, kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::None => model.t("editor.tab.none"),
        BodyKind::Json => model.t("editor.tab.json"),
        BodyKind::Text => model.t("editor.tab.text"),
        BodyKind::Form => model.t("editor.tab.form"),
        BodyKind::Multipart => model.t("editor.tab.multipart"),
        BodyKind::File => model.t("editor.tab.file"),
        BodyKind::Raw => model.t("editor.tab.raw"),
    }
}

fn status_tone(status: Option<u16>) -> PillTone {
    match status {
        Some(s) if (200..300).contains(&s) => PillTone::Success,
        Some(s) if (300..400).contains(&s) => PillTone::Info,
        Some(s) if (400..500).contains(&s) => PillTone::Warning,
        Some(_) => PillTone::Danger,
        None => PillTone::Danger,
    }
}

fn status_color_of(status: Option<u16>) -> Rgba {
    match status {
        Some(s) if (200..300).contains(&s) => theme::green(),
        Some(s) if (300..400).contains(&s) => theme::blue(),
        Some(s) if (400..500).contains(&s) => theme::orange(),
        Some(_) => theme::red(),
        None => theme::fg_dark(),
    }
}

/// 行号列（等宽、右对齐、暗灰）
/// 事件流的到达时间列（只有每个事件的首行有内容，其余留白保持对齐）。
fn time_cell(t: Option<&str>) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(88.))
        .pr(theme::sp3())
        .text_right()
        .text_size(px(theme::font_small()))
        .text_color(theme::fg_dark())
        .font(theme::mono())
        .child(SharedString::from(t.unwrap_or_default().to_string()))
}

/// 事件流时间列查表：`Some(时间)` = 这一行是某个事件的首行，`Some("")` = 占位（保持行号对齐），
/// `None` = 这一版不显示时间列。marks 按行号递增，二分即可。
pub(crate) fn sse_time_for(marks: &[(usize, String)], show: bool, line: usize) -> Option<&str> {
    if !show || marks.is_empty() {
        return None;
    }
    Some(
        marks
            .binary_search_by_key(&line, |(l, _)| *l)
            .map(|i| marks[i].1.as_str())
            .unwrap_or_default(),
    )
}

fn gutter(n: usize) -> impl IntoElement {
    div()
        .flex_none()
        .w(theme::sp6())
        .pr(theme::sp3())
        .text_right()
        .text_size(px(theme::font_small()))
        .text_color(theme::fg_dark())
        .child(SharedString::from(n.to_string()))
}

/// 一行 JSON 的行壳（行号 + 折叠箭头 + 正文元素）。
/// 正文元素由调用方给：正文既要着色又要能鼠标选中，需要 resp_sel 的上下文。
fn json_line(
    time: Option<&str>,
    n: usize,
    arrow: AnyElement,
    folded: bool,
    body: AnyElement,
) -> Div {
    div()
        .flex()
        .w_full()
        .items_start()
        .text_size(px(theme::font_small() + 1.))
        .line_height(px(theme::line_h()))
        .font(theme::mono())
        .when(time.is_some(), |d| d.child(time_cell(time)))
        .child(arrow)
        .child(gutter(n))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(body)
                // 折起来的那段用省略号示意（点行首 ▸ 展开）
                .when(folded, |d| {
                    d.child(
                        div()
                            .text_color(theme::fg_dark())
                            .child(SharedString::from("…")),
                    )
                }),
        )
}

/// 正文一行 → 可交互文本：hover 时把「鼠标下是第几个字节」报给模型，鼠标选中就靠它。
fn resp_interactive(
    line: String,
    ix: usize,
    runs: Vec<TextRun>,
    cx: &mut Context<AppModel>,
) -> InteractiveText {
    let weak = cx.weak_entity();
    // hover 监听器是「裸 App + 弱引用」签名（不是 cx.listener 那种带 Context 的形式）
    let hover = move |col: Option<usize>, _e: MouseMoveEvent, _w: &mut Window, cx: &mut App| {
        if let Some(col) = col {
            let _ = weak.update(cx, |this: &mut AppModel, cx| {
                this.resp_sel_drag_to(ix, col, cx);
            });
        }
    };
    InteractiveText::new(
        SharedString::from(format!("resp-line/{}", ix)),
        StyledText::new(line).with_runs(runs),
    )
    .on_hover(hover)
}

pub(crate) fn history_row(
    model: &mut AppModel,
    h: &HistoryEntry,
    now: i64,
    cx: &mut Context<AppModel>,
) -> impl IntoElement {
    let status_color = status_color_of(h.status);
    let status_text = h
        .status
        .map(|s| s.to_string())
        .or_else(|| h.error.as_ref().map(|_| "✗".to_string()))
        .unwrap_or_default();
    let duration = fmt_duration_ms(h.duration_ms);
    let ago = model.ago_label(h.sent_at, now);
    let entry = h.clone();
    let h_id = h.id;
    let method_c = h.request.method.clone();
    // 行内只留「这次请求自己的信息」：状态 / 方法 / 耗时 / 时间
    // （请求名与 URL 在同一请求的历史里每次相同，不再重复展示）
    div()
        .id(SharedString::from(format!("hist/{}", h.id)))
        .flex()
        .h(theme::row_h())
        .items_center()
        .gap(theme::sp3())
        .px(theme::sp4())
        .cursor_pointer()
        .text_size(px(theme::font_small() + 1.))
        .hover(|d| d.bg(theme::bg_hover()))
        .child(
            div()
                .flex_none()
                .w(px(28.))
                .text_color(status_color)
                .child(status_text),
        )
        .child(widgets::method_badge(&method_c))
        .child(
            div()
                .flex_none()
                .w(px(64.))
                .text_color(theme::fg_dim())
                .child(SharedString::from(duration)),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_none()
                .text_color(theme::fg_dark())
                .child(SharedString::from(ago)),
        )
        .child(widgets::delete_btn(
            "hist",
            cx.listener(move |this, _, _w, cx| this.delete_history(h_id, cx)),
        ))
        .on_click(cx.listener(move |this, _, _w, cx| this.restore_history(&entry, cx)))
}

pub(crate) fn apply_snapshot(dst: &mut RequestItem, src: &RequestItem) {
    dst.method = src.method.clone();
    dst.url = src.url.clone();
    dst.params = src.params.clone();
    dst.headers = src.headers.clone();
    dst.body = src.body.clone();
    dst.description = src.description.clone();
}

/// 命令面板用：URL 中段截断为 …。
pub(crate) fn truncate_url(url: &str, max: usize) -> String {
    let chars: Vec<char> = url.chars().collect();
    if chars.len() <= max {
        url.to_string()
    } else if max <= 15 {
        chars[..max].iter().collect()
    } else {
        let keep = (max - 3) / 2;
        let head: String = chars[..keep].iter().collect();
        let tail: String = chars[chars.len() - keep..].iter().collect();
        format!("{}…{}", head, tail)
    }
}

fn fmt_duration(d: std::time::Duration) -> String {
    fmt_duration_ms(d.as_millis() as u64)
}

pub(crate) fn fmt_duration_ms(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        format!("{} ms", ms)
    }
}

fn fmt_size(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{} B", n)
    }
}

#[cfg(test)]
mod tests {
    use super::sse_time_for;

    #[test]
    fn time_column_only_fills_the_first_line_of_each_event() {
        let marks = vec![(0usize, "10:00:00.000".to_string()), (3, "10:00:01.500".to_string())];
        assert_eq!(sse_time_for(&marks, true, 0), Some("10:00:00.000"));
        assert_eq!(sse_time_for(&marks, true, 3), Some("10:00:01.500"));
        assert_eq!(sse_time_for(&marks, true, 1), Some(""), "中间行留白占位");
        assert_eq!(sse_time_for(&marks, true, 99), Some(""));
        assert_eq!(sse_time_for(&marks, false, 0), None, "开关关掉不显示时间列");
        assert_eq!(sse_time_for(&[], true, 0), None, "不是事件流就没有时间列");
    }
}
