//! 请求编辑区骨架：工具栏（发送按钮/方法/环境）、Body 类型行、编辑器 tab 栏、底部、URL 预览、Docs 段

use super::*;

impl AppModel {
    /// 确保 url/body 输入框实体存在（按需创建，避免每次渲染重建导致失焦）。
    pub(crate) fn ensure_editor_fields(&mut self, cx: &mut Context<Self>) {
        let Some(req) = self.selected_request().cloned() else {
            self.fields = EditorFields::new();
            return;
        };
        let req_id = req.id.clone();
        if self.fields.for_request.as_deref() != Some(req_id.as_str()) {
            self.fields = EditorFields::new();
            self.fields.for_request = Some(req_id.clone());
        }
        if self.fields.url.is_none() {
            let handle = cx.entity();
            let field = TextField::new(
                req.url.clone().into(),
                SharedString::from("https://…"),
                Arc::new(move |s, app| {
                    handle.update(app, |this, cx| this.set_url(s.to_string(), cx));
                }),
                cx,
            );
            field.update(cx, |f, _cx| f.plain = true);
            self.fields.url = Some(field);
        }
        if self.fields.body.is_none() {
            let handle = cx.entity();
            let field = TextField::new(
                req.body.content.clone().into(),
                SharedString::from(""),
                Arc::new(move |s, app| {
                    handle.update(app, |this, cx| this.set_body_content(s.to_string(), cx));
                }),
                cx,
            );
            let json = req.body.kind == BodyKind::Json;
            // 行高挂在输入框上（外层 div 的 .line_height 传不进去，见 TextField::line_height）
            let lh = px(crate::settings::editor_line_h(&self.settings));
            field.update(cx, |f, _cx| {
                f.multiline = true;
                f.auto_grow = true;
                f.json_highlight = json;
                f.line_height = Some(lh);
            });
            self.fields.body = Some(field);
        }
        if self.fields.body_file.is_none() {
            let handle = cx.entity();
            let field = TextField::new(
                req.body.content.clone().into(),
                SharedString::from("/path/to/file"),
                Arc::new(move |s, app| {
                    handle.update(app, |this, cx| this.set_body_content(s.to_string(), cx));
                }),
                cx,
            );
            self.fields.body_file = Some(field);
        }
    }
    /// 请求区顶部工具条：method ▾ | URL | Send ▾（Insomnia 的整条深色块）。
    pub(crate) fn editor_toolbar(
        &mut self,
        req: &RequestItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let method = req.method.clone();

        // URL 输入框：回车发送
        if let Some(f) = &self.fields.url {
            let handle = cx.entity();
            f.update(cx, |f, _cx| {
                if f.on_submit.is_none() {
                    f.on_submit = Some(Arc::new(move |_window, app| {
                        handle.update(app, |this, cx| this.send_request(cx));
                    }));
                }
            });
        }
        let url_field = self.fields.url.clone().expect("url field ensured");

        let (send_label, send_action) = if self.stream_live {
            (self.t("action.stop"), SendAction::Stop)
        } else if self.sending {
            (self.t("action.sending"), SendAction::Busy)
        } else {
            (self.t("action.send"), SendAction::Send)
        };

        // 未定义变量提醒（有才显示）：点开变量面板补
        let missing_vars = {
            let missing = self.missing_vars();
            if missing.is_empty() {
                String::new()
            } else {
                format!(
                    "{}{}：{}",
                    self.t("vars.missing"),
                    missing.len(),
                    missing.join("、")
                )
            }
        };
        div()
            .id("req-toolbar")
            .h(theme::toolbar_h())
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(theme::border_strong())
            .child(
                // method 下拉：定宽，箭头贴右（换方法时位置不跳）
                div()
                    .id("method-cell")
                    .h_full()
                    .w(px(104.))
                    .flex_none()
                    .px(theme::sp4())
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(theme::font_body()))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::method_color(&method))
                    .hover(|d| d.bg(theme::bg_hover()))
                    .child(div().flex_1().min_w_0().truncate().child(SharedString::from(method)))
                    // 箭头用与其它下拉框同一枚 chevron（展开时翻转）
                    .child(widgets::chevron(
                        theme::fg_dim(),
                        self.dropdown_open("method"),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            |this,
                             e: &MouseDownEvent,
                             _w: &mut Window,
                             cx: &mut Context<AppModel>| {
                                this.open_method_menu(e.position, cx)
                            },
                        ),
                    ),
            )
            .child(div().flex_none().w(px(1.)).h_full().bg(theme::border()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .px(theme::sp3())
                    .child(url_field),
            )
            .when(!missing_vars.is_empty(), |d| {
                d.child(
                    div()
                        .id("missing-vars")
                        .flex_none()
                        .h_full()
                        .px(theme::sp3())
                        .flex()
                        .items_center()
                        .gap(theme::sp2())
                        .cursor_pointer()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::orange())
                        .hover(|d| d.bg(theme::bg_hover()))
                        .child("⚠")
                        .child(SharedString::from(missing_vars.clone()))
                        .on_click(cx.listener(|this, _, _w, cx| this.open_vars_panel(cx))),
                )
            })
            .child(
                // 查看完整 URL（仅图标）
                div()
                    .id("view-url")
                    .flex_none()
                    .h_full()
                    .w(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    // 不透明底：URL 输入框的文字会溢出画到邻居上（TextField 不裁剪），盖住它
                    .bg(theme::bg_pane())
                    .hover(|d| d.bg(theme::bg_hover()))
                    .child(widgets::svg_icon(
                        "expand.svg",
                        14.,
                        14.,
                        theme::fg_normal(),
                    ))
                    .on_click(cx.listener(|this, _, _w, cx| this.open_url_dialog(cx))),
            )
            .child(
                // Send 分裂按钮：主键发送，⌄ 出菜单
                div()
                    .id("send-btn")
                    .flex_none()
                    .h_full()
                    .w(theme::send_btn_w())
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::bg_accent())
                    .cursor_pointer()
                    .text_size(px(theme::font_body()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fg_on_accent())
                    .hover(|d| d.bg(theme::bg_accent_hover()))
                    .when(matches!(send_action, SendAction::Stop), |d| {
                        d.bg(theme::red())
                    })
                    .when(matches!(send_action, SendAction::Busy), |d| d.opacity(0.7))
                    .child(SharedString::from(send_label))
                    .on_click({
                        let handle = cx.entity();
                        move |_event: &ClickEvent, _window, app| {
                            handle.update(app, |this, cx| match send_action {
                                SendAction::Send => this.send_request(cx),
                                SendAction::Stop => this.stop_stream(cx),
                                SendAction::Busy => {}
                            });
                        }
                    }),
            )
            .into_any()
    }

    /// Body 页顶部的请求体格式下拉（None/JSON/Text/Form/Form-Data/File/Raw）。
    pub(crate) fn body_kind_row(
        &mut self,
        req: &RequestItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = body_kind_label(self, req.body.kind.clone());
        let hint = match req.body.kind {
            BodyKind::None => self.t("editor.body.none_hint"),
            BodyKind::Form => self.t("editor.body.form_hint"),
            _ => "",
        };
        div()
            .flex()
            .items_center()
            .gap(theme::sp3())
            .pb(theme::sp3())
            .child(widgets::dropdown(
                "body-kind",
                label,
                120.0,
                self.settings.dropdown_style,
                self.dropdown_open("body-kind"),
                cx.listener(
                    |this, e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<AppModel>| {
                        this.open_body_kind_menu(e.position, cx)
                    },
                ),
            ))
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(SharedString::from(hint)),
            )
            .into_any()
    }

    /// tab 条：Body | Query n | Headers（Auth/Docs 已移除）
    pub(crate) fn editor_tabs(&mut self, req: &RequestItem, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.editor_tab;
        let params = req.params.len();
        let headers = req.headers.len();

        div()
            .id("req-tabs")
            .h(theme::tab_h())
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .bg(theme::bg_pane())
            .border_b_1()
            .border_color(theme::border())
            .child(widgets::tab(
                "body",
                self.t("editor.tab.body"),
                tab == EditorTab::Body,
                None,
                cx.listener(|this, _, _w, cx| {
                    this.editor_tab = EditorTab::Body;
                    this.editor_scroll_top();
                    cx.notify();
                }),
            ))
            .child(div().flex_none().w(px(1.)).h(px(18.)).bg(theme::border()))
            .child(widgets::tab(
                "params",
                self.t("editor.tab.query"),
                tab == EditorTab::Params,
                Some(params),
                cx.listener(|this, _, _w, cx| {
                    this.editor_tab = EditorTab::Params;
                    this.editor_scroll_top();
                    cx.notify();
                }),
            ))
            .child(widgets::tab(
                "headers",
                self.t("editor.tab.headers"),
                tab == EditorTab::Headers,
                Some(headers),
                cx.listener(|this, _, _w, cx| {
                    this.editor_tab = EditorTab::Headers;
                    this.editor_scroll_top();
                    cx.notify();
                }),
            ))
            .child(widgets::tab(
                "auth",
                self.t("editor.tab.auth"),
                tab == EditorTab::Auth,
                None,
                cx.listener(|this, _, _w, cx| {
                    this.editor_tab = EditorTab::Auth;
                    this.editor_scroll_top();
                    cx.notify();
                }),
            ))
            .into_any()
    }

    /// 请求区底部：导入 / 命令面板（Insomnia 的 Import from URL / Bulk Edit 位）。
    pub(crate) fn editor_footer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("req-footer")
            .h(theme::footer_h())
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::sp3())
            .px(theme::sp3())
            .bg(theme::bg_base())
            .border_t_1()
            .border_color(theme::border())
            .child(widgets::link(
                "import-curl",
                self.t("editor.footer.import"),
                cx.listener(|this, _, _w, cx| this.open_import_dialog(cx)),
            ))
            .child(widgets::link(
                "footer-codegen",
                self.t("editor.footer.codegen"),
                cx.listener(|this, _, _w, cx| this.open_codegen(cx)),
            ))
            .into_any()
    }

    /// URL PREVIEW 盒（Insomnia：解析后的完整 URL + 复制按钮）。
    pub(crate) fn url_preview_block(
        &mut self,
        req: &RequestItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let url = self.fields.resolved_url.clone();
        let content = if url.is_empty() { req.url.clone() } else { url };
        let _ = req;
        div()
            .flex()
            .flex_col()
            .pb(theme::sp3())
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
                            .flex_1()
                            .min_w_0()
                            .font(theme::mono())
                            .text_size(px(theme::font_small() + 1.))
                            .line_height(px(theme::line_h()))
                            .text_color(theme::fg_dim())
                            .child(SharedString::from(content)),
                    )
                    .child(widgets::copy_icon(
                        "copy-url",
                        cx.listener(|this, _, _w, cx| {
                            let text = this.fields.resolved_url.clone();
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                            this.toast(this.t("flash.copied_text").to_string(), cx);
                        }),
                    )),
            )
            .into_any()
    }
    /// 请求区底部的请求说明（Docs）：一行折叠头 + 展开后的多行编辑器。
    /// 展开状态按请求记忆（存 RequestItem.docs_open，默认展开），切换请求自动跟随。
    pub(crate) fn docs_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.selected_request().map(|r| r.docs_open).unwrap_or(true);
        let field = self.ensure_docs_field(cx);
        let preview = self
            .selected_request()
            .map(|r| r.description.lines().next().unwrap_or("").to_string())
            .unwrap_or_default();
        let mut head = div()
            .id("docs-head")
            .flex()
            .items_center()
            .gap(theme::sp3())
            .h(theme::footer_h())
            .px(theme::sp4())
            .cursor_pointer()
            .text_size(px(theme::font_small()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::fg_dim())
            .hover(|d| d.text_color(theme::fg_bright()))
            .child(div().child(if open { "▾" } else { "▸" }))
            .child(self.t("editor.section.docs"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme::fg_dark())
                    .child(SharedString::from(if open {
                        String::new()
                    } else {
                        preview
                    })),
            )
            .on_click(cx.listener(|this, _, _w, cx| {
                this.update_selected_request(|r| r.docs_open = !r.docs_open, cx);
            }));
        if !open {
            head = head.border_t_1().border_color(theme::border());
            return head.into_any();
        }
        div()
            .id("docs-section")
            .flex()
            .flex_col()
            .flex_none()
            .h(px(180.))
            .border_t_1()
            .border_color(theme::border())
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(theme::sp4())
                    .pb(theme::sp3())
                    .child(field),
            )
            .into_any()
    }
}
