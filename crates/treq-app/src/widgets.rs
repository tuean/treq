//! 基于 gpui 原语的最小控件集：Button、TextField（含 IME 支持）。
//! TextField 改编自 gpui 自带示例 examples/input.rs（Zed 源码，MIT/Apache-2.0）。

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, AssetSource, Bounds, ClickEvent, ClipboardItem, Context, CursorStyle, Div,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    FontWeight, GlobalElementId, InspectorElementId, KeyBinding, LayoutId, ListState, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Rgba, ScrollHandle,
    ShapedLine, SharedString, Stateful, Style, Svg, Task, TextRun, Transformation, UTF16Selection,
    UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, radians, relative, rgb,
    rgba, size, svg,
};
use unicode_segmentation::*;

use crate::settings::DropdownStyle;
use crate::theme;

/// 文本变更回调：(&str 新内容, &mut App)
pub type ChangeCb = Arc<dyn Fn(&str, &mut App) + Send + Sync>;
/// 提交回调（Enter）：(&mut Window, &mut App)
///
/// ⚠️ 回调是在这个 [`TextField`] 自己的 `update` 里跑的，所以**不能在回调里
/// read/update 同一个输入框**：gpui 会以「cannot read TextField while it is
/// already being updated」崩掉（改名框 / 代理超时 / 保存变量 / 导入 curl 都踩过）。
/// 要读自己的内容就 `app.defer(...)` 推迟到本次更新之后再做。
pub type SubmitCb = Arc<dyn Fn(&mut Window, &mut App) + Send + Sync>;

/// 光标闪烁半周期（可见/隐藏各半周期）
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

actions!(
    treq_text,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Submit,
        SuggestUp,
        SuggestDown,
        SuggestAccept,
    ]
);

/// 注册 TextField 需要的全局按键。main.rs 启动时调用一次。
pub fn bind_textfield_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
        KeyBinding::new("enter", Submit, None),
        // `{{` 变量补全：只在输入框上下文里生效（不抢全局上下键）
        KeyBinding::new("up", SuggestUp, Some("TextInput")),
        KeyBinding::new("down", SuggestDown, Some("TextInput")),
        KeyBinding::new("tab", SuggestAccept, Some("TextInput")),
    ]);
}

pub struct TextField {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub last_layout: Option<ShapedLine>,
    pub last_bounds: Option<Bounds<Pixels>>,
    /// 多行模式：全部行（行首 offset, ShapedLine）；单行为最后一行
    pub last_lines: Vec<(usize, ShapedLine)>,
    pub last_line_h: f32,
    /// 单行框为让光标可见而横向滚动的偏移（内容画在 bounds.left() - last_scroll_x）
    pub last_scroll_x: f32,
    pub is_selecting: bool,
    pub on_change: ChangeCb,
    pub on_submit: Option<SubmitCb>,
    /// 多行模式（Body 编辑）：渲染多行、可换行输入；Enter 不触发 submit 而是输入换行
    pub multiline: bool,
    /// 多行模式：高度随内容自动增长（不裁剪、不内部滚动，交给外层容器滚动）
    pub auto_grow: bool,
    /// 按 JSON 语法着色（body 编辑器用）
    pub json_highlight: bool,
    /// 显式行高（多行正文 / 文档用；None = 跟随窗口文本样式）。
    /// 官方继承指望不上：嵌在别的 div 里的实体（view）拿不到外层 div 的
    /// `.line_height(...)`，正文/文档的行高必须挂在输入框自己身上。
    pub line_height: Option<Pixels>,
    /// 多行且需要撑满父容器（Body 编辑区：点空白也能落光标）
    pub fill: bool,
    /// 无边框模式（嵌在外部容器内，如 URL 行）：不画 bg/border
    pub plain: bool,
    /// 光标闪烁：当前是否可见
    pub cursor_visible: bool,
    /// 最近一次输入/移动光标的时间，用于重置闪烁相位
    pub cursor_blink_reset: Instant,
    /// 闪烁定时任务句柄（持有以避免被取消）
    blink_task: Option<Task<()>>,
    /// 可补全的变量名（由 AppModel 在渲染前同步；空的就不弹补全）
    pub var_names: Vec<String>,
    /// `{{` 补全状态：候选项 + 当前选中项 + 正在补全的 range（含 `{{`）
    pub suggest: Option<Suggest>,
}

/// 输入 `{{` 时的补全状态。
#[derive(Clone, Debug, PartialEq)]
pub struct Suggest {
    /// 候选项（已按输入前缀过滤）
    pub items: Vec<String>,
    pub selected: usize,
    /// 要替换的区间（`{{` 起点 .. 光标）
    pub replace: Range<usize>,
}

impl TextField {
    pub fn new(
        content: SharedString,
        placeholder: SharedString,
        on_change: ChangeCb,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| TextField {
            focus_handle: cx.focus_handle(),
            content,
            placeholder,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_lines: vec![],
            last_line_h: 20.0,
            last_scroll_x: 0.0,
            is_selecting: false,
            on_change,
            on_submit: None,
            multiline: false,
            auto_grow: false,
            json_highlight: false,
            line_height: None,
            fill: false,
            plain: false,
            cursor_visible: true,
            cursor_blink_reset: Instant::now(),
            blink_task: None,
            var_names: Vec::new(),
            suggest: None,
        })
    }

    #[allow(dead_code)]
    pub fn with_submit(mut self, f: SubmitCb) -> Self {
        self.on_submit = Some(f);
        self
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    // ---- `{{ 变量 }}` 补全 ----

    /// 内容/光标变化后重算候补（判定与候选都在 core：`vars::complete_at`）。
    /// 每帧在 render 里调用；候选项和位置没变时保留选中项，箭头选择不会被冲掉。
    pub fn refresh_suggest(&mut self) {
        let prev = self.suggest.clone();
        let found = if self.selected_range.is_empty() && !self.var_names.is_empty() {
            treq_core::vars::complete_at(&self.content, self.selected_range.end, &self.var_names, 8)
        } else {
            None
        };
        self.suggest = found.map(|c| {
            let selected = match &prev {
                Some(p) if p.items == c.items && p.replace.start == c.replace.start => {
                    p.selected.min(c.items.len() - 1)
                }
                _ => 0,
            };
            Suggest {
                items: c.items,
                selected,
                replace: c.replace,
            }
        });
    }

    /// 关闭补全（Esc / 失焦 / 内容不再符合条件）。
    pub fn dismiss_suggest(&mut self, cx: &mut Context<Self>) {
        if self.suggest.take().is_some() {
            cx.notify();
        }
    }

    fn suggest_up(&mut self, _: &SuggestUp, _w: &mut Window, cx: &mut Context<Self>) {
        if let Some(sg) = &mut self.suggest {
            let n = sg.items.len();
            sg.selected = if sg.selected == 0 {
                n - 1
            } else {
                sg.selected - 1
            };
            cx.notify();
        }
    }

    fn suggest_down(&mut self, _: &SuggestDown, _w: &mut Window, cx: &mut Context<Self>) {
        if let Some(sg) = &mut self.suggest {
            sg.selected = (sg.selected + 1) % sg.items.len();
            cx.notify();
        }
    }

    /// Tab / Enter：用选中项把 `{{ 前缀` 换成 `{{ 变量 }}`（替换逻辑在 core，带单测）。
    pub fn accept_suggest(&mut self, cx: &mut Context<Self>) {
        let Some(sg) = self.suggest.clone() else {
            return;
        };
        let Some(name) = sg.items.get(sg.selected).cloned() else {
            return;
        };
        let (content, caret) = treq_core::vars::apply_completion(&self.content, &sg.replace, &name);
        self.suggest = None;
        self.content = content.into();
        let caret = clamp_range(&self.content, caret..caret).start;
        self.selected_range = caret..caret;
        self.marked_range = None;
        (self.on_change)(&self.content, cx);
        self.reset_cursor_blink(cx);
        cx.notify();
    }

    fn suggest_accept(&mut self, _: &SuggestAccept, _w: &mut Window, cx: &mut Context<Self>) {
        self.accept_suggest(cx)
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        // IME 组词中不提交（等候选确认）
        if self.marked_range.is_some() {
            return;
        }
        // 补全打开时，Enter = 选中候选项
        if self.suggest.is_some() {
            self.accept_suggest(cx);
            return;
        }
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        if let Some(f) = self.on_submit.as_ref() {
            f(window, cx);
            self.selected_range = self.content.len()..self.content.len();
        }
        self.reset_cursor_blink(cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 双击全选：整段内容选中，敲字直接替换（长 URL / 参数值 / 改名框都省得拖选）
        if event.click_count >= 2 {
            self.is_selecting = false;
            self.selected_range = 0..self.content.len();
            self.selection_reversed = false;
            self.marked_range = None;
            self.reset_cursor_blink(cx);
            cx.notify();
            return;
        }
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            if self.multiline {
                self.replace_text_in_range(None, &text, window, cx);
            } else {
                self.replace_text_in_range(None, &text.replace('\n', " "), window, cx);
            }
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let range = clamp_range(&self.content, self.selected_range.clone());
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[range].to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let range = clamp_range(&self.content, self.selected_range.clone());
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[range].to_string()));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    /// 重置光标闪烁：立即显示并重新计时（输入、移动、点击时调用）。
    fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.cursor_blink_reset = Instant::now();
        cx.notify();
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.reset_cursor_blink(cx);
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds.as_ref() else {
            return 0;
        };
        let lines = &self.last_lines;
        if lines.is_empty() || self.content.is_empty() {
            return 0;
        }
        let line_h = self.last_line_h;
        let rel_y = (position.y - bounds.top()).to_f64() as f32;
        if rel_y <= 0.0 {
            return lines.first().map(|(s, _)| *s).unwrap_or(0);
        }
        let li = ((rel_y / line_h).floor() as usize).min(lines.len() - 1);
        let (start, line) = &lines[li];
        let x = (position.x - bounds.left() + px(self.last_scroll_x)).to_f64() as f32;
        if x < 0.0 {
            return *start;
        }
        *start + line.closest_index_for_x(px(x))
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.reset_cursor_blink(cx);
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf16_to_utf8(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        // 输入法会来问「刚才那段的文本」，内容变短后 range 可能越界（旧版这里切片 panic）
        let range = clamp_range(&self.content, self.range_from_utf16(&range_utf16));
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = clamp_range(
            &self.content,
            range_utf16
                .as_ref()
                .map(|r| self.range_from_utf16(r))
                .or(self.marked_range.clone())
                .unwrap_or(self.selected_range.clone()),
        );
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        (self.on_change)(&self.content, cx);
        self.reset_cursor_blink(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = clamp_range(
            &self.content,
            range_utf16
                .as_ref()
                .map(|r| self.range_from_utf16(r))
                .or(self.marked_range.clone())
                .unwrap_or(self.selected_range.clone()),
        );
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            // new_selected_range 的坐标是 new_text 的（相对选中区起点），不是 content 的
            .map(|r| marked_selected_range(range.start, new_text, r))
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        (self.on_change)(&self.content, cx);
        self.reset_cursor_blink(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start) - px(self.last_scroll_x),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end) - px(self.last_scroll_x),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index =
            last_layout.index_for_x(point.x - line_point.x + px(self.last_scroll_x))?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct TextElement {
    input: Entity<TextField>,
}

struct PrepaintState {
    /// (行首 offset, ShapedLine)
    lines: Vec<(usize, ShapedLine)>,
    cursor: Option<PaintQuad>,
    selection: Option<Vec<PaintQuad>>,
    /// 单行框的横向滚动偏移
    scroll_x: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = if self.input.read(cx).multiline {
            // 高度随行数增长；auto_grow（body 编辑器）不设上限，全部内容都撑开，
            // 由外层滚动容器负责滚动（固定框模式仍限制 300px 后内部裁剪滚动）
            let input = self.input.read(cx);
            let first_line_h = input.line_height.unwrap_or_else(|| window.line_height());
            let line_count = input.content.split('\n').count().max(1) as f32;
            let mut h = line_count * first_line_h.to_f64() as f32;
            if !input.auto_grow {
                h = h.min(300.);
            }
            px(h.max(first_line_h.to_f64() as f32)).into()
        } else {
            let input = self.input.read(cx);
            input.line_height.unwrap_or_else(|| window.line_height()).into()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), theme::fg_dark().into())
        } else {
            (content, theme::fg_normal().into())
        };
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run.clone()
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run.clone()]
        };
        let run_base = run; // 多行分支用（runs 已 clone 不再拥有原值）
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_h = input.line_height.unwrap_or_else(|| window.line_height());

        // 多行：按行 shape；单行：整段一行。
        // 行结构 vec[(行首 offset, ShapedLine)]
        // 多行时 runs 的 len 是整段长度，套到每行会越界（IME 下划线偏移错乱），
        // 所以多行每行单独构造 runs（json_highlight 时按 JSON 词法着色）；
        // 单行保留 marked 下划线逻辑（runs 已按整段构造）
        let mut lines: Vec<(usize, ShapedLine)> = Vec::new();
        let mut offset = 0usize;
        for seg in display_text.split('\n') {
            let line_runs = if input.multiline && input.json_highlight {
                crate::jsonview::editor_runs(seg, &run_base)
            } else if input.multiline {
                vec![TextRun {
                    len: seg.len(),
                    ..run_base.clone()
                }]
            } else {
                runs.clone()
            };
            let line = window.text_system().shape_line(
                SharedString::from(seg.to_string()),
                font_size,
                &line_runs,
                None,
            );
            lines.push((offset, line));
            offset += seg.len() + 1; // +1 换行符
        }

        let line_for_offset = |o: usize| -> usize {
            lines
                .iter()
                .rposition(|(start, _)| *start <= o)
                .unwrap_or(0)
        };

        // 光标在当前行里的 x（未滚动）
        let caret_x = {
            let li = line_for_offset(cursor);
            let (start, line) = &lines[li];
            line.x_for_index(cursor.saturating_sub(*start).min(line.text.len()))
        };
        // 单行框：内容长了就横向滚动让光标可见（多行有自己的滚动容器，不动）
        let scroll_x = if input.multiline {
            px(0.)
        } else {
            px(scroll_x_for(
                caret_x.to_f64() as f32,
                bounds.size.width.to_f64() as f32,
                lines.last().map(|(_, l)| l.width.to_f64() as f32).unwrap_or_default(),
            ))
        };

        // 光标定位
        let cursor = if selected_range.is_empty() && !display_text.is_empty() {
            let li = line_for_offset(cursor);
            Some(fill(
                Bounds::new(
                    point(
                        bounds.left() + caret_x - scroll_x,
                        bounds.top() + line_h * li as f32,
                    ),
                    size(px(2.), line_h),
                ),
                theme::fg_bright(),
            ))
        } else if selected_range.is_empty() && display_text.is_empty() {
            // 光标固定为文本行高，避免 flex stretch 导致的"撑满组件"竖条
            Some(fill(
                Bounds::new(
                    point(bounds.left() - scroll_x, bounds.top()),
                    size(px(2.), line_h),
                ),
                theme::fg_bright(),
            ))
        } else {
            None
        };

        // 选择高亮：按行取交集
        let selection = if selected_range.is_empty() {
            None
        } else {
            let sel_start = selected_range.start;
            let sel_end = selected_range.end;
            let mut quads = Vec::new();
            for (li, (start, line)) in lines.iter().enumerate() {
                let line_end = start + line.text.len();
                let a = sel_start.max(*start);
                let b = sel_end.min(line_end);
                if a < b {
                    let x1 = line.x_for_index(a - start);
                    let x2 = line.x_for_index(b - start);
                    quads.push(fill(
                        Bounds::from_corners(
                            point(
                                bounds.left() + x1 - scroll_x,
                                bounds.top() + line_h * li as f32,
                            ),
                            point(
                                bounds.left() + x2 - scroll_x,
                                bounds.top() + line_h * (li as f32 + 1.),
                            ),
                        ),
                        theme::selection(),
                    ));
                }
            }
            if quads.is_empty() { None } else { Some(quads) }
        };

        PrepaintState {
            lines,
            cursor,
            selection,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(quads) = prepaint.selection.take() {
            for q in quads {
                window.paint_quad(q);
            }
        }
        let lines = std::mem::take(&mut prepaint.lines);
        let line_h = self
            .input
            .read(cx)
            .line_height
            .unwrap_or_else(|| window.line_height());
        for (i, (_, line)) in lines.iter().enumerate() {
            let origin = point(
                bounds.origin.x - prepaint.scroll_x,
                bounds.origin.y + line_h * i as f32,
            );
            line.paint(origin, line_h, window, cx).unwrap();
        }
        let scroll_x = prepaint.scroll_x;
        self.input.update(cx, |input, _cx| {
            input.last_layout = lines.last().map(|(_, l)| l.clone());
            input.last_lines = lines.clone();
            input.last_line_h = line_h.to_f64() as f32;
            input.last_bounds = Some(bounds);
            input.last_scroll_x = scroll_x.to_f64() as f32;
        });
        let cursor_visible = self.input.read(cx).cursor_visible;
        if focus_handle.is_focused(window)
            && cursor_visible
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
    }
}

/// 单行输入框的横向滚动量：把光标留在框内、右侧留 4px；内容比框短则为 0。
///
/// 抽成纯函数是因为这段算错了表现很隐蔽（文字跑出框 / 光标看不见）。
/// utf16 偏移 → 字节偏移（逐字符累加，结果一定落在字符边界上）。
fn utf16_to_utf8(s: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut bytes = 0;
    for ch in s.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += ch.len_utf16();
        bytes += ch.len_utf8();
    }
    bytes
}

/// IME 的 `new_selected_range` 是**相对 new_text** 的 utf16 偏移，
/// 换算成新内容里的字节范围要加上被替换区间的起点 `base`。
fn marked_selected_range(base: usize, new_text: &str, sel_utf16: &Range<usize>) -> Range<usize> {
    let start = utf16_to_utf8(new_text, sel_utf16.start);
    let end = utf16_to_utf8(new_text, sel_utf16.end);
    base + start..base + end
}

/// 把 range 收进内容长度内、并对齐到字符边界；IME / 输入法给的 range 可能越界，
/// 直接拿去切片会 panic（旧版崩在这）。
fn clamp_range(content: &str, range: Range<usize>) -> Range<usize> {
    let len = content.len();
    // 起点往前找最近边界，终点往后找最近边界（本身就是边界就不动）
    let s = range.start.min(len);
    let e = range.end.min(len);
    let start = (0..=s).rev().find(|&i| content.is_char_boundary(i)).unwrap_or(0);
    let end = (e..=len).find(|&i| content.is_char_boundary(i)).unwrap_or(len);
    start..end.max(start)
}

fn scroll_x_for(caret_x: f32, view_w: f32, content_w: f32) -> f32 {
    (caret_x + 4. - view_w).max(0.).min((content_w + 4. - view_w).max(0.))
}

#[cfg(test)]
mod scroll_tests {
    use super::scroll_x_for;

    #[test]
    fn short_content_never_scrolls() {
        // 内容比框短：光标再靠右也不滚
        assert_eq!(scroll_x_for(50., 300., 60.), 0.);
        assert_eq!(scroll_x_for(0., 300., 0.), 0.);
    }

    #[test]
    fn caret_at_end_scrolls_so_that_caret_stays_inside() {
        // 内容 1000，框 300，光标在末尾 → 滚到「末尾 + 4px 正好贴右边缘」
        let sc = scroll_x_for(1000., 300., 1000.);
        assert!((sc - 704.).abs() < 0.01, "got {sc}");
        // 滚完光标落在框内（右侧留 4px）
        assert!(1000. - sc <= 300. - 4. + 0.01);
    }

    #[test]
    fn scroll_clamps_to_content_end() {
        // 光标不会滚过头：最多滚到内容尾部对齐框右边
        let sc = scroll_x_for(1e6, 300., 1000.);
        assert!((sc - 704.).abs() < 0.01, "got {sc}");
    }

    #[test]
    fn caret_at_start_scrolls_back() {
        assert_eq!(scroll_x_for(0., 300., 1000.), 0.);
    }
}

#[cfg(test)]
mod text_range_tests {
    use super::{clamp_range, marked_selected_range, utf16_to_utf8};

    #[test]
    fn utf16_offset_to_byte_offset() {
        assert_eq!(utf16_to_utf8("ab", 1), 1);
        assert_eq!(utf16_to_utf8("请求", 1), 3);
        assert_eq!(utf16_to_utf8("请", 5), 3, "超出长度就停在末尾");
        assert_eq!(utf16_to_utf8("", 3), 0);
    }

    #[test]
    fn ime_selection_is_relative_to_new_text() {
        // base = 已替换区间在新内容里的起点
        assert_eq!(marked_selected_range(0, "q", &(1..1)), 1..1);
        assert_eq!(marked_selected_range(3, "ni", &(2..2)), 5..5);
        // 旧实现拿新 content 换算再叠 base，会把光标推到内容之外
        assert_eq!(marked_selected_range(6, "q", &(1..1)), 7..7);
    }

    #[test]
    fn clamp_keeps_range_inside_content_and_on_boundaries() {
        assert_eq!(clamp_range("abc", 0..3), 0..3);
        assert_eq!(clamp_range("q", 3..5), 1..1, "越界 range 收成空区间，不 panic");
        assert_eq!(clamp_range("请求", 0..2), 0..3, "不对齐就对齐到字符边界");
        assert_eq!(clamp_range("abc", 2..1), 2..2);
        assert_eq!(clamp_range("", 5..9), 0..0);
    }

    #[test]
    fn clamp_output_is_always_sliceable() {
        // 各种刁钻输入（越界 / 倒序 / 落在一个多字节字符中间）都不能产出 panic 的 range
        for content in ["", "a", "中文", "a中b", "🦀x"] {
            for (s, e) in [
                (0, 0),
                (1, 1),
                (2, 2),
                (3, 3),
                (4, 4),
                (9, 9),
                (0, 99),
                (99, 99),
                (2, 1),
                (6, 3),
            ] {
                let r = clamp_range(content, s..e);
                assert!(
                    content.get(r.clone()).is_some(),
                    "content={content:?} {s}..{e} → {r:?}"
                );
            }
        }
    }

    #[test]
    fn ime_sequence_never_escapes_content() {
        // 复现崩溃序列：中文内容 + 输入法逐字母补全，下一次输入的区间用上一次算出的光标
        let mut content = String::from("中文");
        let mut sel = content.len()..content.len();
        for (new_text, sel_utf16) in [("q", 1..1), ("qi", 2..2), ("qin", 3..3)] {
            let range = clamp_range(&content, sel.clone());
            content = content[0..range.start].to_owned() + new_text + &content[range.end..];
            sel = marked_selected_range(range.start, new_text, &sel_utf16);
            assert!(sel.end <= content.len(), "content={content:?} sel={sel:?}");
            assert!(content.is_char_boundary(sel.start));
            assert!(content.is_char_boundary(sel.end));
        }
        // 旧算法在第一次补全后就把光标推到内容之外（3 + 6 = 9 > 7）→ 下一次切片 panic
        let buggy = utf16_to_utf8("中文q", 1) + 6;
        assert!(buggy > "中文q".len(), "buggy={buggy}");
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        // 每帧按光标重算补全（纯状态，不 notify，不会自激）
        self.refresh_suggest();
        if self.blink_task.is_none() {
            self.blink_task = Some(cx.spawn_in(window, async move |this, cx| {
                let mut was_focused = false;
                loop {
                    cx.background_executor().timer(CURSOR_BLINK_INTERVAL).await;
                    let alive = this.update_in(cx, |this, window, cx| {
                        if this.focus_handle.is_focused(window) {
                            if !was_focused {
                                // 刚获得焦点：先显示，再从完整周期开始闪
                                was_focused = true;
                                this.cursor_visible = true;
                                this.cursor_blink_reset = Instant::now();
                                cx.notify();
                            } else if this.cursor_blink_reset.elapsed() >= CURSOR_BLINK_INTERVAL {
                                this.cursor_visible = !this.cursor_visible;
                                cx.notify();
                            }
                        } else {
                            was_focused = false;
                            this.cursor_visible = true;
                        }
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            }));
        }
        div()
            .id("textfield")
            .flex()
            .when(self.multiline, |d| {
                // 固定框：高度固定 + 内部滚动，超出部分被外层裁剪滚动；
                // fill=true 时撑满父容器，整个区域都可点击落光标。
                // auto_grow：高度跟随内容（不裁剪），整块交给外层滚动容器
                d.flex_col()
                    .items_start()
                    .line_height(theme::control_h())
                    // auto_grow：高度跟随内容，长行不画到父容器外（弹框里的长值编辑框）
                    .when(self.auto_grow, |d| {
                        d.flex_none().min_h(theme::sp6()).overflow_hidden()
                    })
                    .when(!self.auto_grow, |d| {
                        d.when(self.fill, |d| d.flex_1().min_h(theme::sp6()).h_full())
                            .when(!self.fill, |d| d.h(px(160.)))
                            .overflow_y_scroll()
                    })
            })
            .when(!self.multiline, |d| {
                d.h(theme::control_h())
                    .items_center()
                    .line_height(px(theme::input_line_h()))
                    // 单行框：超出框的内容不画出去（长值会糊到邻居身上）
                    .overflow_hidden()
            })
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::suggest_up))
            .on_action(cx.listener(Self::suggest_down))
            .on_action(cx.listener(Self::suggest_accept))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .px(theme::sp3())
            .when(!self.plain, |d| {
                d.bg(theme::bg_input())
                    .border_1()
                    .border_color(if focused {
                        theme::primary()
                    } else {
                        theme::border_input()
                    })
                    .rounded_md()
            })
            .text_size(px(theme::font_body()))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
// ===================== 组件库 =====================

/// 按钮变体。
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum BtnVariant {
    /// 默认（surfaceHighlight 底 + 边框）
    Default,
    /// 主操作（品牌紫填充）
    Accent,
    /// 危险（红字/红边）
    Danger,
}

/// 通用按钮。
#[allow(dead_code)]
pub fn btn(
    label: SharedString,
    variant: BtnVariant,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (bg, hover_bg, fg, border) = match variant {
        BtnVariant::Default => (
            theme::bg_button(),
            theme::bg_hover(),
            theme::fg_normal(),
            theme::border_strong(),
        ),
        BtnVariant::Accent => (
            theme::bg_accent(),
            theme::bg_accent_hover(),
            theme::fg_on_accent(),
            theme::bg_accent(),
        ),
        BtnVariant::Danger => (
            theme::bg_button(),
            theme::bg_selected(),
            theme::red(),
            theme::border_strong(),
        ),
    };
    div()
        .id(SharedString::from(format!("btn/{}", label)))
        .flex_none()
        .h(theme::control_h())
        .flex()
        .items_center()
        .px(theme::sp3())
        .bg(bg)
        .hover(move |d| d.bg(hover_bg))
        .active(|d| d.opacity(0.85))
        .border_1()
        .border_color(border)
        .rounded_sm()
        .cursor_pointer()
        .text_size(px(theme::font_body()))
        .text_color(fg)
        .child(label)
        .on_click(on_click)
}

/// 删除按钮（kv 行 / 树行 / 历史行）：比文字大小一级，带 hover 红底。
pub fn delete_btn(
    id: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    div()
        .id(SharedString::from(format!("del/{}", id)))
        .flex_none()
        .size(theme::control_h())
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .cursor_pointer()
        .text_size(px(15.))
        .text_color(theme::fg_dim())
        .hover(|d| d.bg(theme::bg_hover()).text_color(theme::red()))
        .child("×")
        .on_click(on_click)
}

/// 复选框（kv 行启用开关；截图：浅灰底深色 ✓）。
pub fn checkbox(
    id: String,
    checked: bool,
    on_toggle: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded(px(2.))
        .border_1()
        .when(checked, |d| {
            d.bg(theme::fg_normal())
                .border_color(theme::fg_normal())
                .child(
                    div()
                        .text_size(px(10.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme::bg_pane())
                        .child("✓"),
                )
        })
        .when(!checked, |d| d.border_color(theme::border_strong()))
        .on_mouse_down(MouseButton::Left, on_toggle)
}

/// 方法标签（Insomnia 风格：小块底 + 方法色文字，树行/工具条共用）。
pub fn method_tag(method: &str) -> impl IntoElement {
    let color = theme::method_color(method);
    div()
        .flex_none()
        .w(px(44.))
        .h(theme::sp4() + theme::sp1())
        .px(theme::sp2())
        .flex()
        .items_center()
        .justify_center()
        .bg(theme::bg_button())
        .rounded(px(3.))
        .text_size(px(9.))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(SharedString::from(method.to_uppercase()))
}

/// 方法微信息（历史行等窄处用，无底色）。
#[allow(dead_code)]
pub fn method_badge(method: &str) -> impl IntoElement {
    let color = theme::method_color(method);
    div()
        .flex_none()
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(SharedString::from(method.to_uppercase()))
}

// ===================== Insomnia 风格零件 =====================

/// 状态 pill 色系。
#[derive(Clone, Copy, PartialEq)]
pub enum PillTone {
    Success,
    Warning,
    Danger,
    Info,
    /// 次要信息（耗时/大小）：#3b3b3b 底
    Neutral,
    /// 更弱信息（时间）：#212121 底
    Dark,
}

fn pill_colors(tone: PillTone) -> (Rgba, Rgba) {
    match tone {
        PillTone::Success => (theme::bg_success(), theme::fg_white()),
        PillTone::Warning => (theme::orange(), theme::bg_sunken()),
        PillTone::Danger => (theme::red(), theme::fg_white()),
        PillTone::Info => (theme::blue(), theme::bg_sunken()),
        PillTone::Neutral => (theme::bg_button(), theme::fg_normal()),
        PillTone::Dark => (theme::bg_sunken(), theme::fg_dim()),
    }
}

/// 只读 pill（状态码 / 耗时 / 大小）。
pub fn pill(id: &str, text: impl Into<SharedString>, tone: PillTone) -> impl IntoElement {
    let (bg, fg) = pill_colors(tone);
    div()
        .id(SharedString::from(format!("pill/{}", id)))
        .flex_none()
        .h(px(22.))
        .px(theme::sp3())
        .flex()
        .items_center()
        .gap(theme::sp2())
        .bg(bg)
        .rounded(px(3.))
        .text_size(px(theme::font_small() + 1.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .child(text.into())
}

/// 可点 pill（右侧可带 ⌄ 箭头），如「5 Hours Ago ▾」。
pub fn pill_btn(
    id: &str,
    text: impl Into<SharedString>,
    tone: PillTone,
    chevron: bool,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (bg, fg) = pill_colors(tone);
    div()
        .id(SharedString::from(format!("pillbtn/{}", id)))
        .flex_none()
        .h(px(22.))
        .px(theme::sp3())
        .flex()
        .items_center()
        .gap(theme::sp2())
        .bg(bg)
        .rounded(px(3.))
        .cursor_pointer()
        .text_size(px(theme::font_small() + 1.))
        .text_color(fg)
        .hover(move |d| d.opacity(0.85))
        .child(text.into())
        .when(chevron, |d| {
            d.child(div().text_size(px(9.)).text_color(fg).child("▾"))
        })
        .on_mouse_down(MouseButton::Left, on_click)
}

/// 文本链接按钮（Add / Delete All / Import from URL）。
pub fn link(
    id: &str,
    label: impl Into<SharedString>,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("link/{}", id)))
        .flex_none()
        .h(px(20.))
        .px(theme::sp2())
        .flex()
        .items_center()
        .rounded(px(3.))
        .cursor_pointer()
        .text_size(px(theme::font_small() + 1.))
        .text_color(theme::fg_dim())
        .hover(|d| d.text_color(theme::fg_bright()).bg(theme::bg_hover()))
        .child(label.into())
        .on_mouse_down(MouseButton::Left, on_click)
}

/// 图标/字形按钮（活动栏、面板角标）；`enabled = false` 时置灰且不响应。
pub fn icon_btn_enabled(
    id: &str,
    glyph: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let base = div()
        .id(SharedString::from(format!("icon/{}", id)))
        .size(theme::control_h())
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .text_size(px(theme::font_small() + 2.))
        .text_color(if enabled {
            theme::fg_dim()
        } else {
            theme::border_strong()
        });
    // 禁用态也要画出字形（只是置灰、不响应点击）
    if !enabled {
        return base.child(glyph.into()).into_any();
    }
    base.cursor_pointer()
        .hover(|d| d.bg(theme::bg_hover()).text_color(theme::fg_bright()))
        .child(glyph.into())
        .on_click(on_click)
        .into_any()
}

/// 图标按钮（默认可用）。
pub fn icon_btn(
    id: &str,
    glyph: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    icon_btn_enabled(id, glyph, true, on_click)
}

/// 下拉框外观（Body 格式、代码生成语言等都用它，保证视觉一致）。
/// 点击回调里自己开菜单（见 AppModel::toggle_popup）。
/// 内置 SVG 素材源（main 里 with_assets 挂上；目前只有下拉箭头）。
pub struct SvgAssets;

impl AssetSource for SvgAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        match path.trim_start_matches('/') {
            "chevron-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/chevron-down.svg"
            )))),
            "expand.svg" => Ok(Some(Cow::Borrowed(include_bytes!("../assets/expand.svg")))),
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

/// 内置图标（走 SvgAssets；颜色随 text_color）。
pub fn svg_icon(path: &'static str, w: f32, h: f32, color: Rgba) -> Svg {
    svg()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .text_color(color)
        .path(path)
}

/// 锐利 SVG 下拉箭头（8×5；颜色随 text_color 渲染，open 时旋转 180°）。
pub fn chevron(color: Rgba, open: bool) -> impl IntoElement {
    svg()
        .w(px(8.))
        .h(px(5.))
        .flex_none()
        .text_color(color)
        .path("chevron-down.svg")
        .when(open, |s| {
            s.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
        })
}

/// 下拉框外观（设置页可选：Classic/Ghost/Outlined/Underline/Pill/Split）。
/// 点击回调里自己开菜单（见 AppModel::toggle_popup）；`open` 用于画打开态。
pub fn dropdown(
    id: &str,
    label: impl Into<SharedString>,
    min_w: f32,
    style: DropdownStyle,
    open: bool,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let label = label.into();
    let base = div()
        .id(SharedString::from(format!("dd/{}", id)))
        .h(theme::control_h())
        .flex()
        .items_center()
        // 固定宽度 + 标签占满剩余空间：箭头始终贴右，同伴控件宽度一致（换文案不跳宽度）
        .w(px(min_w))
        .cursor_pointer()
        .text_size(px(theme::font_body()))
        .on_mouse_down(MouseButton::Left, on_click);
    // 标签（截断）+ 箭头（贴右）：各款式共用同一对内边距，左右对称
    let label_el = || {
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(label.clone())
    };

    match style {
        // A 经典描边：原样式（bg_sunken + 粗描边 + 文字箭头）
        DropdownStyle::Classic => base
            .px(theme::sp3())
            .gap(theme::sp3())
            .bg(theme::bg_sunken())
            .border_1()
            .border_color(theme::border_strong())
            .rounded(px(3.))
            .text_color(theme::fg_normal())
            .hover(|d| d.border_color(theme::primary()))
            .child(label_el())
            .child(
                div()
                    .text_size(px(9.))
                    .text_color(theme::fg_dim())
                    .child("▾"),
            )
            .into_any(),
        // B 幽灵无框：平时只留文字，hover 浮出浅底
        DropdownStyle::Ghost => base
            .px(theme::sp3())
            .gap(theme::sp3())
            .rounded(px(3.))
            .bg(theme::bg_hover())
            .text_color(if open {
                theme::fg_bright()
            } else {
                theme::fg_dim()
            })
            .when(!open, |d| {
                d.bg(gpui::transparent_black())
                    .hover(|d| d.bg(theme::bg_hover()).text_color(theme::fg_normal()))
            })
            .child(label_el())
            .child(chevron(
                if open {
                    theme::primary()
                } else {
                    theme::fg_dark()
                },
                open,
            ))
            .into_any(),
        // C 强化描边（默认）：细描边，hover/打开时描边与箭头染品牌紫，打开底微亮
        DropdownStyle::Outlined => base
            .px(theme::sp3())
            .gap(theme::sp3())
            .bg(if open {
                rgb(0x262626)
            } else {
                theme::bg_sunken()
            })
            .border_1()
            .border_color(if open {
                theme::primary()
            } else {
                theme::border()
            })
            .rounded(px(3.))
            .text_color(theme::fg_normal())
            .when(!open, |d| d.hover(|d| d.border_color(theme::primary())))
            .child(label_el())
            .child(chevron(
                if open {
                    theme::primary()
                } else {
                    theme::fg_dim()
                },
                open,
            ))
            .into_any(),
        // D 下划线页签：无盒，紫色下划线标记开合
        DropdownStyle::Underline => base
            .px(theme::sp3())
            .gap(theme::sp3())
            .border_b_2()
            .border_color(if open {
                theme::primary()
            } else {
                rgba(0x00000000)
            })
            .text_color(if open {
                theme::fg_bright()
            } else {
                theme::fg_dim()
            })
            .when(!open, |d| {
                d.hover(|d| {
                    d.border_color(theme::primary())
                        .text_color(theme::fg_normal())
                })
            })
            .child(label_el())
            .child(chevron(
                if open {
                    theme::fg_normal()
                } else {
                    theme::fg_dim()
                },
                open,
            ))
            .into_any(),
        // E 胶囊：打开时紫底白字
        DropdownStyle::Pill => base
            .px(px(11.))
            .gap(theme::sp2())
            .bg(if open {
                theme::bg_accent()
            } else {
                theme::bg_button()
            })
            .rounded_full()
            .text_color(if open {
                theme::fg_on_accent()
            } else {
                theme::fg_normal()
            })
            .when(open, |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(!open, |d| {
                d.hover(|d| d.bg(theme::border_strong()).text_color(theme::fg_bright()))
            })
            .child(label_el())
            .child(chevron(
                if open {
                    theme::fg_on_accent()
                } else {
                    theme::fg_dim()
                },
                open,
            ))
            .into_any(),
        // F 分离箭头：macOS 原生弹钮，文字区与箭头区之间有分隔线
        DropdownStyle::Split => base
            .gap(px(0.))
            .bg(theme::bg_sunken())
            .border_1()
            .border_color(if open {
                theme::primary()
            } else {
                theme::border_strong()
            })
            .rounded(px(4.))
            .text_color(theme::fg_normal())
            .when(!open, |d| d.hover(|d| d.border_color(theme::primary())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px(theme::sp3())
                    .flex()
                    .items_center()
                    .child(div().flex_1().min_w_0().truncate().child(label.clone())),
            )
            .child(
                div()
                    .h_full()
                    .w(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_color(theme::border())
                    .when(!open, |d| d.hover(|d| d.bg(theme::bg_hover())))
                    .child(chevron(
                        if open {
                            theme::fg_bright()
                        } else {
                            theme::fg_dim()
                        },
                        open,
                    )),
            )
            .into_any(),
    }
}

/// 复制图标（两个重叠方块；不依赖字体，避免缺字形）。
pub fn copy_icon(
    id: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = theme::fg_dim();
    div()
        .id(SharedString::from(format!("icon/{}", id)))
        .size(px(22.))
        .flex_none()
        .relative()
        .rounded(px(3.))
        .cursor_pointer()
        .hover(|d| d.bg(theme::bg_hover()))
        .child(
            div()
                .absolute()
                .left(px(6.))
                .top(px(4.))
                .size(px(9.))
                .border_1()
                .border_color(color)
                .rounded(px(1.)),
        )
        .child(
            div()
                .absolute()
                .left(px(3.))
                .top(px(7.))
                .size(px(9.))
                .border_1()
                .border_color(color)
                .rounded(px(1.)),
        )
        .on_click(on_click)
}

/// 小节标题（URL PREVIEW）。
pub fn section_label(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .pb(theme::sp2())
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::fg_dark())
        .child(text.into())
}

/// 面板 tab（Insomnia：激活项白字加粗，未激活灰字）。
/// 返回具体类型，便于调用方追加（如 tab 内的 ⌄ 下拉）。
pub fn tab(
    id: &str,
    label: impl Into<SharedString>,
    active: bool,
    badge: Option<usize>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("tab/{}", id)))
        .h_full()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .gap(theme::sp2())
        .cursor_pointer()
        .text_size(px(theme::font_body()))
        .when(active, |d| {
            d.text_color(theme::fg_bright())
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!active, |d| {
            d.text_color(theme::fg_dim())
                .hover(|d| d.text_color(theme::fg_normal()))
        })
        .child(label.into())
        .when(badge.is_some_and(|b| b > 0), |d| {
            d.child(
                div()
                    .h(px(15.))
                    .min_w(px(15.))
                    .px(theme::sp2())
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::bg_button())
                    .rounded(px(2.))
                    .text_size(px(10.))
                    .text_color(theme::fg_dim())
                    .child(SharedString::from(badge.unwrap().to_string())),
            )
        })
        .on_click(on_click)
}

/// 通用 Modal 卡片：标题 / 内容 / 底部按钮区（由调用方构造元素）。
#[allow(dead_code)]
pub struct ModalSpec<T: IntoElement, F: IntoElement> {
    pub title: SharedString,
    pub width: Pixels,
    pub content: T,
    pub footer: F,
}

pub fn modal<T: IntoElement, F: IntoElement>(spec: ModalSpec<T, F>) -> AnyElement {
    div()
        .w(spec.width)
        .bg(theme::bg_popup())
        .border_1()
        .border_color(theme::border_strong())
        .rounded_lg()
        .shadow_lg()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(40.))
                .px(theme::sp4())
                .flex()
                .items_center()
                .border_b_1()
                .border_color(theme::border())
                .child(
                    div()
                        .text_size(px(theme::font_body() + 2.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme::fg_bright())
                        .child(spec.title),
                ),
        )
        .child(spec.content)
        .child(
            div()
                .h(px(40.))
                .px(theme::sp4())
                .flex()
                .items_center()
                .justify_end()
                .gap(theme::sp2())
                .border_t_1()
                .border_color(theme::border())
                .child(spec.footer),
        )
        .into_any()
}

/// 全屏遮罩层（Modal 背景）。
pub fn backdrop(child: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .size_full()
        .bg(theme::bg_backdrop())
        .flex()
        .items_center()
        .justify_center()
        .child(child)
        .into_any()
}

// ---- 常显竖滚动条 -----------------------------------------------------------
// gpui 0.2.2 的滚动容器只给滚动条「留位置」不画条（style.scrollbar_width 只影响布局），
// 所以这里自己画一根：延迟绘制（压在所有内容之上），prepaint 阶段读滚动状态
// （此时本帧布局已完成，位置与内容同帧，不会晚一帧）。

const BAR_W: f32 = 7.;
const BAR_INSET: f32 = 3.;
const BAR_MIN: f32 = 22.;

/// 滑块矩形：视口矩形 + 可滚距离 + 当前偏移（gpui 里往下滚是负值）。
/// 返回 None = 不需要画（没得滚 / 视口太小）。
pub fn scroll_thumb_rect(view: Bounds<Pixels>, max_h: f32, off_y: f32) -> Option<Bounds<Pixels>> {
    let vh = f32::from(view.size.height);
    let vw = f32::from(view.size.width);
    if vh < 3. * BAR_MIN || vw < 60. || max_h < 1. {
        return None;
    }
    let track = vh - 2. * BAR_INSET;
    // 滑块长度按「视口 / 内容」等比，太短也要留出抓得住的高度
    let thumb = (track * vh / (vh + max_h)).clamp(BAR_MIN, track);
    let prog = (-off_y / max_h).clamp(0., 1.);
    let x = f32::from(view.origin.x) + vw - BAR_INSET - BAR_W;
    let y = f32::from(view.origin.y) + BAR_INSET + prog * (track - thumb);
    Some(Bounds::new(point(px(x), px(y)), size(px(BAR_W), px(thumb))))
}

/// 变高列表的滑块几何（纯计算）：
/// 进度按「顶部行号 / 可滚行数」，长度按「可见行数估计 / 总行数」。
/// 不用 `max_offset_for_scrollbar()`：变高列表逐行测量，未测部分高度是估计值，
/// 拿它算出来的内容总高会偏小（几千行的正文只剩几百 px），滑块会长得虚高。
pub fn list_thumb_rect_from(
    view: Bounds<Pixels>,
    count: usize,
    top_ix: usize,
    top_off: f32,
    row_h: f32,
) -> Option<Bounds<Pixels>> {
    let vh = f32::from(view.size.height);
    let vw = f32::from(view.size.width);
    if vh < 3. * BAR_MIN || vw < 60. || count < 2 {
        return None;
    }
    let row_h = if row_h > 1. { row_h } else { 20. };
    let vis = (vh / row_h).max(1.);
    let frac = vis / count as f32;
    if frac >= 1. {
        return None; // 一屏放得下 = 没得滚
    }
    let track = vh - 2. * BAR_INSET;
    let thumb = (track * frac).clamp(BAR_MIN, track);
    let scrolled = top_ix as f32 + (top_off / row_h).clamp(0., 1.);
    let prog = (scrolled / (count as f32 - vis).max(1.)).clamp(0., 1.);
    let x = f32::from(view.origin.x) + vw - BAR_INSET - BAR_W;
    let y = f32::from(view.origin.y) + BAR_INSET + prog * (track - thumb);
    Some(Bounds::new(point(px(x), px(y)), size(px(BAR_W), px(thumb))))
}

/// 变高列表（`gpui::list`）当前状态的滑块。
pub fn list_thumb_rect(s: &ListState) -> Option<Bounds<Pixels>> {
    let top = s.logical_scroll_top();
    let row_h = s
        .bounds_for_item(top.item_ix)
        .map(|b| f32::from(b.size.height))
        .unwrap_or(0.);
    list_thumb_rect_from(
        s.viewport_bounds(),
        s.item_count(),
        top.item_ix,
        f32::from(top.offset_in_item),
        row_h,
    )
}

/// 常显竖滚动条元素（配合 `gpui::deferred(...)` 放在根部统一画）。
pub struct VBar {
    get: Box<dyn Fn() -> Option<Bounds<Pixels>>>,
}

impl VBar {
    /// 普通滚动容器（`track_scroll` 的句柄 / uniform_list 的 base_handle）。
    pub fn from_scroll(handle: &ScrollHandle) -> Self {
        let h = handle.clone();
        Self {
            get: Box::new(move || {
                scroll_thumb_rect(
                    h.bounds(),
                    f32::from(h.max_offset().height),
                    f32::from(h.offset().y),
                )
            }),
        }
    }

    /// 变高列表（gpui::list 的 ListState）。
    pub fn from_list(state: &ListState) -> Self {
        let s = state.clone();
        Self {
            get: Box::new(move || list_thumb_rect(&s)),
        }
    }
}

impl Element for VBar {
    type RequestLayoutState = ();
    type PrepaintState = Option<Bounds<Pixels>>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // 不参与布局：真正的矩形在 prepaint 里按滚动状态算
        let mut style = Style::default();
        style.size.width = px(0.).into();
        style.size.height = px(0.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        (self.get)()
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        thumb: &mut Option<Bounds<Pixels>>,
        window: &mut Window,
        _cx: &mut App,
    ) {
        if let Some(rect) = thumb.take() {
            window.paint_quad(fill(rect, theme::scroll_thumb()).corner_radii(px(BAR_W / 2.)));
        }
    }
}

impl IntoElement for VBar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

#[cfg(test)]
mod scroll_thumb_tests {
    use super::{list_thumb_rect_from, scroll_thumb_rect};
    use gpui::{Bounds, point, px, size};

    fn view(w: f32, h: f32) -> Bounds<gpui::Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)))
    }

    #[test]
    fn no_thumb_when_nothing_to_scroll() {
        assert!(scroll_thumb_rect(view(600., 400.), 0., 0.).is_none(), "内容没超出");
        assert!(scroll_thumb_rect(view(600., 20.), 500., 0.).is_none(), "视口太小");
    }

    #[test]
    fn list_thumb_uses_row_index_not_estimated_height() {
        let v = view(600., 600.);
        // 2000 行、每行 20px、视口 600 → 可见 30 行
        let top = list_thumb_rect_from(v, 2000, 0, 0., 20.).unwrap();
        assert!((f32::from(top.size.height) - 22.).abs() < 0.01, "长度按行数算：600*30/2000=9 → 夹到最小 22");
        assert!(f32::from(top.origin.y) < 5.);
        // 滚到最后一屏：滑到底
        let end = list_thumb_rect_from(v, 2000, 1970, 0., 20.).unwrap();
        let track = 600. - 2. * 3.;
        assert!((f32::from(end.origin.y) - (600. - 3. - f32::from(end.size.height))).abs() < 0.01);
        assert!(f32::from(end.origin.y) - f32::from(top.origin.y) > track * 0.9);
        // 中间位置单调
        let mid = list_thumb_rect_from(v, 2000, 1000, 0., 20.).unwrap();
        assert!(f32::from(mid.origin.y) > f32::from(top.origin.y));
        assert!(f32::from(mid.origin.y) < f32::from(end.origin.y));
        // 一屏放得下（5 行 × 20px）→ 不画
        assert!(list_thumb_rect_from(v, 5, 0, 0., 20.).is_none());
        // 行数少但行高估不准（默认 20）：10 行 → 可见 30 > 10 → 也不画
        assert!(list_thumb_rect_from(v, 10, 0, 0., 0.).is_none());
        // 半个屏幕的高度也算进进度（平滑滚动不跳行）
        let a = list_thumb_rect_from(v, 2000, 500, 0., 20.).unwrap();
        let b = list_thumb_rect_from(v, 2000, 500, 10., 20.).unwrap();
        assert!(f32::from(b.origin.y) > f32::from(a.origin.y));
    }

    #[test]
    fn thumb_tracks_offset_from_top_to_bottom() {
        let v = view(600., 400.);
        // 内容 1600 = 视口 400 + 可滚 1200
        let top = scroll_thumb_rect(v, 1200., 0.).unwrap();
        let mid = scroll_thumb_rect(v, 1200., -600.).unwrap();
        let bottom = scroll_thumb_rect(v, 1200., -1200.).unwrap();
        assert!(f32::from(top.origin.y) < f32::from(mid.origin.y));
        assert!(f32::from(mid.origin.y) < f32::from(bottom.origin.y));
        // 滑块长度 = 视口/内容 × 轨道
        let track = 400. - 2. * 3.;
        let want = track * 400. / 1600.;
        assert!((f32::from(top.size.height) - want).abs() < 0.01);
        // 到底时滑块贴轨道下沿
        let end = 400. - 3. - f32::from(bottom.size.height);
        assert!((f32::from(bottom.origin.y) - end).abs() < 0.01);
        // 越界偏移（回弹）夹在两端
        assert_eq!(scroll_thumb_rect(v, 1200., 999.).unwrap().origin.y, top.origin.y);
        assert_eq!(scroll_thumb_rect(v, 1200., -9999.).unwrap().origin.y, bottom.origin.y);
        // 贴右边缘，宽度固定
        assert!((f32::from(top.origin.x) - (600. - 3. - 7.)).abs() < 0.01);
        assert_eq!(top.size.width, px(7.));
    }
}
