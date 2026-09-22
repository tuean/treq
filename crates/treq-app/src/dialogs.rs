//! 弹窗渲染：回收站 / 环境编辑器 / cURL 导入。

use crate::i18n::Locale;
use crate::model::{AppModel, SettingsTab, VarSource};
use crate::settings::DropdownStyle;
use crate::theme;
use crate::widgets::{self};
use gpui::{prelude::*, *};
use treq_core::TrashKind;

/// 设置行的小标题。
fn setting_label(text: &str) -> impl IntoElement {
    div()
        .text_size(px(theme::font_small()))
        .text_color(theme::fg_dim())
        .child(SharedString::from(text.to_string()))
}

/// 配置页一行字体设置：左标签 + 定宽输入框。
fn font_row(
    label: &str,
    field: gpui::Entity<crate::widgets::TextField>,
    width: gpui::Pixels,
    _model: &crate::model::AppModel,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(theme::sp4())
        .child(div().w(px(110.)).child(setting_label(label)))
        .child(div().w(width).child(field))
}

/// 单选圆点（选中态品牌紫实心）。
fn radio_dot(selected: bool) -> impl IntoElement {
    div()
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(if selected {
            theme::primary()
        } else {
            theme::border_strong()
        })
        .when(selected, |d| {
            d.child(div().size(px(6.)).rounded_full().bg(theme::primary()))
        })
}

/// 设置页的选项小胶囊（单选片段 / 动作按钮）。
fn chip(
    id: &str,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("chip/{}", id)))
        .h(px(24.))
        .px(theme::sp3())
        .flex()
        .items_center()
        .rounded(px(3.))
        .cursor_pointer()
        .text_size(px(theme::font_body()))
        .bg(if selected {
            theme::bg_accent()
        } else {
            theme::bg_button()
        })
        .text_color(if selected {
            theme::fg_on_accent()
        } else {
            theme::fg_normal()
        })
        .when(!selected, |d| {
            d.hover(|d| d.bg(theme::bg_hover()).text_color(theme::fg_bright()))
        })
        .child(label.into())
        .on_mouse_down(MouseButton::Left, on_click)
}

impl AppModel {
    /// 「备份与恢复」对话框：列出备份 zip，可立即备份 / 合并恢复 / 覆盖恢复。
    pub(crate) fn restore_dialog_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let bar_restore_list = self.scroll_handle("restore-list");
        let Some(dlg) = &self.restore_dialog else {
            return div().into_any();
        };
        let now = treq_core::backup::now_secs();
        let selected = dlg.selected;
        let message = dlg.message.clone();
        let failed = dlg.failed;
        let dir_label = dlg.dir.display().to_string();
        let empty = dlg.items.is_empty();
        let items = dlg.items.clone();
        let mut list = div()
            .id("restore-list")
            .max_h(px(240.))
            .flex()
            .flex_col()
            .overflow_scroll().track_scroll(&bar_restore_list);
        for (i, b) in items.iter().enumerate() {
            let ago = self.ago_label(b.created * 1000, now * 1000);
            let is_sel = i == selected;
            list = list.child(
                div()
                    .id(SharedString::from(format!("restore-item-{i}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(theme::sp3())
                    .px(theme::sp2())
                    .py(theme::sp1())
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(theme::bg_selected()))
                    .when(!is_sel, |d| d.hover(|d| d.bg(theme::bg_hover())))
                    .on_click(cx.listener(move |this, _, _w, cx| {
                        if let Some(d) = this.restore_dialog.as_mut() {
                            d.selected = i;
                            d.message = None;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::fg_normal())
                            .child(SharedString::from(b.name.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::fg_dim())
                            .child(SharedString::from(format!("{} · {}", b.size_label(), ago))),
                    ),
            );
        }
        let mut card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(SharedString::from(format!(
                        "{}{}{}",
                        self.t("backup.dir"),
                        " · ",
                        dir_label
                    ))),
            );
        if empty {
            card = card.child(
                div()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .child(self.t("backup.empty")),
            );
        } else {
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(self.t("backup.list_hint")),
            );
            card = card.child(list);
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("backup.merge_hint")),
            );
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("backup.overwrite_hint")),
            );
        }
        if let Some(msg) = message {
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(if failed { theme::red() } else { theme::green() })
                    .child(SharedString::from(msg)),
            );
        }
        let mut footer = div().flex().items_center().gap(theme::sp3());
        let has = !empty;
        let handle = cx.entity();
        footer = footer.child(widgets::btn(
            SharedString::from(self.t("backup.now")),
            widgets::BtnVariant::Default,
            {
                let handle = cx.entity();
                move |_, _w, app| {
                    handle.update(app, |this, cx| this.backup_now(cx));
                }
            },
        ));
        if has {
            let h1 = handle.clone();
            footer = footer.child(widgets::btn(
                SharedString::from(self.t("backup.restore_merge")),
                widgets::BtnVariant::Default,
                move |_, _w, app| {
                    h1.update(app, |this, cx| this.restore_from_backup(false, cx));
                },
            ));
            let h2 = handle.clone();
            footer = footer.child(widgets::btn(
                SharedString::from(self.t("backup.restore_overwrite")),
                widgets::BtnVariant::Danger,
                move |_, _w, app| {
                    h2.update(app, |this, cx| this.restore_from_backup(true, cx));
                },
            ));
        }
        footer = footer.child(
            div()
                .id("restore-close")
                .h(theme::control_h())
                .flex()
                .items_center()
                .px(theme::sp3())
                .ml_auto()
                .text_size(px(theme::font_body()))
                .text_color(theme::fg_dim())
                .cursor_pointer()
                .hover(|d| d.text_color(theme::fg_normal()))
                .child(self.t("action.cancel"))
                .on_click(cx.listener(|this, _, _w, cx| this.close_restore_dialog(cx))),
        );
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("backup.title")),
            width: px(520.),
            content: card,
            footer,
        }))
    }

    pub(crate) fn codegen_dialog_render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let code = self.codegen_text();
        let lang = self.codegen.unwrap_or(treq_core::CodegenLang::JavaOkHttp);
        // 代码框：等宽 + 深底 + 描边，和 URL 预览块同一套观感；横向不折行，直接滚动
        let bar_codegen_body = self.scroll_handle("codegen-body");
        let mut lines = div()
            .id("codegen-body")
            .h(px(320.))
            .w_full()
            .px(theme::sp3())
            .py(theme::sp3())
            .flex_col()
            .gap(theme::sp1())
            .font(theme::mono())
            .text_size(px(theme::mono_size()))
            .line_height(px(theme::line_h()))
            .overflow_scroll().track_scroll(&bar_codegen_body);
        for line in code.lines() {
            let mut row = div().flex().whitespace_nowrap();
            for (text, color) in crate::syntax::code_spans(line) {
                row = row.child(div().text_color(color).child(SharedString::from(text)));
            }
            lines = lines.child(row);
        }
        let code_box = div()
            .bg(theme::bg_field())
            .border_1()
            .border_color(theme::border_strong())
            .rounded(px(3.))
            .child(lines);
        // 语言/库选择：点击出菜单（当前项高亮），选择后记住
        let picker = widgets::dropdown(
            "codegen-lang",
            lang.label(),
            220.0,
            self.settings.dropdown_style,
            self.dropdown_open("codegen"),
            cx.listener(
                |this, e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<AppModel>| {
                    this.open_codegen_menu(e.position, cx)
                },
            ),
        );
        let content = div()
            .flex()
            .flex_col()
            .gap(theme::sp2())
            .child(
                div()
                    .px(theme::sp4())
                    .pt(theme::sp3())
                    .pb(theme::sp2())
                    .flex()
                    .items_center()
                    .gap(theme::sp3())
                    .child(picker)
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::fg_dark())
                            .child(self.t("codegen.hint")),
                    ),
            )
            .child(div().px(theme::sp4()).child(code_box));
        let _ = lang;
        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(widgets::btn(
                SharedString::from(self.t("action.copy")),
                widgets::BtnVariant::Default,
                {
                    let handle = cx.entity();
                    move |_, w, app| {
                        handle.update(app, |this, c| this.copy_code(w, c));
                    }
                },
            ))
            .child(widgets::btn(
                SharedString::from(self.t("action.cancel")),
                widgets::BtnVariant::Default,
                {
                    let handle = cx.entity();
                    move |_, _w, app| {
                        handle.update(app, |this, c| this.close_codegen(c));
                    }
                },
            ));
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("action.plugin")),
            width: px(560.),
            content,
            footer,
        }))
    }

    pub(crate) fn trash_dialog_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entries = self.trash_store.list().unwrap_or_default();
        let bar_trash_rows = self.scroll_handle("trash-rows");
        let mut rows = div()
            .id("trash-rows")
            .flex_col()
            .gap(theme::sp1())
            .overflow_scroll().track_scroll(&bar_trash_rows)
            .max_h(px(300.));
        if entries.is_empty() {
            rows = rows.child(
                div()
                    .px(theme::sp3())
                    .py(theme::sp4())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .child(self.t("trash.empty")),
            );
        }
        for e in &entries {
            let col_id = e.collection_id.clone();
            let group_id = e.group_id.clone();
            let req_id = e.request_id.clone();
            let label = match e.kind {
                TrashKind::Collection => format!("📁 {}", e.name),
                TrashKind::Group => format!("🗂 {}", e.name),
                TrashKind::Request => format!("📄 {}", e.name),
            };
            let entry_key = format!(
                "{}",
                req_id
                    .clone()
                    .or_else(|| group_id.clone())
                    .unwrap_or_else(|| col_id.clone())
            );
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("trash/{}", entry_key)))
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .gap(theme::sp2())
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_normal())
                    .child(div().flex_1().truncate().child(SharedString::from(label)))
                    .child(
                        div()
                            .id(SharedString::from(format!("trash-restore/{}", entry_key)))
                            .h(theme::control_h())
                            .flex()
                            .items_center()
                            .px(theme::sp3())
                            .text_size(px(theme::font_body()))
                            .text_color(theme::blue())
                            .cursor_pointer()
                            .hover(|d| d.text_color(theme::fg_bright()))
                            .child(self.t("trash.restore"))
                            .on_click(cx.listener({
                                let cid = col_id.clone();
                                let gid = group_id.clone();
                                let rid = req_id.clone();
                                move |this: &mut AppModel,
                                      _e: &ClickEvent,
                                      _w: &mut Window,
                                      cx: &mut Context<AppModel>| {
                                    this.trash_restore(
                                        cid.clone(),
                                        gid.clone(),
                                        rid.clone(),
                                        cx,
                                    );
                                }
                            })),
                    ),
            );
        }
        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(
                div()
                    .id("trash-empty-all")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .rounded(px(3.))
                    .text_size(px(theme::font_body()))
                    .text_color(theme::red())
                    .cursor_pointer()
                    .when(self.trash_confirm, |d| d.bg(theme::red()).text_color(theme::fg_white()))
                    .hover(|d| d.opacity(0.8))
                    .child(if self.trash_confirm {
                        self.t("trash.empty_confirm")
                    } else {
                        self.t("trash.empty_all")
                    })
                    .on_click(cx.listener(|this, _, _w, cx| this.trash_empty(cx))),
            )
            .child(widgets::btn(
                SharedString::from(self.t("action.cancel")),
                widgets::BtnVariant::Default,
                {
                    let handle = cx.entity();
                    move |_, _w, app| {
                        handle.update(app, |this, c| this.close_trash_dialog(c));
                    }
                },
            ));
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("trash.title")),
            width: px(400.),
            content: rows,
            footer,
        }))
        .into_any()
    }

    pub(crate) fn env_editor_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(ed) = &self.env_editor else {
            return div().into_any();
        };
        let env_name = if ed.target == "base" {
            self.workspace.base_env.name.clone()
        } else {
            self.workspace
                .environments
                .iter()
                .find(|e| e.id == ed.target)
                .map(|e| e.name.clone())
                .unwrap_or_default()
        };
        let title = format!("{} {}", self.t("side.environment"), env_name);
        let vars = ed.vars.clone();
        let fields = ed.fields.clone();

        let bar_env_rows = self.scroll_handle("env-rows");
        let mut rows = div()
            .id("env-rows")
            .flex_col()
            .gap(theme::sp1())
            .overflow_scroll().track_scroll(&bar_env_rows)
            .max_h(px(300.));
        for (i, ((_k, _v), (kf, vf))) in vars.iter().zip(fields.iter()).enumerate() {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp2())
                    .child(div().w(px(120.)).child(kf.clone()))
                    .child(div().flex_1().child(vf.clone()))
                    .child(
                        div()
                            .id(SharedString::from(format!("env-del/{}", i)))
                            .h(theme::control_h())
                            .flex()
                            .items_center()
                            .px(theme::sp2())
                            .text_size(px(theme::font_body()))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|d| d.text_color(theme::red()))
                            .child("×")
                            .on_click(cx.listener(move |this, _, _w, cx| {
                                this.env_remove_var(i, cx);
                            })),
                    ),
            );
        }
        rows = rows.child(
            div()
                .id("env-add")
                .mt_1()
                .h(theme::control_h())
                .flex()
                .items_center()
                .px(theme::sp3())
                .text_size(px(theme::font_body()))
                .text_color(theme::link_blue())
                .cursor_pointer()
                .hover(|d| d.text_color(theme::blue()))
                .child("+ 变量")
                .on_click(cx.listener(|this, _, _w, cx| this.env_add_var(cx))),
        );
        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(
                div()
                    .id("env-cancel")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.cancel"))
                    .on_click(cx.listener(|this, _, _w, cx| this.env_cancel(cx))),
            )
            .child(widgets::btn(
                SharedString::from(self.t("action.save")),
                widgets::BtnVariant::Accent,
                {
                    let handle = cx.entity();
                    move |_, _w, app| {
                        handle.update(app, |this, c| this.env_save(c));
                    }
                },
            ));
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(title),
            width: px(480.),
            content: rows,
            footer,
        }))
        .into_any()
    }

    /// Cookie 罐：按域列出存下的 cookie，可逐域清或全清。
    pub(crate) fn cookie_dialog_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let groups = self.cookie_groups();
        let now = treq_core::cookies::now_unix();
        let message = self.cookie_message.clone();
        let bar_cookie_list = self.scroll_handle("cookie-list");
        let mut body = div()
            .id("cookie-list")
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .max_h(px(360.))
            .overflow_scroll().track_scroll(&bar_cookie_list);
        if groups.is_empty() {
            body = body.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(self.t("cookies.empty")),
            );
        }
        for (host, list) in groups {
            let mut rows = div().flex().flex_col().gap(theme::sp1());
            for c in &list {
                rows = rows.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::sp1())
                        .child(
                            div()
                                .flex()
                                .gap(theme::sp3())
                                .font(theme::mono())
                                .text_size(px(theme::mono_size()))
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(theme::json_key())
                                        .child(SharedString::from(c.name.clone())),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(theme::fg_normal())
                                        .child(SharedString::from(c.value.clone())),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_small()))
                                .text_color(theme::fg_dark())
                                .child(SharedString::from(c.attrs(now))),
                        ),
                );
            }
            let host_for_clear = host.clone();
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::sp2())
                    .px(theme::sp3())
                    .py(theme::sp3())
                    .border_1()
                    .border_color(theme::border())
                    .rounded_md()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::sp3())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(theme::font_body()))
                                    .text_color(theme::fg_bright())
                                    .child(SharedString::from(host)),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "cookie-clear-{}",
                                        host_for_clear
                                    )))
                                    .flex_none()
                                    .h(theme::control_h())
                                    .flex()
                                    .items_center()
                                    .px(theme::sp3())
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::fg_dim())
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(theme::red()))
                                    .child(self.t("cookies.clear_host"))
                                    .on_click(cx.listener(move |this, _, _w, cx| {
                                        this.clear_cookies(Some(&host_for_clear), cx);
                                    })),
                            ),
                    )
                    .child(rows),
            );
        }
        let card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(self.t("cookies.hint")),
            )
            .child(body)
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::green())
                    .child(SharedString::from(message.unwrap_or_default())),
            );
        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(
                div()
                    .id("cookies-clear-all")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::red()))
                    .child(self.t("cookies.clear_all"))
                    .on_click(cx.listener(|this, _, _w, cx| this.clear_cookies(None, cx))),
            )
            .child(
                div()
                    .id("cookies-close")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.cancel"))
                    .on_click(cx.listener(|this, _, _w, cx| this.close_cookie_dialog(cx))),
            );
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("cookies.title")),
            width: px(560.),
            content: card,
            footer,
        }))
    }

    /// 变量面板：合并后的环境变量 + 当前请求缺哪些；敏感名默认打码。
    pub(crate) fn vars_panel_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.env_var_rows();
        let missing = self.missing_vars();
        let reveal = self.vars_reveal;
        let active_name = self
            .active_env()
            .map(|e| e.name.clone())
            .unwrap_or_else(|| self.t("env.none").to_string());

        let bar_vars_list = self.scroll_handle("vars-list");
        let mut list = div()
            .id("vars-list")
            .flex()
            .flex_col()
            .gap(theme::sp1())
            .max_h(px(380.))
            .overflow_scroll().track_scroll(&bar_vars_list);
        for (name, value, from_active) in &rows {
            let secret = treq_core::vars::looks_secret(name);
            let shown = if secret && !reveal && !value.is_empty() {
                "••••••".to_string()
            } else {
                value.clone()
            };
            let mut row = div()
                .flex()
                .items_center()
                .gap(theme::sp3())
                .px(theme::sp3())
                .py(theme::sp1())
                .rounded(px(3.))
                .child(
                    div()
                        .w(px(200.))
                        .flex_none()
                        .truncate()
                        .font(theme::mono())
                        .text_size(px(theme::mono_size()))
                        .text_color(theme::json_key())
                        .child(SharedString::from(name.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font(theme::mono())
                        .text_size(px(theme::mono_size()))
                        .text_color(if shown.is_empty() {
                            theme::fg_dark()
                        } else {
                            theme::fg_normal()
                        })
                        .child(SharedString::from(if shown.is_empty() {
                            "（空）".to_string()
                        } else {
                            shown
                        })),
                );
            if *from_active {
                row = row.child(
                    div()
                        .flex_none()
                        .px(theme::sp2())
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::primary())
                        .child(SharedString::from(active_name.clone())),
                );
            }
            list = list.child(row);
        }
        for name in &missing {
            let name_for_add = name.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp3())
                    .px(theme::sp3())
                    .py(theme::sp1())
                    .child(
                        div()
                            .w(px(200.))
                            .flex_none()
                            .truncate()
                            .font(theme::mono())
                            .text_size(px(theme::mono_size()))
                            .text_color(theme::red())
                            .child(SharedString::from(name.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::red())
                            .child(self.t("vars.undefined")),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("vars-add-{}", name)))
                            .flex_none()
                            .px(theme::sp2())
                            .text_size(px(theme::font_small()))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|d| d.text_color(theme::primary()))
                            .child(self.t("vars.add"))
                            .on_click(cx.listener(move |this, _, _w, cx| {
                                this.add_var_to_env(name_for_add.clone(), cx);
                            })),
                    ),
            );
        }
        if rows.is_empty() && missing.is_empty() {
            list = list.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(self.t("vars.empty")),
            );
        }

        let head = if missing.is_empty() {
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::fg_dim())
                .child(self.t("vars.hint"))
        } else {
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::red())
                .child(SharedString::from(format!(
                    "{}{}：{}",
                    self.t("vars.missing"),
                    missing.len(),
                    missing.join("、")
                )))
        };

        let card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(head)
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(SharedString::from(format!(
                        "{}：{} · {}",
                        self.t("vars.env"),
                        active_name,
                        self.t("vars.masked_hint")
                    ))),
            )
            .child(list);

        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(
                div()
                    .id("vars-reveal")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t(if reveal { "vars.hide" } else { "vars.reveal" }))
                    .on_click(cx.listener(|this, _, _w, cx| this.toggle_vars_reveal(cx))),
            )
            .child(
                div()
                    .id("vars-edit-env")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.edit_env"))
                    .on_click(cx.listener(|this, _, _w, cx| {
                        let target = if this.settings.active_environment_id.is_some() {
                            this.settings
                                .active_environment_id
                                .clone()
                                .unwrap_or_default()
                        } else {
                            "base".to_string()
                        };
                        this.close_vars_panel(cx);
                        this.open_env_editor(&target, cx);
                    })),
            )
            .child(
                div()
                    .id("vars-close")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.cancel"))
                    .on_click(cx.listener(|this, _, _w, cx| this.close_vars_panel(cx))),
            );
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("vars.title")),
            width: px(620.),
            content: card,
            footer,
        }))
    }

    /// 「设置」页：样式 / 通用 / 网络 / 备份四个 tab；改动即存。
    pub(crate) fn settings_dialog_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let bar_settings_body = self.scroll_handle("settings-body");
        let Some(tab) = self.settings_page else {
            return div().into_any();
        };
        let tabs = [
            (SettingsTab::Styles, "settings.tab.style"),
            (SettingsTab::General, "settings.tab.general"),
            (SettingsTab::Network, "settings.tab.net"),
            (SettingsTab::Backup, "settings.tab.backup"),
        ];
        let mut bar = div()
            .id("settings-tabs")
            .h(theme::tab_h())
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .bg(theme::bg_sunken())
            .border_b_1()
            .border_color(theme::border());
        for (i, (t, key)) in tabs.into_iter().enumerate() {
            if i > 0 {
                bar = bar.child(div().flex_none().w(px(1.)).h(px(18.)).bg(theme::border()));
            }
            bar = bar.child(widgets::tab(
                &format!("settings-{}", i),
                self.t(key),
                t == tab,
                None,
                cx.listener(move |this, _, _w, cx| this.open_settings_page(t, cx)),
            ));
        }
        let body = match tab {
            SettingsTab::Styles => self.settings_styles_tab(cx),
            SettingsTab::General => self.settings_general_tab(cx),
            SettingsTab::Network => self.settings_network_tab(cx),
            SettingsTab::Backup => self.settings_backup_tab(cx),
        };
        let content = div().flex().flex_col().child(bar).child(
            div()
                .id("settings-body")
                .h(px(360.))
                .overflow_scroll().track_scroll(&bar_settings_body)
                .child(body),
        );
        let footer = div().flex().gap(theme::sp3()).child(widgets::btn(
            SharedString::from(self.t("settings.close")),
            widgets::BtnVariant::Default,
            {
                let handle = cx.entity();
                move |_, _w, app| {
                    handle.update(app, |this, c| this.close_settings_page(c));
                }
            },
        ));
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("settings.title")),
            width: px(580.),
            content,
            footer,
        }))
    }

    /// 样式栏：下拉框方案，点选即生效并写入配置。
    fn settings_styles_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.dropdown_style;
        let mut body = div()
            .px(theme::sp4())
            .py(theme::sp3())
            .flex()
            .flex_col()
            .gap(theme::sp1())
            .child(
                div()
                    .pb(theme::sp2())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("settings.style.hint")),
            );
        for style in DropdownStyle::ALL {
            body = body.child(
                div()
                    .id(SharedString::from(format!(
                        "style-row-{}",
                        style.label_key()
                    )))
                    .flex()
                    .items_center()
                    .gap(theme::sp4())
                    .px(theme::sp3())
                    .h(px(40.))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::bg_hover()))
                    .child(radio_dot(style == current))
                    .child(widgets::dropdown(
                        &format!("style-sample-{}", style.label_key()),
                        "JSON",
                        110.0,
                        style,
                        self.dropdown_open("style-preview") && self.style_preview == Some(style),
                        cx.listener(move |this, e: &MouseDownEvent, _w, cx| {
                            this.preview_dropdown_style(style, e.position, cx)
                        }),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(theme::font_body()))
                                    .text_color(theme::fg_normal())
                                    .child(self.t(style.label_key())),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::fg_dark())
                                    .child(self.t(style.desc_key())),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _w, cx| this.set_dropdown_style(style, cx)),
                    ),
            );
        }
        // 多行编辑器（请求体 JSON / Docs markdown）行高：手动输入
        let line_field = self.editor_line_field(cx);
        body = body.child(
            div()
                .pt(theme::sp4())
                .flex()
                .flex_col()
                .gap(theme::sp2())
                .child(setting_label(self.t("settings.editor_line_h")))
                .child(div().w(px(90.)).child(line_field))
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::fg_dark())
                        .child(self.t("settings.editor_line_h.hint")),
                ),
        );

        // 字体：界面与代码分开配，家族留空＝系统默认，改完立即生效
        use crate::model::FontField;
        let (ui_lo, ui_hi) = AppModel::FONT_UI_RANGE;
        let (mono_lo, mono_hi) = AppModel::FONT_MONO_RANGE;
        body = body.child(
            div()
                .pt(theme::sp5())
                .flex()
                .flex_col()
                .gap(theme::sp2())
                .child(setting_label(self.t("settings.font")))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::sp3())
                        .child(font_row(
                            self.t("settings.font.ui"),
                            self.font_field(FontField::UiFamily, cx),
                            px(210.),
                            self,
                        ))
                        .child(font_row(
                            self.t("settings.font.ui_size"),
                            self.font_field(FontField::UiSize, cx),
                            px(90.),
                            self,
                        ))
                        .child(font_row(
                            self.t("settings.font.mono"),
                            self.font_field(FontField::MonoFamily, cx),
                            px(210.),
                            self,
                        ))
                        .child(font_row(
                            self.t("settings.font.mono_size"),
                            self.font_field(FontField::MonoSize, cx),
                            px(90.),
                            self,
                        )),
                )
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::fg_dark())
                        .child(format!(
                            "{}（{}–{} / {}–{}）",
                            self.t("settings.font.hint"),
                            ui_lo as i32,
                            ui_hi as i32,
                            mono_lo as i32,
                            mono_hi as i32
                        )),
                ),
        );
        body.into_any()
    }

    /// 通用栏：界面语言 + 工作区目录。
    fn settings_general_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let zh = self.settings.locale != "en";
        let ws = self.settings.workspace_root.display().to_string();
        div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp5())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp4())
                    .child(
                        div()
                            .w(px(96.))
                            .child(setting_label(self.t("settings.general.language"))),
                    )
                    .child(chip(
                        "lang-zh",
                        "中文",
                        zh,
                        cx.listener(|this, _, _w, cx| this.set_locale(Locale::Zh, cx)),
                    ))
                    .child(chip(
                        "lang-en",
                        "English",
                        !zh,
                        cx.listener(|this, _, _w, cx| this.set_locale(Locale::En, cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp4())
                    .child(
                        div()
                            .w(px(96.))
                            .child(setting_label(self.t("settings.general.workspace"))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(theme::font_body()))
                            .text_color(theme::fg_normal())
                            .child(SharedString::from(ws)),
                    )
                    .child(chip(
                        "ws-choose",
                        self.t("settings.general.choose_dir"),
                        false,
                        cx.listener(|this, _, _w, cx| this.choose_workspace(cx)),
                    )),
            )
            .into_any()
    }

    /// 网络栏：代理 + 单请求超时（与发请求共用）。
    fn settings_network_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.ensure_net_fields(cx);
        let Some(dlg) = &self.net_dialog else {
            return div().into_any();
        };
        let (proxy_field, timeout_field) = (dlg.proxy.clone(), dlg.timeout.clone());
        let error = dlg.error.clone();
        let current_proxy = self
            .settings
            .proxy
            .clone()
            .unwrap_or_else(|| self.t("proxy.none").to_string());
        let current_timeout = crate::settings::timeout(&self.settings);
        let mut card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(setting_label(self.t("proxy.hint")))
            .child(proxy_field)
            .child(setting_label(self.t("net.timeout_hint")))
            .child(div().w(px(120.)).child(timeout_field))
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(SharedString::from(format!(
                        "{}{} · {}{}s",
                        self.t("proxy.current"),
                        current_proxy,
                        self.t("net.timeout_short"),
                        current_timeout
                    ))),
            );
        if let Some(e) = error {
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::red())
                    .child(SharedString::from(e)),
            );
        }
        card.child(
            div()
                .pt(theme::sp2())
                .flex()
                .gap(theme::sp3())
                .child(
                    div()
                        .id("net-clear")
                        .h(theme::control_h())
                        .flex()
                        .items_center()
                        .px(theme::sp3())
                        .text_size(px(theme::font_body()))
                        .text_color(theme::fg_dim())
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::fg_normal()))
                        .child(self.t("proxy.clear"))
                        .on_click(cx.listener(|this, _, _w, cx| {
                            this.proxy_draft.clear();
                            this.timeout_draft.clear();
                            if let Some(dlg) = &this.net_dialog {
                                dlg.proxy
                                    .update(cx, |f, _cx| f.content = SharedString::from(""));
                                dlg.timeout
                                    .update(cx, |f, _cx| f.content = SharedString::from(""));
                            }
                            this.save_net_settings(cx);
                        })),
                )
                .child(widgets::btn(
                    SharedString::from(self.t("action.save")),
                    widgets::BtnVariant::Accent,
                    {
                        let handle = cx.entity();
                        move |_, _w, app| {
                            handle.update(app, |this, c| this.save_net_settings(c));
                        }
                    },
                )),
        )
        .into_any()
    }

    /// 备份栏：自动备份间隔（重启后生效）+ 立即备份 / 从备份恢复。
    fn settings_backup_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let cur = crate::settings::backup_interval(&self.settings);
        let keep = crate::settings::backup_keep(&self.settings);
        let dir = treq_core::backup::backup_dir_for(&self.settings.workspace_root);
        let mut row = div().flex().items_center().gap(theme::sp4()).child(
            div()
                .w(px(96.))
                .child(setting_label(self.t("settings.backup.interval"))),
        );
        let mut opts: Vec<u64> = vec![0, 30, 60, 1440];
        if !opts.contains(&cur) {
            opts.push(cur);
            opts.sort_unstable();
        }
        for v in opts {
            let label = if v == 0 {
                SharedString::from(self.t("settings.backup.off"))
            } else if v == 1440 {
                SharedString::from(format!("1{}", self.t("settings.backup.day")))
            } else {
                SharedString::from(format!("{}{}", v, self.t("settings.backup.minutes")))
            };
            row = row.child(chip(
                &format!("backup-{}", v),
                label,
                cur == v,
                cx.listener(move |this, _, _w, cx| {
                    this.settings.backup_interval_min = Some(v);
                    crate::settings::save(&this.settings).ok();
                    this.sync_backup_cfg();
                    cx.notify();
                }),
            ));
        }
        // 保留份数：改动立刻按新份数清一遍，不用等线程下次醒来
        let mut keep_row = div().flex().items_center().gap(theme::sp4()).child(
            div()
                .w(px(96.))
                .child(setting_label(self.t("settings.backup.keep"))),
        );
        let mut opts: Vec<usize> = vec![3, 5, 10, 20, 30];
        if !opts.contains(&keep) {
            opts.push(keep);
            opts.sort_unstable();
        }
        for n in opts {
            keep_row = keep_row.child(chip(
                &format!("backup-keep-{}", n),
                SharedString::from(format!("{}{}", n, self.t("settings.backup.files"))),
                keep == n,
                cx.listener(move |this, _, _w, cx| {
                    this.settings.backup_keep = Some(n);
                    crate::settings::save(&this.settings).ok();
                    this.sync_backup_cfg();
                    let dir = treq_core::backup::backup_dir_for(&this.settings.workspace_root);
                    treq_core::backup::prune_backups(&dir, n).ok();
                    cx.notify();
                }),
            ));
        }
        div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp4())
            .child(row)
            .child(keep_row)
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("settings.backup.hint")),
            )
            .child(
                div()
                    .flex()
                    .gap(theme::sp3())
                    .child(chip(
                        "backup-now",
                        self.t("backup.now"),
                        false,
                        cx.listener(|this, _, _w, cx| this.backup_now(cx)),
                    ))
                    .child(chip(
                        "backup-restore",
                        self.t("settings.backup.restore"),
                        false,
                        cx.listener(|this, _, _w, cx| this.open_restore_dialog(cx)),
                    )),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(SharedString::from(format!(
                        "{}{}",
                        self.t("backup.dir"),
                        dir.display()
                    ))),
            )
            .into_any()
    }

    /// 切换下拉框样式（立即生效并持久化）。
    pub fn set_dropdown_style(&mut self, style: DropdownStyle, cx: &mut Context<Self>) {
        if self.settings.dropdown_style != style {
            self.settings.dropdown_style = style;
            crate::settings::save(&self.settings).ok();
            cx.notify();
        }
    }

    /// 「存为变量」对话框：选来源（正文 JSON 路径 / 响应头）、取名、看预览。
    pub(crate) fn save_var_dialog_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dlg) = &self.save_var_dialog else {
            return div().into_any();
        };
        let (path_field, name_field) = (dlg.path.clone(), dlg.name.clone());
        let is_body = matches!(dlg.source, VarSource::Body);
        let error = dlg.error.clone();
        // 预览：命中就显示取到的值（长了截断），不命中就把原因当提示（红字在保存时才强调）
        let preview = self.save_var_preview(cx);
        let name_now = name_field.read(cx).content.trim().to_string();
        let target = self.save_var_target();
        let exists = !name_now.is_empty() && self.save_var_exists(&name_now);
        let label = |t: String| {
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::fg_dim())
                .child(SharedString::from(t))
        };
        let seg = |id: &'static str, text: String, active: bool, source: VarSource| {
            let handle = cx.entity();
            div()
                .id(id)
                .h(theme::control_h())
                .flex()
                .items_center()
                .px(theme::sp3())
                .rounded(px(3.))
                .cursor_pointer()
                .text_size(px(theme::font_small()))
                .bg(if active {
                    theme::bg_sunken()
                } else {
                    theme::bg_base()
                })
                .text_color(if active {
                    theme::fg_bright()
                } else {
                    theme::fg_dim()
                })
                .child(SharedString::from(text))
                .on_click(move |_, _w, app| {
                    handle.update(app, |this, cx| this.set_var_source(source, cx));
                })
        };
        let mut card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(label(self.t("vars.save_hint").to_string()))
            .child(
                div()
                    .flex()
                    .gap(theme::sp1())
                    .child(seg(
                        "sv-body",
                        self.t("vars.source_body").to_string(),
                        is_body,
                        VarSource::Body,
                    ))
                    .child(seg(
                        "sv-header",
                        self.t("vars.source_header").to_string(),
                        !is_body,
                        VarSource::Header,
                    )),
            )
            .child(if is_body {
                label(self.t("vars.path_hint").to_string())
            } else {
                label(self.t("vars.header_hint").to_string())
            })
            .child(path_field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp3())
                    .child(
                        div()
                            .w(px(300.))
                            .child(label(self.t("vars.preview").to_string())),
                    )
                    .child(match &preview {
                        Ok((v, n)) => {
                            let mut text: String = v.chars().take(80).collect();
                            if v.chars().count() > 80 {
                                text.push('…');
                            }
                            let extra = if *n > 1 {
                                format!("{} {}{}", "（", n, self.t("vars.matched_first"))
                            } else {
                                String::new()
                            };
                            div()
                                .font(theme::mono())
                                .text_size(px(theme::font_small()))
                                .text_color(theme::green())
                                .child(SharedString::from(format!("{text}{extra}")))
                        }
                        Err(e) => div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::fg_dark())
                            .child(SharedString::from(e.clone())),
                    }),
            )
            .child(label(self.t("vars.name").to_string()))
            .child(name_field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp2())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(SharedString::from(format!(
                        "{}{}",
                        self.t("vars.target_env"),
                        target
                    )))
                    .when(exists, |d| {
                        d.child(
                            div()
                                .text_color(theme::orange())
                                .child(self.t("vars.will_overwrite")),
                        )
                    }),
            );
        if let Some(e) = error {
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::red())
                    .child(SharedString::from(e)),
            );
        }
        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(
                div()
                    .id("sv-cancel")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.cancel"))
                    .on_click(cx.listener(|this, _, _w, cx| this.close_save_var_dialog(cx))),
            )
            .child(widgets::btn(
                SharedString::from(self.t("vars.save_btn")),
                widgets::BtnVariant::Accent,
                {
                    let handle = cx.entity();
                    move |_, _w, app| {
                        handle.update(app, |this, c| this.confirm_save_var(c));
                    }
                },
            ));
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("vars.save_title")),
            width: px(520.),
            content: card,
            footer,
        }))
    }

    /// 「移动到…」对话框：输入框过滤 + 列表（集合根目录 / 各分组），Enter 选第一项。
    pub(crate) fn move_dialog_render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(d) = &self.move_dialog else {
            return div().into_any();
        };
        let field = d.field.clone();
        let request_name = d.request_name.clone();
        let selected = d.selected;
        let total = self
            .workspace
            .collections
            .iter()
            .map(|c| 1 + c.groups.len())
            .sum::<usize>();
        let targets = self.move_candidates();
        // 自动聚焦（渲染阶段做：actor 拿不到 window）
        if !field.read(cx).focus_handle.is_focused(window) {
            field.update(cx, |f, _| f.focus_handle.focus(window));
        }
        let bar_move_rows = self.scroll_handle("move-rows");
        let mut rows = div()
            .id("move-rows")
            .flex_col()
            .py(theme::sp2())
            .overflow_scroll().track_scroll(&bar_move_rows)
            .max_h(px(320.));
        for (i, t) in targets.iter().enumerate() {
            let (group_id, label) = (t.group_id.clone(), t.label.clone());
            let is_cur = i == selected;
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("move/{}", i)))
                    .h(theme::row_h())
                    .flex()
                    .items_center()
                    .px(theme::sp4())
                    .cursor_pointer()
                    .text_size(px(theme::font_body()))
                    .when(is_cur, |d| {
                        d.relative()
                            .bg(theme::bg_selected_accent())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.))
                                    .top(px(0.))
                                    .h_full()
                                    .w(px(2.))
                                    .bg(theme::primary()),
                            )
                    })
                    .when(!is_cur, |d| d.hover(|d| d.bg(theme::bg_hover())))
                    .text_color(if is_cur {
                        theme::fg_white()
                    } else {
                        theme::fg_normal()
                    })
                    .truncate()
                    .child(SharedString::from(if group_id.is_none() {
                        format!("{}{}", label, self.t("action.move_root"))
                    } else {
                        label
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _w, cx| {
                            this.move_set_selected(i, cx);
                            this.move_pick_selected(cx);
                        }),
                    ),
            );
        }
        if targets.is_empty() {
            rows = rows.child(
                div()
                    .px(theme::sp4())
                    .py(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dark())
                    .child(self.t("move.empty")),
            );
        }
        let card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .truncate()
                    .child(SharedString::from(format!(
                        "{}{}",
                        self.t("move.moving"),
                        request_name
                    ))),
            )
            .child(field);
        let footer = div()
            .flex()
            .items_center()
            .gap(theme::sp3())
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(SharedString::from(format!("{} / {}", targets.len(), total))),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .flex_1()
                    .child(self.t("move.hint")),
            )
            .child(
                div()
                    .id("move-cancel")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.cancel"))
                    .on_click(cx.listener(|this, _, _w, cx| this.close_move_dialog(cx))),
            );
        widgets::backdrop(div().flex().flex_col().items_center().child(widgets::modal(
            widgets::ModalSpec {
                title: SharedString::from(self.t("move.title")),
                width: px(560.),
                content: card.child(rows),
                footer,
            },
        )))
    }

    /// 「查看完整 URL」：工具条里放不下时看全并直接改（单行框，变量已补全提示）。
    pub(crate) fn url_dialog_render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let resolved = self.full_url();
        let bar_url_dlg_text = self.scroll_handle("url-dlg-text");
        let field = self.fields.url.clone();
        if let Some(f) = &field {
            if !f.read(cx).focus_handle.is_focused(window) {
                f.update(cx, |f, _| f.focus_handle.focus(window));
            }
        }
        let mut card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .child(widgets::section_label(self.t("url.edit_label")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .py(theme::sp1())
                    .bg(theme::bg_field())
                    .border_1()
                    .border_color(theme::border_strong())
                    .rounded(px(3.))
                    .when_some(field, |d, f| d.child(f)),
            );
        // 完整 URL 恒展示（编辑后即时刷新）
        card = card.child(
            div()
                .pt(theme::sp3())
                .flex()
                .flex_col()
                .child(widgets::section_label(self.t("editor.section.url_preview")))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(theme::sp3())
                        .p(theme::sp3())
                        .bg(theme::bg_field())
                        .border_1()
                        .border_color(theme::border_strong())
                        .rounded(px(3.))
                        .child(
                            div()
                                .id("url-dlg-text")
                                .flex_1()
                                .min_w_0()
                                .max_h(px(160.))
                                .overflow_scroll().track_scroll(&bar_url_dlg_text)
                                .font(theme::mono())
                                .text_size(px(theme::mono_size()))
                                .line_height(px(theme::line_h()))
                                .text_color(theme::fg_dim())
                                .child(SharedString::from(if resolved.is_empty() {
                                    self.t("url.empty").to_string()
                                } else {
                                    resolved
                                })),
                        )
                        .child(widgets::copy_icon(
                            "dlg-url",
                            cx.listener(|this, _, _w, cx| {
                                let text = this.full_url();
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                                this.toast(this.t("flash.copied_url").to_string(), cx);
                            }),
                        )),
                ),
        );
        let footer = div().flex().gap(theme::sp3()).child(
            div()
                .id("url-dlg-close")
                .h(theme::control_h())
                .flex()
                .items_center()
                .px(theme::sp3())
                .text_size(px(theme::font_body()))
                .text_color(theme::fg_dim())
                .cursor_pointer()
                .hover(|d| d.text_color(theme::fg_normal()))
                .child(self.t("action.close"))
                .on_click(cx.listener(|this, _, _w, cx| this.close_url_dialog(cx))),
        );
        widgets::backdrop(div().flex().flex_col().items_center().child(widgets::modal(
            widgets::ModalSpec {
                title: SharedString::from(self.t("url.title")),
                width: px(680.),
                content: card,
                footer,
            },
        )))
    }

    /// 长值弹框：全文换行显示（可鼠标选中、⌘C 或按钮复制）。
    pub(crate) fn kv_zoom_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let bar_kv_zoom_edit = self.scroll_handle("kv-zoom-edit");
        let Some(z) = &self.kv_zoom else {
            return div().into_any();
        };
        let title = SharedString::from(z.title.clone());
        let lines = crate::model::kv_zoom_lines(&z.text);
        let sel = z.sel;
        let focus = z.focus.clone();
        let field = z.field.clone();
        let bar_kv_zoom_text = self.scroll_handle("kv-zoom-text");
        let mut body = div()
            .id("kv-zoom-text")
            .flex()
            .flex_col()
            .w_full()
            .max_h(px(300.))
            .overflow_scroll().track_scroll(&bar_kv_zoom_text)
            .p(theme::sp3())
            .bg(theme::bg_field())
            .border_1()
            .border_color(theme::border_strong())
            .rounded(px(3.))
            .font(theme::mono())
            .text_size(px(theme::mono_size()))
            .line_height(px(theme::line_h()))
            .text_color(theme::fg_normal())
            .track_focus(&focus)
            .on_key_down(cx.listener(
                |this: &mut AppModel, e: &KeyDownEvent, _w: &mut Window, cx: &mut Context<AppModel>| {
                    if e.keystroke.key == "c" && e.keystroke.modifiers.platform {
                        this.kv_zoom_copy_shortcut(cx);
                    }
                },
            ));
        for (ix, line) in lines.iter().enumerate() {
            // 单色 run 铺满整行，选中高亮直接切它（跟响应区同一个 apply_selection）
            let mut runs = vec![TextRun {
                len: line.len(),
                font: theme::mono(),
                color: theme::fg_normal().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            crate::jsonview::apply_selection(
                &mut runs,
                sel.and_then(|s| s.row_range(ix, line.len())),
                theme::selection(),
            );
            let text = line.clone();
            let weak = cx.weak_entity();
            let hover = move |col: Option<usize>, _e: MouseMoveEvent, _w: &mut Window, cx: &mut App| {
                if let Some(col) = col {
                    let _ = weak.update(cx, |this: &mut AppModel, cx| {
                        this.kv_zoom_drag_to(ix, col, cx);
                    });
                }
            };
            body = body.child(
                div()
                    .w_full()
                    .min_h(px(theme::line_h()))
                    .child(
                        InteractiveText::new(
                            SharedString::from(format!("kz-line/{}", ix)),
                            StyledText::new(text).with_runs(runs),
                        )
                        .on_hover(hover),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _e: &MouseDownEvent, w, cx| {
                            this.kv_zoom_begin(ix, w, cx)
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _e: &MouseUpEvent, _w, cx| this.kv_zoom_end(cx)),
                    ),
            );
        }
        let count = lines.iter().map(|l| l.len() + 1).sum::<usize>().saturating_sub(1);
        let footer = div()
            .flex()
            .items_center()
            .gap(theme::sp3())
            .child(
                div()
                    .flex_1()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(SharedString::from(format!(
                        "{} {}",
                        count,
                        self.t("kv.chars")
                    ))),
            )
            .child(
                div()
                    .id("kv-zoom-copy")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .rounded(px(3.))
                    .bg(theme::primary())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_white())
                    .cursor_pointer()
                    .hover(|d| d.opacity(0.9))
                    .child(self.t("action.copy"))
                    .on_click(cx.listener(|this, _, _w, cx| this.kv_zoom_copy_all(cx))),
            )
            .child(
                div()
                    .id("kv-zoom-close")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.close"))
                    .on_click(cx.listener(|this, _, _w, cx| this.close_kv_zoom(cx))),
            );
        widgets::backdrop(
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(widgets::modal(widgets::ModalSpec {
                    title,
                    width: px(760.),
                    content: div()
                        .px(theme::sp4())
                        .py(theme::sp4())
                        .flex()
                        .flex_col()
                        .child(body)
                        .child(
                            div()
                                .pt(theme::sp3())
                                .text_size(px(theme::font_small()))
                                .text_color(theme::fg_dark())
                                .child(self.t("kv.edit_hint")),
                        )
                        .child(
                            // 编辑框：多行 auto_grow，值里有 \n 也能改；超长行裁在框内
                            div()
                                .id("kv-zoom-edit")
                                .w_full()
                                .mt(theme::sp2())
                                .max_h(px(160.))
                                .overflow_scroll().track_scroll(&bar_kv_zoom_edit)
                                .pt(theme::sp1())
                                .bg(theme::bg_field())
                                .border_1()
                                .border_color(theme::border_strong())
                                .rounded(px(3.))
                                .font(theme::mono())
                                .text_size(px(theme::mono_size()))
                                .child(field),
                        ),
                    footer,
                })),
        )
    }

    pub(crate) fn import_dialog_render(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dlg) = &self.import_dialog else {
            return div().into_any();
        };
        let draft = dlg.draft.clone();
        let error = dlg.error.clone();
        let mut card = div()
            .px(theme::sp4())
            .py(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .child(self.t("import.hint")),
            )
            .child(draft.clone());
        if let Some(e) = error {
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::red())
                    .child(SharedString::from(e)),
            );
        }
        // 通用 Modal 骨架（遮罩 + 卡片）
        let footer = div()
            .flex()
            .gap(theme::sp3())
            .child(
                div()
                    .id("import-cancel")
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::fg_normal()))
                    .child(self.t("action.cancel"))
                    .on_click(cx.listener(|this, _, _w, cx| this.close_import_dialog(cx))),
            )
            .child(widgets::btn(
                SharedString::from(self.t("action.import")),
                widgets::BtnVariant::Accent,
                {
                    let handle = cx.entity();
                    move |_, _w, app| {
                        handle.update(app, |this, c| this.confirm_import(c));
                    }
                },
            ));
        widgets::backdrop(widgets::modal(widgets::ModalSpec {
            title: SharedString::from(self.t("action.import_curl")),
            width: px(440.),
            content: card,
            footer,
        }))
    }
}

