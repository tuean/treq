//! 弹出菜单/下拉的渲染（按窗口边界夹取位置 + 整行点击触发 action）

use super::*;

/// 弹出菜单四周至少留的边距
const POPUP_MARGIN: f32 = 6.0;

/// 菜单左上角画在哪：贴右/贴底时往回收；收到小于边距就贴边距
/// （菜单比视口还大时只保证上/左边距，右边反正也放不下）。
pub(crate) fn popup_origin(
    x: f32,
    y: f32,
    menu: (f32, f32),
    viewport: (f32, f32),
) -> (f32, f32) {
    let (menu_w, menu_h) = menu;
    let (vw, vh) = viewport;
    (
        x.min(vw - menu_w - POPUP_MARGIN).max(POPUP_MARGIN),
        y.min(vh - menu_h - POPUP_MARGIN).max(POPUP_MARGIN),
    )
}

impl AppModel {
    /// 弹出菜单：按窗口边界夹取位置（否则贴边/贴底的菜单会被窗口裁掉），
    /// 点击整条菜单行触发 action；点击别处由根节点关闭。
    pub(crate) fn popup_render(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let items = self.popup.items.clone();
        let (menu_w, menu_h) = self.popup.size_hint();
        let vw = window.viewport_size().width.to_f64() as f32;
        let vh = window.viewport_size().height.to_f64() as f32;
        let (x, y) = popup_origin(self.popup.x, self.popup.y, (menu_w, menu_h), (vw, vh));

        let mut menu = div()
            .absolute()
            .left(px(x))
            .top(px(y))
            // gpui 无 z-index：层序 = 绘制顺序，popup 作为根的最后子节点自然在最上
            .min_w(px(menu_w))
            .bg(theme::bg_popup())
            .border_1()
            .border_color(theme::border_strong())
            .rounded(px(4.))
            .py(theme::sp1())
            .shadow_lg();
        for (id, label) in items {
            let action_id = id.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("menu/{}", action_id)))
                    .h(theme::control_h())
                    .flex()
                    .items_center()
                    .px(theme::sp4())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_normal())
                    .hover(|d| d.bg(theme::bg_hover()).text_color(theme::fg_bright()))
                    .cursor_pointer()
                    .child(SharedString::from(label))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _w, cx| {
                            this.menu_action(&action_id, cx);
                        }),
                    ),
            );
        }
        menu.into_any()
    }

    /// 左侧活动栏：logo + 工具入口（Insomnia 的 rail）。
    pub fn rail_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("rail")
            .flex_none()
            .w(px(theme::RAIL_W))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .py(theme::sp4())
            .gap(theme::sp3())
            .bg(theme::bg_base())
            .border_r_1()
            .border_color(theme::border())
            .child(
                div()
                    .id("rail-logo")
                    .size(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(theme::primary())
                    .cursor_pointer()
                    .text_size(px(18.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::fg_white())
                    .child("»")
                    .on_click(cx.listener(|this, _, _w, cx| this.open_quick(cx))),
            )
            .child(widgets::icon_btn_enabled(
                "nav-back",
                "‹",
                self.nav.can_back(),
                cx.listener(|this, _, _w, cx| this.nav_back(cx)),
            ))
            .child(widgets::icon_btn_enabled(
                "nav-forward",
                "›",
                self.nav.can_forward(),
                cx.listener(|this, _, _w, cx| this.nav_forward(cx)),
            ))
            .child(div().flex_1())
            .child(widgets::icon_btn(
                "quick",
                "⌘K",
                cx.listener(|this, _, _w, cx| this.open_quick(cx)),
            ))
            .child(widgets::icon_btn(
                "trash",
                "🗑",
                cx.listener(|this, _, _w, cx| this.open_trash_dialog(cx)),
            ))
            .child(widgets::icon_btn(
                "reload",
                "↻",
                cx.listener(|this, _, _w, cx| this.refresh(cx)),
            ))
    }

    pub fn tree_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("tree")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.tree_w))
            .h_full()
            .bg(theme::bg_base())
            .border_r_1()
            .border_color(theme::border())
            .relative()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(
                    |this: &mut AppModel,
                     e: &MouseDownEvent,
                     _w: &mut Window,
                     cx: &mut Context<AppModel>| {
                        // 空白处右键：新建集合 / 导入 cURL
                        this.toggle_popup(
                            "tree-empty",
                            e.position.x.to_f64() as f32,
                            e.position.y.to_f64() as f32,
                            vec![
                                (
                                    "new-collection".into(),
                                    this.t("action.new_collection").to_string(),
                                ),
                                (
                                    "import-curl".into(),
                                    this.t("action.import_curl").to_string(),
                                ),
                                ("backup-now".into(), this.t("backup.now").to_string()),
                                ("backup-restore".into(), this.t("backup.title").to_string()),
                            ],
                            cx,
                        );
                    },
                ),
            );

        // 顶部：环境变量（Insomnia 的 Base Environment 行）：固定标签 + 当前环境下拉 + 编辑
        let env_label = self
            .active_env()
            .map(|e| e.name.clone())
            .unwrap_or_else(|| self.workspace.base_env.name.clone());
        let env_target = self
            .settings
            .active_environment_id
            .clone()
            .unwrap_or_else(|| "base".into());
        root = root.child(
            div()
                .flex()
                .items_center()
                .h(px(34.))
                .px(theme::sp3())
                .gap(theme::sp3())
                .border_b_1()
                .border_color(theme::border())
                .child(
                    div()
                        .flex_none()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::fg_dark())
                        .child(self.t("side.env_vars")),
                )
                .child(
                    div()
                        .id("env-dropdown")
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(theme::sp2())
                        .cursor_pointer()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::fg_normal())
                        .hover(|d| d.text_color(theme::fg_bright()))
                        // 标签占满剩余宽度：箭头贴右（宽度变化时位置不跳）
                        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(env_label)))
                        .child(widgets::chevron(
                            theme::fg_dim(),
                            self.dropdown_open("env"),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(
                                |this,
                                 e: &MouseDownEvent,
                                 _w: &mut Window,
                                 cx: &mut Context<AppModel>| {
                                    this.open_env_menu(e.position, cx)
                                },
                            ),
                        ),
                )
                .child(widgets::icon_btn(
                    "env-edit",
                    "⚙",
                    cx.listener(move |this, _, _w, cx| {
                        this.open_env_editor(&env_target, cx);
                    }),
                )),
        );

        // 过滤框 + 新建请求
        let search = self.ensure_search_field(cx);
        root = root.child(
            div()
                .flex()
                .items_center()
                .gap(theme::sp2())
                .px(theme::sp3())
                .py(theme::sp3())
                .child(
                    // 边框只有 TextField 自己那层（外层再包一个就是两层框）
                    div()
                        .flex_1()
                        .h(theme::control_h())
                        .flex()
                        .items_center()
                        .child(search),
                )
                .child({
                    // 折叠全部 / 全部展开（有内容时才可用）
                    let expanded = self.any_expanded();
                    let enabled = !self.workspace.collections.is_empty();
                    widgets::icon_btn_enabled(
                        "collapse-all",
                        if expanded { "⊟" } else { "⊞" },
                        enabled,
                        cx.listener(|this, _, _w, cx| this.toggle_collapse_all(cx)),
                    )
                })
                .child(widgets::icon_btn(
                    "new-item",
                    "＋",
                    cx.listener(|this, e: &ClickEvent, _w, cx| {
                        this.open_new_item_menu(e.position(), cx)
                    }),
                )),
        );

        // 扁平行模型 + 虚拟滚动：只构造可见行的元素（1563 行 → 约 30 行/帧）
        self.rebuild_rows();
        let total = self.rows.len();
        let list = uniform_list(
            "tree-list",
            total,
            cx.processor(
                |this: &mut AppModel,
                 range: std::ops::Range<usize>,
                 _w: &mut Window,
                 cx: &mut Context<AppModel>| {
                    let end = range.end.min(this.rows.len());
                    let mut out: Vec<AnyElement> =
                        Vec::with_capacity(end.saturating_sub(range.start));
                    for i in range.start..end {
                        // 行是 Copy 的，只有可见的这 ~30 行才会去取名称字符串
                        let row = this.rows[i];
                        out.push(this.tree_row_element(row, cx));
                    }
                    out
                },
            ),
        )
        .flex_1()
        .min_h_0()
        .track_scroll(self.tree_scroll.clone());
        let tree_base = self.tree_scroll.0.borrow().base_handle.clone();
        self.track_scrollbar("tree-list", &tree_base);

        if self.workspace.collections.is_empty() {
            root = root.child(
                div()
                    .px(theme::sp4())
                    .py(theme::sp3())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("side.no_collections")),
            );
        }
        root = root.child(list);

        // 底部：命令面板入口（Insomnia 底部的 Git Sync 位）
        root = root.child(
            div()
                .flex()
                .items_center()
                .h(theme::footer_h())
                .px(theme::sp3())
                .border_t_1()
                .border_color(theme::border())
                .child(widgets::link(
                    "quick-panel",
                    format!("⌘K  {}", self.t("action.quick_panel")),
                    cx.listener(|this, _, _w, cx| this.open_quick(cx)),
                )),
        );
        let _ = window;
        root
    }
}

#[cfg(test)]
mod popup_pos_tests {
    use super::popup_origin;

    #[test]
    fn keeps_the_menu_inside_the_window() {
        let vp = (1280.0, 833.0);
        let size = (180.0, 120.0);
        // 中间：原地不动
        assert_eq!(popup_origin(100.0, 200.0, size, vp), (100.0, 200.0));
        // 贴右下：往回收，右边/下边各留 6px
        assert_eq!(
            popup_origin(1279.0, 832.0, size, vp),
            (1280.0 - 180.0 - 6.0, 833.0 - 120.0 - 6.0)
        );
        // 越出左上：贴 6px
        assert_eq!(popup_origin(-50.0, -50.0, size, vp), (6.0, 6.0));
        // 菜单比窗口还大：不 panic，仍给左边距
        assert_eq!(popup_origin(10.0, 10.0, (2000.0, 2000.0), vp), (6.0, 6.0));
    }
}
