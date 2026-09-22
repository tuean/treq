//! 响应区：工具条（状态/耗时/大小/存文件/复制/存变量）、过滤条、正文（JSON 着色 + 折叠箭头 + 行右键）

use super::*;

impl AppModel {
    /// 响应区工具条：状态 / 耗时 / 大小 pill + 历史下拉（Insomnia 的整条深色块）。
    pub(crate) fn response_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let resp = self.response.clone();
        let ago = self
            .selection
            .as_ref()
            .and_then(|s| match s {
                Selection::Request(id) => self.last_status.get(id).map(|(t, _)| *t),
                _ => None,
            })
            .map(|t| self.ago_label(t, treq_core::now_millis()))
            .unwrap_or_default();

        let mut bar = div()
            .id("resp-toolbar")
            .min_h(px(38.))
            .flex_none()
            .flex()
            // 窄面板：一行放不下就折到下一行（格式化/原始/复制…）
            .flex_wrap()
            .items_center()
            .gap(theme::sp3())
            .py(theme::sp1())
            .px(theme::sp4())
            .bg(theme::bg_sunken())
            .border_b_1()
            .border_color(theme::border_strong());

        match &resp {
            Some(r) => {
                let (tone, text) = match r.status {
                    Some(s) => (
                        status_tone(Some(s)),
                        format!("{} {}", s, r.status_text.trim()),
                    ),
                    None => (PillTone::Danger, self.t("response.error").to_string()),
                };
                bar = bar
                    .child(widgets::pill("status", text, tone))
                    .child(widgets::pill(
                        "duration",
                        fmt_duration(r.duration),
                        PillTone::Neutral,
                    ))
                    .child(widgets::pill(
                        "size",
                        fmt_size(r.body.len()),
                        PillTone::Neutral,
                    ));
            }
            None => {
                bar = bar.child(widgets::pill(
                    "status",
                    self.t("response.title"),
                    PillTone::Dark,
                ));
            }
        }
        bar = bar.child(div().flex_1());
        bar = bar
            .child(view_switch(
                "pretty",
                self.response_pretty,
                self.t("response.view.pretty"),
                cx,
            ))
            .child(view_switch(
                "raw",
                !self.response_pretty,
                self.t("response.view.raw"),
                cx,
            ));
        // 事件流：给一颗「时间」开关（正文左侧显示每个事件的到达时间）
        if resp
            .as_ref()
            .is_some_and(|r| treq_core::http::is_event_stream(&r.headers))
        {
            bar = bar.child(view_switch(
                "sse-time",
                self.sse_show_time,
                self.t("sse.time"),
                cx,
            ));
        }
        // 只有超大响应才给「完整」开关（默认跳过渲染，避免卡住 UI）
        if resp
            .as_ref()
            .is_some_and(|r| crate::jsonview::over_render_limit(&r.body))
        {
            bar = bar.child(view_switch(
                "full",
                self.resp_force_full,
                self.t("response.view.full"),
                cx,
            ));
        }
        // JSON 折叠：只折最外层，「展开全部」一键还原（过滤条有内容时没有折叠可言）
        if !self.resp_fold_ends.is_empty() && crate::fold::has_foldable(&self.resp_fold_ends) {
            let folded_any = !self.resp_folded.is_empty();
            bar = bar.child(view_switch(
                "fold-all",
                folded_any,
                if folded_any {
                    self.t("response.unfold_all")
                } else {
                    self.t("response.fold_all")
                },
                cx,
            ));
        }
        // 响应正文动作：复制原文 / 存文件（大响应只能靠它拿出来）
        if resp.is_some() {
            bar = bar
                .child(widgets::pill_btn(
                    "copy-body",
                    self.t("action.copy"),
                    PillTone::Dark,
                    false,
                    cx.listener(|this, _, _w, cx| this.copy_response_body(cx)),
                ))
                .child(widgets::pill_btn(
                    "save-body",
                    self.t("action.save"),
                    PillTone::Dark,
                    false,
                    cx.listener(|this, _, _w, cx| this.save_response_body(cx)),
                ))
                // 请求链：把响应里的值（token / id）存成环境变量给后面的请求用
                .child(widgets::pill_btn(
                    "save-var",
                    self.t("vars.save_btn"),
                    PillTone::Dark,
                    false,
                    cx.listener(|this, _, _w, cx| this.open_save_var_dialog(cx)),
                ));
        }
        if !ago.is_empty() {
            bar = bar.child(widgets::pill_btn(
                "ago",
                ago,
                PillTone::Dark,
                true,
                cx.listener(
                    |this, e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<AppModel>| {
                        this.open_history_menu(e.position, cx)
                    },
                ),
            ));
        }
        bar.into_any()
    }

    /// 响应底部过滤条（Insomnia：JSONPath 过滤）。
    pub(crate) fn response_filter_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let field = self.ensure_resp_filter_field(cx);
        div()
            .id("resp-filter")
            .h(theme::footer_h())
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::sp3())
            .px(theme::sp4())
            .bg(theme::bg_base())
            .border_t_1()
            .border_color(theme::border())
            .child(div().flex_1().min_w_0().font(theme::mono()).child(field))
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child("?"),
            )
            .into_any()
    }

    /// 响应体显示数据（pretty/filter 之后的行 + 是否 JSON 着色）。
    /// 每次响应或过滤条件变化才重算一次，不再每帧 pretty-print + 逐行 tokenize。
    pub(crate) fn ensure_resp_lines(&mut self, resp: &ResponseData) {
        let locale = self.locale();
        let key = (
            self.resp_gen,
            self.resp_filter.trim().to_string(),
            self.response_pretty,
            self.resp_force_full,
            locale == crate::i18n::Locale::Zh,
        );
        if self.resp_lines_key.as_ref() == Some(&key) {
            return;
        }
        let ct = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.clone());
        let tr = move |k: &str| crate::i18n::tr(locale, k).to_string();
        let (lines, json, notice) = crate::jsonview::body_lines(
            &resp.body,
            ct.as_deref(),
            &self.resp_filter,
            self.response_pretty,
            self.resp_force_full,
            &tr,
        );
        self.resp_all_lines = lines;
        self.resp_lines_json = json;
        self.resp_lines_notice = notice;
        self.resp_lines_key = Some(key);
        // JSON 正文才配折叠；过滤条有内容时看到的是查询结果，不折
        let foldable = json && self.folding_active();
        self.resp_fold_ends = if foldable {
            crate::fold::group_ends(&self.resp_all_lines)
        } else {
            Vec::new()
        };
        if !foldable {
            self.resp_folded.clear();
        } else {
            // 换了响应（行数对不上就整个清掉）
            let n = self.resp_all_lines.len();
            self.resp_folded.retain(|i| *i < n);
        }
        self.rebuild_visible_lines();
    }

    pub(crate) fn render_response_body(
        &mut self,
        resp: &ResponseData,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_resp_lines(resp);
        // 变高列表的行高依赖可用宽度：窗口/面板宽度变了要重新测量，
        // 否则换行高度过期会导致滚动位置错乱（gpui::List 的要求）。
        let w_key = (
            window.viewport_size().width.to_f64() as f32,
            self.settings.panel_width.unwrap_or(theme::DEFAULT_PANEL_W),
        );
        if w_key != self.resp_list_w {
            self.resp_list_w = w_key;
            self.resp_list.reset(self.resp_lines.len());
        }
        let pad_top = if self.resp_lines_notice.is_some() {
            0.
        } else {
            1.
        };
        let _ = pad_top;
        if let Some(notice) = self.resp_lines_notice.clone() {
            // 「大响应跳过渲染」是提示不是报错：只有过滤/解析失败才标红
            let info = crate::jsonview::over_render_limit(&resp.body)
                && self.resp_filter.trim().is_empty();
            return if info {
                info_hint(&notice)
            } else {
                error_hint(&notice)
            };
        }
        // 请求失败（连不上/超时）时正文区通常是空的 —— 别让 Preview 一片空白：
        // 先给归因（一句人话 + 原始链），有 body 再往下接正文。
        if let Some(err) = &resp.error {
            let mut block = div()
                .id("resp-error")
                .flex()
                .flex_col()
                .gap(theme::sp1())
                .px(theme::sp4())
                .py(theme::sp3())
                .child(
                    div()
                        .text_size(px(theme::mono_size()))
                        .text_color(theme::red())
                        .child(SharedString::from(err.to_string())),
                );
            if let Some(raw) = err.raw() {
                block = block.child(
                    div()
                        .font(theme::mono())
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::fg_dark())
                        .child(SharedString::from(treq_core::http::conn_detail(raw))),
                );
            }
            if resp.body.is_empty() {
                return block.into_any();
            }
            return div()
                .flex()
                .flex_col()
                .size_full()
                .min_h_0()
                .child(block)
                .child(self.render_body_lines(cx))
                .into_any();
        }
        self.render_body_lines(cx)
    }

    /// 正文行（变高虚拟列表）。抽出来是为了让失败响应也能先把错误块画在上面。
    pub(crate) fn render_body_lines(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // 变高虚拟列表：只对可见行做 tokenize + 构造元素（长响应不再每帧全量重建）
        let resp_list = self.resp_list.clone();
        self.track_list_scrollbar(&resp_list);
        let json = self.resp_lines_json;
        let state = self.resp_list.clone();
        let count = self.resp_lines.len();
        let list = list(
            state,
            cx.processor(
                move |this: &mut AppModel,
                      ix: usize,
                      _w: &mut Window,
                      cx: &mut Context<AppModel>| {
                    let Some(line) = this.resp_lines.get(ix) else {
                        return div().into_any();
                    };
                    if json {
                        // 行号 / 折叠都按「折叠前的原始行」来，折了也认得出是第几行
                        let orig = this.resp_visible_idx.get(ix).copied().unwrap_or(ix);
                        let foldable = matches!(
                            this.resp_fold_ends.get(orig),
                            Some(Some(end)) if *end > orig
                        );
                        let folded = foldable && this.resp_folded.contains(&orig);
                        let arrow = if foldable {
                            let on_click = cx.listener(
                                move |this: &mut AppModel,
                                      _: &ClickEvent,
                                      _w: &mut Window,
                                      cx: &mut Context<AppModel>| {
                                    this.toggle_fold(orig, cx)
                                },
                            );
                            div()
                                .id(SharedString::from(format!("fold/{}", orig)))
                                .flex_none()
                                .w(theme::sp4())
                                .text_color(theme::fg_dim())
                                .cursor_pointer()
                                .hover(|d| d.text_color(theme::fg_normal()))
                                .child(SharedString::from(if folded { "▸" } else { "▾" }))
                                .on_click(on_click)
                                .into_any()
                        } else {
                            div().flex_none().w(theme::sp4()).into_any()
                        };
                        let sel = this.resp_sel.and_then(|s| s.row_range(ix, line.len()));
                        let mut runs = crate::jsonview::line_runs(line);
                        crate::jsonview::apply_selection(&mut runs, sel, theme::selection());
                        let body = resp_interactive(line.clone(), ix, runs, cx).into_any();
                        let time = sse_time_for(&this.sse_marks, this.sse_show_time, orig);
                        let mut row = json_line(time, orig + 1, arrow, folded, body)
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(
                                    move |this: &mut AppModel,
                                          e: &MouseDownEvent,
                                          _w: &mut Window,
                                          cx: &mut Context<AppModel>| {
                                        this.open_resp_line_menu(orig, e.position, cx);
                                    },
                                ),
                            );
                        // 选中：左键按下定起点，拖动中更新终点（列由 hover 报上来）
                        row = row
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this: &mut AppModel,
                                          e: &MouseDownEvent,
                                          w: &mut Window,
                                          cx: &mut Context<AppModel>| {
                                        match e.click_count {
                                            2 => this.resp_double_click(ix, cx),
                                            3.. => this.resp_select_all(cx),
                                            _ => this.resp_sel_begin(ix, w, cx),
                                        }
                                    },
                                ),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this: &mut AppModel, _, _, cx| {
                                    this.resp_sel_end(cx);
                                }),
                            );
                        row.into_any()
                    } else {
                        let sel = this.resp_sel.and_then(|s| s.row_range(ix, line.len()));
                        let mut runs = vec![TextRun {
                            len: line.len(),
                            font: theme::mono(),
                            color: theme::fg_normal().into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }];
                        crate::jsonview::apply_selection(&mut runs, sel, theme::selection());
                        let body = resp_interactive(line.clone(), ix, runs, cx).into_any();
                        div()
                            .flex()
                            .w_full()
                            .font(theme::mono())
                            .text_size(px(theme::mono_size()))
                            .line_height(px(theme::line_h()))
                            .when(this.sse_show_time && !this.sse_marks.is_empty(), |d| {
                                d.child(time_cell(sse_time_for(
                                    &this.sse_marks,
                                    this.sse_show_time,
                                    ix,
                                )))
                            })
                            .child(gutter(ix + 1))
                            .child(div().flex_1().min_w_0().child(body))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this: &mut AppModel,
                                          e: &MouseDownEvent,
                                          w: &mut Window,
                                          cx: &mut Context<AppModel>| {
                                        match e.click_count {
                                            2 => this.resp_double_click(ix, cx),
                                            3.. => this.resp_select_all(cx),
                                            _ => this.resp_sel_begin(ix, w, cx),
                                        }
                                    },
                                ),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this: &mut AppModel, _, _, cx| {
                                    this.resp_sel_end(cx);
                                }),
                            )
                            .into_any()
                    }
                },
            ),
        )
        .flex_1()
        .min_h_0();
        let _ = count;
        let focus = self.resp_focus.clone();
        div()
            .id("resp-body")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .py(theme::sp3())
            // 点正文拿焦点后，⌘C 才是「复制选中文本」；没选中就不拦截，让别的处理者去
            .track_focus(&focus)
            .on_key_down(cx.listener(
                |this: &mut AppModel, e: &KeyDownEvent, _w: &mut Window, cx: &mut Context<AppModel>| {
                    if e.keystroke.key == "c" && e.keystroke.modifiers.platform {
                        this.resp_copy_shortcut(cx);
                    }
                },
            ))
            .child(list)
            .into_any()
    }
}
