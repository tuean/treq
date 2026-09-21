//! 键值表：Params/Headers/表单 的行内编辑、multipart 表单字段表

use super::*;

/// 值超过这么多字符就给「展开看全文」按钮（单行框反正放不下）
const KV_INLINE_MAX_CHARS: usize = 40;

impl AppModel {
    pub(crate) fn ensure_kv_field(
        &mut self,
        key: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> Entity<TextField> {
        if let Some(f) = self.fields.kv.get(key) {
            return f.clone();
        }
        let handle = cx.entity();
        let key_owned = key.to_string();
        let field = TextField::new(
            value.into(),
            SharedString::from(""),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| {
                    this.kv_set_any(&key_owned, s.to_string(), cx);
                });
            }),
            cx,
        );
        let lh = px(theme::line_h());
        field.update(cx, |f, _cx| {
            f.plain = true;
            f.line_height = Some(lh);
        });
        self.fields.kv.insert(key.to_string(), field.clone());
        field
    }

    /// kv_set 的统一入口：key = "{which}:{req_id}:{index}:{key|value|desc}"
    pub fn kv_set_any(&mut self, field_key: &str, value: String, cx: &mut Context<Self>) {
        let parts: Vec<&str> = field_key.split(':').collect();
        if parts.len() != 4 {
            return;
        }
        let which = match parts[0] {
            "P" => KvWhich::Params,
            "FD" => KvWhich::FormData,
            _ => KvWhich::Headers,
        };
        let index: usize = parts[2].parse().unwrap_or(0);
        let slot = parts[3];
        self.update_selected_request(
            |r| {
                if which == KvWhich::FormData {
                    if let Some(field) = r.body.form_data.get_mut(index) {
                        if slot == "key" {
                            field.key = value;
                        } else {
                            field.value = value;
                        }
                    }
                } else {
                    let kvs = match which {
                        KvWhich::Params => &mut r.params,
                        _ => &mut r.headers,
                    };
                    if let Some(kv) = kvs.get_mut(index) {
                        match slot {
                            "key" => kv.key = value,
                            "desc" => kv.description = value,
                            _ => kv.value = value,
                        }
                    }
                }
            },
            cx,
        );
    }

    // ===================== 请求编辑区 =====================

    pub fn editor_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(req) = self.selected_request().cloned() else {
            return div()
                .id("editor")
                .flex_1()
                .h_full()
                .bg(theme::bg_pane())
                .child(
                    div()
                        .p(theme::sp5())
                        .text_size(px(theme::font_body()))
                        .text_color(theme::fg_dark())
                        .child(self.t("editor.select_hint")),
                );
        };
        self.ensure_form_data(cx);
        let req = self.selected_request().cloned().unwrap_or(req);
        self.ensure_editor_fields(cx);
        let req_id = req.id.clone();

        // URL 预览
        self.update_preview(cx);

        let toolbar = self.editor_toolbar(&req, cx);
        let tabs = self.editor_tabs(&req, cx);
        let footer = self.editor_footer(cx);
        let docs = self.docs_section(cx);

        let mut content = div()
            .id("editor-content")
            .flex()
            .flex_col()
            .px(theme::sp4())
            .py(theme::sp4())
            .overflow_scroll()
            .flex_1()
            .min_h_0()
            .track_scroll(&self.editor_scroll);
        let bar_editor = self.editor_scroll.clone();
        self.track_scrollbar("editor-kv", &bar_editor);
        match self.editor_tab {
            EditorTab::Params => {
                content = content
                    .child(self.url_preview_block(&req, cx))
                    .child(self.kv_toolbar(KvWhich::Params, cx))
                    .child(self.kv_table(KvWhich::Params, &req.params, cx));
            }
            EditorTab::Auth => {
                content = content.child(self.auth_form(&req, cx));
            }
            EditorTab::Headers => {
                content = content
                    .child(self.kv_toolbar(KvWhich::Headers, cx))
                    .child(self.kv_table(KvWhich::Headers, &req.headers, cx));
            }
            EditorTab::Body => {
                content = content.child(self.body_kind_row(&req, cx));
                match req.body.kind {
                    BodyKind::None => {
                        content = content.child(
                            div()
                                .p(theme::sp3())
                                .text_size(px(theme::font_small()))
                                .text_color(theme::fg_dark())
                                .child(self.t("editor.body.none_hint")),
                        );
                    }
                    BodyKind::File => {
                        let file_field = self
                            .fields
                            .body_file
                            .clone()
                            .expect("body file field ensured");
                        let choose = widgets::btn(
                            SharedString::from(self.t("editor.choose_file")),
                            widgets::BtnVariant::Default,
                            cx.listener(|this, _, _w, cx| this.choose_body_file(cx)),
                        );
                        content = content.child(
                            div()
                                .flex_col()
                                .gap(theme::sp2())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(theme::sp2())
                                        .child(div().flex_1().child(file_field))
                                        .child(choose),
                                )
                                .child(
                                    div()
                                        .text_size(px(theme::font_small()))
                                        .text_color(theme::fg_dark())
                                        .child(self.t("editor.body.file_hint")),
                                ),
                        );
                    }
                    BodyKind::Multipart => {
                        content = content.child(self.form_data_table(&req.body.form_data, cx));
                    }
                    BodyKind::Json => {
                        let body_field = self.fields.body.clone().expect("body field ensured");
                        content = content.child(self.json_body_box(body_field, cx));
                    }
                    _ => {
                        let body_field = self.fields.body.clone().expect("body field ensured");
                        // 行高由输入框自己的 line_height 决定（设置 → 外观）
                        content = content.child(body_field);
                    }
                }
            }
        }

        // 注意：gpui 的 div 默认 display:block，只有 .flex() 才是 flex 容器，
        // 否则子元素的 flex_1 不会分配剩余空间（内容区不会撑开）。
        let pane = div()
            .id("editor")
            .flex()
            .flex_1()
            // gpui/taffy 的 flex item 默认 min-width:auto（= min-content）：
            // 编辑区内部有等宽长文本/固定控件，min-content 会把响应区挤出窗口。
            .min_w_0()
            .h_full()
            .flex_col()
            .bg(theme::bg_pane())
            .child(toolbar)
            .child(tabs)
            .child(content)
            .child(docs)
            .child(footer);
        let _ = (window, req_id);
        pane
    }
    /// JSON 正文框：右下角浮一个「美化」按钮（点一下重排缩进），下方报错行。
    pub(crate) fn json_body_box(
        &mut self,
        field: Entity<TextField>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut box_el = div()
            .relative()
            .flex_col()
            .child(field)
            .child(
                div()
                    .id("body-beautify")
                    .absolute()
                    .bottom(px(7.))
                    .right(px(9.))
                    .h(px(20.))
                    .px(theme::sp3())
                    .flex()
                    .items_center()
                    .rounded(px(3.))
                    .bg(theme::bg_popup())
                    .border_1()
                    .border_color(theme::border())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dim())
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::bg_hover()).text_color(theme::fg_bright()))
                    .child(SharedString::from(self.t("action.beautify")))
                    .on_click(cx.listener(|this, _, _w, cx| this.beautify_body(cx))),
            );
        if let Some(err) = self.body_error.clone() {
            box_el = box_el.child(
                div()
                    .pt(theme::sp2())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::red())
                    .child(SharedString::from(err)),
            );
        }
        box_el.into_any()
    }

    /// kv 表工具条：Add / Delete All / Toggle Description。
    pub(crate) fn kv_toolbar(&mut self, which: KvWhich, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("kv-toolbar")
            .flex()
            .items_center()
            .gap(theme::sp4())
            .pb(theme::sp2())
            .child(widgets::link(
                "kv-add",
                self.t("editor.kv.add"),
                cx.listener(move |this, _, _w, cx| this.kv_add(which, cx)),
            ))
            .child(widgets::link(
                "kv-clear",
                self.t("editor.kv.delete_all"),
                cx.listener(move |this, _, _w, cx| this.kv_clear(which, cx)),
            ))
            .child(widgets::link(
                "kv-desc",
                self.t("editor.kv.toggle_desc"),
                cx.listener(move |this, _, _w, cx| {
                    this.kv_show_desc = !this.kv_show_desc;
                    cx.notify();
                }),
            ))
            .child(div().flex_1())
            .into_any()
    }

    /// 值单元格：长值（单行框放不下）给个「展开看全文」按钮，弹框里换行显示、可选中复制。
    fn value_cell(
        &mut self,
        which: KvWhich,
        index: usize,
        key: &str,
        value: &str,
        field_key: &str,
        field: Entity<TextField>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut cell = div()
            .flex_1()
            .min_w_0()
            .relative()
            .child(underline(field));
        if value.chars().count() > KV_INLINE_MAX_CHARS {
            let which_label = self.t(match which {
                KvWhich::Params => "editor.tab.query",
                KvWhich::Headers => "editor.tab.headers",
                KvWhich::FormData => "editor.tab.multipart",
            });
            let title = if key.is_empty() {
                which_label.to_string()
            } else {
                format!("{} · {}", which_label, key)
            };
            let text = value.to_string();
            let target = field_key.to_string();
            cell = cell.pr(px(18.)).child(
                div()
                    .id(SharedString::from(format!(
                        "zoom/{}/{}/{}",
                        kv_prefix(which),
                        index,
                        key
                    )))
                    .absolute()
                    .right(px(0.))
                    .top(px(0.))
                    .h(theme::control_h())
                    .w(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::bg_hover()))
                    .child(widgets::svg_icon("expand.svg", 12., 12., theme::fg_dim()))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.open_kv_zoom(title.clone(), target.clone(), text.clone(), w, cx)
                    })),
            );
        }
        cell.into_any()
    }

    pub(crate) fn kv_table(
        &mut self,
        which: KvWhich,
        kvs: &[Kv],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let req_id = match &self.selection {
            Some(Selection::Request(id)) => id.clone(),
            _ => String::new(),
        };
        let show_desc = self.kv_show_desc;
        let mut table = div().flex().flex_col();
        for (i, kv) in kvs.iter().enumerate() {
            let prefix_key = format!("{}:{}:{}:key", kv_prefix(which), req_id, i);
            let prefix_val = format!("{}:{}:{}:value", kv_prefix(which), req_id, i);
            let prefix_desc = format!("{}:{}:{}:desc", kv_prefix(which), req_id, i);
            let key_field = self.ensure_kv_field(&prefix_key, kv.key.clone(), cx);
            let val_field = self.ensure_kv_field(&prefix_val, kv.value.clone(), cx);
            let checked = kv.enabled;
            let enabled = kv.enabled;
            let row = div()
                .flex_col()
                .when(enabled, |d| d)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::sp3())
                        .h(theme::kv_row_h())
                        .child({
                            let handle = cx.entity();
                            let w = which;
                            widgets::checkbox(
                                format!("chk/{}/{}", kv_prefix(which), i),
                                checked,
                                move |_m: &MouseDownEvent, _win: &mut Window, app: &mut App| {
                                    handle.update(app, |this, c| this.kv_toggle(w, i, c));
                                },
                            )
                        })
                        .child(div().flex_1().min_w_0().overflow_hidden().child(underline(key_field)))
                        .child(self.value_cell(which, i, &kv.key, &kv.value, &prefix_val, val_field, cx))
                        .child({
                            let id = format!("kv/{}/{}", kv_prefix(which), i);
                            let w = which;
                            widgets::delete_btn(
                                id.clone(),
                                cx.listener(move |this, _, _win, cx| this.kv_remove(w, i, cx)),
                            )
                        }),
                )
                .when(show_desc, |d| {
                    let f = self.ensure_kv_field(&prefix_desc, kv.description.clone(), cx);
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::sp3())
                            .h(theme::kv_row_h())
                            .child(div().flex_none().w(px(14.)))
                            .child(div().flex_1().min_w_0().child(underline(f)))
                            .child(div().flex_none().size(theme::kv_row_h())),
                    )
                });
            table = table.child(row);
        }
        table
    }
    /// multipart/form-data 字段表：勾选启用 / key / 文本-文件切换 / value / 选文件 / 删除。
    pub(crate) fn form_data_table(
        &mut self,
        fields: &[FormField],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let req_id = match &self.selection {
            Some(Selection::Request(id)) => id.clone(),
            _ => String::new(),
        };
        let mut table = div().flex().flex_col().gap(theme::sp2()).pt(theme::sp2());
        for (i, field) in fields.iter().enumerate() {
            let prefix_key = format!("FD:{}:{}:key", req_id, i);
            let prefix_val = format!("FD:{}:{}:value", req_id, i);
            let key_field = self.ensure_kv_field(&prefix_key, field.key.clone(), cx);
            let val_field = self.ensure_kv_field(&prefix_val, field.value.clone(), cx);
            let is_file = field.is_file;
            let row = div()
                .flex()
                .items_center()
                .gap(theme::sp2())
                .child({
                    let handle = cx.entity();
                    widgets::checkbox(
                        format!("chk/FD/{}/{}", req_id, i),
                        field.enabled,
                        move |_m: &MouseDownEvent, _win: &mut Window, app: &mut App| {
                            handle.update(app, |this, c| this.kv_toggle(KvWhich::FormData, i, c));
                        },
                    )
                })
                .child(div().flex_1().child(key_field))
                .child(
                    div()
                        .id(SharedString::from(format!("fd-type/{}/{}", req_id, i)))
                        .flex_none()
                        .h(theme::kv_row_h())
                        .flex()
                        .items_center()
                        .px(theme::sp2())
                        .text_size(px(theme::font_small()))
                        .text_color(if is_file {
                            theme::orange()
                        } else {
                            theme::fg_dim()
                        })
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::fg_bright()))
                        .child(if is_file {
                            self.t("editor.form.file")
                        } else {
                            self.t("editor.form.text")
                        })
                        .on_click(
                            cx.listener(move |this, _, _w, cx| this.form_field_toggle_file(i, cx)),
                        ),
                )
                .child(self.value_cell(
                    KvWhich::FormData,
                    i,
                    &field.key,
                    &field.value,
                    &prefix_val,
                    val_field,
                    cx,
                ))
                .when(is_file, |d| {
                    d.child(
                        div()
                            .id(SharedString::from(format!("fd-choose/{}/{}", req_id, i)))
                            .flex_none()
                            .px(theme::sp2())
                            .text_size(px(theme::font_small()))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|d| d.text_color(theme::primary()))
                            .child("…")
                            .on_click(cx.listener(move |this, _, _w, cx| {
                                this.form_field_choose_file(i, cx)
                            })),
                    )
                })
                .child({
                    let id = format!("fd/{}/{}", req_id, i);
                    widgets::delete_btn(
                        id.clone(),
                        cx.listener(move |this, _, _win, cx| {
                            this.kv_remove(KvWhich::FormData, i, cx)
                        }),
                    )
                });
            table = table.child(row);
        }
        table = table.child(
            div()
                .id(SharedString::from(format!("fd-add/{}", req_id)))
                .mt_1()
                .flex()
                .items_center()
                .gap(theme::sp2())
                .child(
                    div()
                        .id(SharedString::from(format!("fd-add-btn/{}", req_id)))
                        .flex_none()
                        .h(theme::kv_row_h())
                        .flex()
                        .items_center()
                        .px(theme::sp3())
                        .text_size(px(theme::font_small()))
                        .text_color(theme::fg_dim())
                        .border_1()
                        .border_color(theme::border_strong())
                        .rounded(px(3.))
                        .cursor_pointer()
                        .hover(|d| {
                            d.text_color(theme::fg_bright())
                                .border_color(theme::fg_dim())
                        })
                        .child(self.t("editor.form.add"))
                        .on_click(
                            cx.listener(|this, _, _win, cx| this.kv_add(KvWhich::FormData, cx)),
                        ),
                )
                .child(div().flex_1()),
        );
        table
    }

    // ===================== 响应区 =====================

    pub fn response_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = self.response_tab;
        let header_count = self.response.as_ref().map(|r| r.headers.len()).unwrap_or(0);

        let mut root = div()
            .id("response")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.panel_w))
            .h_full()
            .bg(theme::bg_pane())
            .border_l_1()
            .border_color(theme::border());

        root = root.child(self.response_toolbar(cx));
        root = root.child(
            div()
                .id("resp-tabs")
                .h(theme::tab_h())
                .flex_none()
                .w_full()
                .flex()
                .items_center()
                .bg(theme::bg_pane())
                .border_b_1()
                .border_color(theme::border())
                .child(widgets::tab(
                    "preview",
                    self.t("response.tab.preview"),
                    tab == ResponseTab::Body,
                    None,
                    cx.listener(|this, _, _w, cx| {
                        this.response_tab = ResponseTab::Body;
                        cx.notify();
                    }),
                ))
                .child(div().flex_none().w(px(1.)).h(px(18.)).bg(theme::border()))
                .child(widgets::tab(
                    "headers",
                    self.t("response.tab.headers"),
                    tab == ResponseTab::Headers,
                    Some(header_count),
                    cx.listener(|this, _, _w, cx| {
                        this.response_tab = ResponseTab::Headers;
                        cx.notify();
                    }),
                ))
                .child(widgets::tab(
                    "cookies",
                    self.t("response.tab.cookies"),
                    tab == ResponseTab::Cookies,
                    None,
                    cx.listener(|this, _, _w, cx| {
                        this.response_tab = ResponseTab::Cookies;
                        cx.notify();
                    }),
                ))
                .child(widgets::tab(
                    "timeline",
                    self.t("response.tab.timeline"),
                    tab == ResponseTab::Timeline,
                    None,
                    cx.listener(|this, _, _w, cx| {
                        this.response_tab = ResponseTab::Timeline;
                        cx.notify();
                    }),
                ))
                .child(widgets::tab(
                    "history",
                    self.t("response.tab.history"),
                    tab == ResponseTab::History,
                    None,
                    cx.listener(|this, _, _w, cx| {
                        this.response_tab = ResponseTab::History;
                        this.load_history(cx);
                        cx.notify();
                    }),
                )),
        );

        // 内容
        match tab {
            ResponseTab::History => {
                if self.history.is_empty() {
                    root = root.child(empty_hint(self.t("response.history_empty")));
                } else {
                    let bar_resp_history = self.scroll_handle("resp-history");
                    let mut list = div()
                        .id("resp-history")
                        .flex()
                        .flex_col()
                        .py(theme::sp2())
                        .overflow_scroll().track_scroll(&bar_resp_history);
                    let now = treq_core::now_millis();
                    let entries = self.history.clone();
                    for h in &entries {
                        list = list.child(history_row(self, h, now, cx));
                    }
                    root = root.child(list);
                }
            }
            ResponseTab::Body => {
                let resp = self.response.clone();
                match resp {
                    Some(r) => {
                        root = root.child(self.render_response_body(&r, window, cx));
                        root = root.child(self.response_filter_bar(cx));
                    }
                    None => {
                        root = root.child(empty_hint(if self.sending {
                            self.t("action.sending")
                        } else {
                            self.t("response.none")
                        }));
                    }
                }
            }
            ResponseTab::Headers => match self.response.clone() {
                Some(resp) => {
                    let bar_resp_headers = self.scroll_handle("resp-headers");
                    let mut list = div()
                        .id("resp-headers")
                        .flex()
                        .flex_col()
                        .px(theme::sp4())
                        .py(theme::sp2())
                        .overflow_scroll().track_scroll(&bar_resp_headers);
                    for (k, v) in &resp.headers {
                        list = list.child(
                            div()
                                .flex()
                                .items_start()
                                .gap(theme::sp3())
                                .text_size(px(theme::font_small() + 1.))
                                .line_height(px(theme::line_h()))
                                .font(theme::mono())
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(theme::json_key())
                                        .child(SharedString::from(k)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_color(theme::fg_normal())
                                        .child(SharedString::from(v)),
                                ),
                        );
                    }
                    root = root.child(list);
                }
                None => root = root.child(empty_hint(self.t("response.none"))),
            },
            ResponseTab::Cookies => {
                // 用 core 的解析（跟 cookie 罐同一套规则）；带 url 好判断 host-only/域
                let cookies = self
                    .response
                    .as_ref()
                    .map(|r| {
                        treq_core::cookies::from_response_headers(
                            &self.fields.resolved_url,
                            &r.headers,
                            treq_core::cookies::now_unix(),
                        )
                    })
                    .unwrap_or_default();
                if cookies.is_empty() {
                    root = root.child(empty_hint(self.t("response.cookies_empty")));
                } else {
                    let bar_resp_cookies = self.scroll_handle("resp-cookies");
                    let mut list = div()
                        .id("resp-cookies")
                        .flex()
                        .flex_col()
                        .px(theme::sp4())
                        .py(theme::sp2())
                        .overflow_scroll().track_scroll(&bar_resp_cookies);
                    let now = treq_core::cookies::now_unix();
                    for c in cookies {
                        let (name, value, attrs) = (c.name.clone(), c.value.clone(), c.attrs(now));
                        list = list.child(
                            div()
                                .flex()
                                .flex_col()
                                .px(theme::sp4())
                                .py(theme::sp2())
                                .gap(theme::sp1())
                                .border_b_1()
                                .border_color(theme::border())
                                .child(
                                    div()
                                        .flex()
                                        .gap(theme::sp3())
                                        .font(theme::mono())
                                        .text_size(px(theme::font_small() + 1.))
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_color(theme::json_key())
                                                .child(SharedString::from(name)),
                                        )
                                        .child(
                                            div()
                                                .text_color(theme::fg_normal())
                                                .child(SharedString::from(value)),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(theme::font_small()))
                                        .text_color(theme::fg_dark())
                                        .child(SharedString::from(attrs)),
                                ),
                        );
                    }
                    root = root.child(list);
                }
            }
            ResponseTab::Timeline => {
                let resp = self.response.clone();
                match resp {
                    Some(r) => {
                        let ms = r.duration.as_millis() as u64;
                        let bar_resp_timeline = self.scroll_handle("resp-timeline");
                        let mut list = div()
                            .id("resp-timeline")
                            .flex()
                            .flex_col()
                            .gap(theme::sp3())
                            .px(theme::sp4())
                            .py(theme::sp3())
                            .overflow_scroll().track_scroll(&bar_resp_timeline);
                        list = list
                            .child(widgets::section_label(self.t("response.timeline.total")))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(theme::sp3())
                                    .child(
                                        div()
                                            .h(theme::sp3())
                                            .w(px(220.))
                                            .bg(theme::bg_sunken())
                                            .rounded(px(2.))
                                            .child(
                                                div()
                                                    .h_full()
                                                    .w(px(220.
                                                        * (ms.min(1000) as f32 / 1000.).max(0.02)))
                                                    .bg(theme::primary())
                                                    .rounded(px(2.)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(theme::font_small() + 1.))
                                            .text_color(theme::fg_normal())
                                            .child(SharedString::from(fmt_duration_ms(ms))),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(theme::sp4())
                                    .text_size(px(theme::font_small() + 1.))
                                    .text_color(theme::fg_dim())
                                    .child(SharedString::from(format!(
                                        "{} {}",
                                        self.t("response.size"),
                                        fmt_size(r.body.len())
                                    )))
                                    .child(SharedString::from(format!(
                                        "{} {}",
                                        self.t("response.status"),
                                        r.status
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| "—".into())
                                    )))
                                    .child(SharedString::from(format!(
                                        "{} {}",
                                        self.t("response.tab.headers"),
                                        r.headers.len()
                                    ))),
                            );
                        if let Some(err) = &r.error {
                            // 归因：一句人话在前，原始英文链折在下面
                            let mut block = div().flex().flex_col().gap(theme::sp1()).child(
                                div()
                                    .text_size(px(theme::font_small() + 1.))
                                    .text_color(theme::red())
                                    .child(SharedString::from(err.to_string())),
                            );
                            if let Some(raw) = err.raw() {
                                block = block.child(
                                    div()
                                        .font(theme::mono())
                                        .text_size(px(theme::font_small() - 1.))
                                        .text_color(theme::fg_dark())
                                        .child(SharedString::from(treq_core::http::conn_detail(
                                            raw,
                                        ))),
                                );
                            }
                            list = list.child(block);
                        }
                        root = root.child(list);
                    }
                    None => root = root.child(empty_hint(self.t("response.none"))),
                }
            }
        }
        root
    }
}
