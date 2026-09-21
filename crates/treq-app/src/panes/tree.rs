//! 侧栏树：把过滤+折叠后的树摊平成扁平行，行元素（集合/分组/请求/空节点）

use super::*;

impl AppModel {
    /// 把（过滤 + 折叠后的）树摊平成一行数组：只记录**位置**和行状态，
    /// 不复制名称/URL，所以 1400 行也就几十微秒、零堆分配。
    /// 纯逻辑在 [`crate::search::flatten_tree`]（有单测）。
    pub(crate) fn rebuild_rows(&mut self) {
        let q = self.search_query.trim().to_lowercase();
        let flat = crate::search::flatten_tree(
            &self.workspace.collections,
            &q,
            &self.collapsed,
            self.selection.as_ref(),
            self.rename.as_ref().map(|r| &r.target),
        );
        let mut rows = std::mem::take(&mut self.rows);
        rows.clear();
        rows.reserve(flat.len());
        for (at, st) in flat {
            let status = match at.req {
                Some(ri) => {
                    let r = match at.grp {
                        Some(gi) => self.workspace.collections[at.col].groups[gi]
                            .requests
                            .get(ri),
                        None => self.workspace.collections[at.col].requests.get(ri),
                    };
                    r.and_then(|r| self.last_status.get(&r.id))
                        .and_then(|(_, s)| *s)
                }
                None => None,
            };
            rows.push(TreeRow {
                at,
                status,
                selected: st.selected,
                renaming: st.renaming,
                collapsed: st.collapsed,
                depth: st.depth,
            });
        }
        self.rows = rows;
        // Ctrl+K / 历史跳转后：把选中行滚进视野（已可见时不动）
        self.consume_reveal();
    }

    /// 把行位置解析成渲染需要的数据（只对可见行调用）。
    pub(crate) fn resolve_row(
        &self,
        at: RowRef,
    ) -> Option<(RowKind, String, String, String, String)> {
        // (kind, id, 名称, 方法, 所属集合 id)
        let col = self.workspace.collections.get(at.col)?;
        match (at.grp, at.req) {
            (_, Some(ri)) => {
                let r = match at.grp {
                    Some(gi) => col.groups.get(gi)?.requests.get(ri)?,
                    None => col.requests.get(ri)?,
                };
                Some((
                    RowKind::Request,
                    r.id.clone(),
                    r.name.clone(),
                    r.method.clone(),
                    col.id.clone(),
                ))
            }
            (Some(gi), None) => {
                let g = col.groups.get(gi)?;
                Some((
                    RowKind::Group,
                    g.id.clone(),
                    g.name.clone(),
                    String::new(),
                    col.id.clone(),
                ))
            }
            (None, None) => Some((
                RowKind::Collection,
                col.id.clone(),
                col.name.clone(),
                String::new(),
                col.id.clone(),
            )),
        }
    }

    /// 渲染一行（只对可见行调用）。
    pub(crate) fn tree_row_element(&mut self, row: TreeRow, cx: &mut Context<Self>) -> AnyElement {
        let Some((kind, id, label, method, owner)) = self.resolve_row(row.at) else {
            return div().into_any();
        };
        let label = SharedString::from(label);
        let method = SharedString::from(method);
        if row.renaming
            && let Some(RenameState { field, .. }) = &self.rename
        {
            let pad = theme::tree_indent(row.depth);
            return div()
                .flex()
                // 撑满行宽，否则输入框会被挤成一条线（文本被裁掉）
                .w_full()
                .h(theme::row_h())
                .items_center()
                .pl(pad)
                .pr(theme::sp3())
                .child(field.clone())
                .into_any();
        }
        match kind {
            // w_full + 名称列 min_w_0：行铺满侧栏（选中/悬停高亮等宽），长名字收缩出省略号
            RowKind::Collection => div()
                .id(SharedString::from(format!("col/{}", id)))
                .w_full()
                .flex()
                .h(theme::row_h())
                .items_center()
                .px(theme::sp3())
                .gap(theme::sp2())
                .cursor_pointer()
                .text_size(px(theme::font_body()))
                .when(row.selected, |d| d.bg(theme::bg_selected()))
                .child(self.fold_arrow(RowKind::Collection, &id, row.collapsed, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::fg_normal())
                        .child(label.clone()),
                )
                .child(widgets::delete_btn(
                    "col",
                    cx.listener({
                        let del = id.clone();
                        move |this, _, _w, cx| this.delete_collection(&del, cx)
                    }),
                ))
                .on_click({
                    let click = id.clone();
                    cx.listener(move |this, _, _w, cx| this.toggle_collapsed(&click, cx))
                })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let menu = id.clone();
                        move |this: &mut AppModel,
                              e: &MouseDownEvent,
                              _w,
                              cx: &mut Context<AppModel>| {
                            this.open_collection_menu(menu.clone(), e.position, cx);
                        }
                    }),
                )
                .into_any(),
            RowKind::Group => div()
                .id(SharedString::from(format!("group/{}", id)))
                .w_full()
                .flex()
                .h(theme::row_h())
                .items_center()
                .pr(theme::sp3())
                .pl(theme::tree_indent(row.depth))
                .gap(theme::sp2())
                .cursor_pointer()
                .text_size(px(theme::font_body()))
                .when(row.selected, |d| d.bg(theme::bg_selected()))
                .child(self.fold_arrow(RowKind::Group, &id, row.collapsed, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(theme::fg_dim())
                        .child(label.clone()),
                )
                .child(widgets::delete_btn(
                    "group",
                    cx.listener({
                        let (del_g, del_c) = (id.clone(), owner.clone());
                        move |this, _, _w, cx| this.delete_group(&del_c, &del_g, cx)
                    }),
                ))
                .on_click({
                    let click = id.clone();
                    cx.listener(move |this, _, _w, cx| this.toggle_collapsed(&click, cx))
                })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let (menu, owner) = (id.clone(), owner.clone());
                        move |this: &mut AppModel,
                              e: &MouseDownEvent,
                              _w,
                              cx: &mut Context<AppModel>| {
                            this.open_group_menu(menu.clone(), owner.clone(), e.position, cx);
                        }
                    }),
                )
                .into_any(),
            RowKind::Request => {
                let status_text = row.status.map(|s| s.to_string()).unwrap_or_default();
                let status_color = status_color_of(row.status);
                div()
                    .id(SharedString::from(format!("req/{}", id)))
                    .relative()
                    .w_full()
                    .flex()
                    .h(theme::row_h())
                    .items_center()
                    .pr(theme::sp3())
                    .pl(theme::tree_indent(row.depth))
                    .gap(theme::sp3())
                    .cursor_pointer()
                    .text_size(px(theme::font_body()))
                    .when(row.selected, |d| {
                        d.bg(theme::bg_selected()).child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(0.))
                                .h_full()
                                .w(px(2.))
                                .bg(theme::primary()),
                        )
                    })
                    .hover(|d| d.bg(theme::bg_hover()))
                    .child(widgets::method_tag(&method))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if row.selected {
                                theme::fg_bright()
                            } else {
                                theme::fg_dim()
                            })
                            .child(label.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(theme::font_small()))
                            .text_color(status_color)
                            .child(SharedString::from(status_text)),
                    )
                    .child(widgets::delete_btn(
                        "req",
                        cx.listener({
                            let del = id.clone();
                            move |this, _, _w, cx| this.delete_request(&del, cx)
                        }),
                    ))
                    .on_click({
                        let click = id.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.select_request(click.clone(), window, cx);
                        })
                    })
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener({
                            let menu = id.clone();
                            move |this: &mut AppModel,
                                  e: &MouseDownEvent,
                                  _w,
                                  cx: &mut Context<AppModel>| {
                                this.open_request_menu(menu.clone(), e.position, cx);
                            }
                        }),
                    )
                    .into_any()
            }
        }
    }

    /// 折叠箭头（集合/分组行共用）。
    pub(crate) fn fold_arrow(
        &self,
        kind: RowKind,
        id: &str,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let elem_id = match kind {
            RowKind::Collection => format!("col-fold/{}", id),
            _ => format!("group-fold/{}", id),
        };
        div()
            .id(SharedString::from(elem_id))
            .flex_none()
            .w(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(9.))
            .text_color(theme::fg_dim())
            .hover(|d| d.text_color(theme::fg_bright()))
            .child(if collapsed { "▸" } else { "▾" })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let fold = id.to_string();
                    move |this: &mut AppModel,
                          _e: &MouseDownEvent,
                          _w,
                          cx: &mut Context<AppModel>| {
                        this.toggle_collapsed(&fold, cx);
                    }
                }),
            )
            .into_any()
    }
}
