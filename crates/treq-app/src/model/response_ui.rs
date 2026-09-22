//! 响应侧界面逻辑：过滤条、JSON 折叠、行右键菜单、每请求的响应缓存

use super::*;

impl AppModel {
    // ---- 响应过滤条 ----
    pub(crate) fn ensure_resp_filter_field(&mut self, cx: &mut Context<Self>) -> Entity<TextField> {
        if let Some(f) = &self.resp_filter_field {
            return f.clone();
        }
        let placeholder = self.t("response.filter.placeholder");
        let handle = cx.entity();
        let field = TextField::new(
            self.resp_filter.clone().into(),
            SharedString::from(placeholder),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| {
                    this.resp_filter = s.to_string();
                    cx.notify();
                });
            }),
            cx,
        );
        field.update(cx, |f, _cx| f.plain = true);
        self.resp_filter_field = Some(field.clone());
        field
    }

    pub fn kv_remove(&mut self, which: KvWhich, index: usize, cx: &mut Context<Self>) {
        self.update_selected_request(
            |r| match which {
                KvWhich::Params => {
                    if index < r.params.len() {
                        r.params.remove(index);
                    }
                }
                KvWhich::Headers => {
                    if index < r.headers.len() {
                        r.headers.remove(index);
                    }
                }
                KvWhich::FormData => {
                    if index < r.body.form_data.len() {
                        r.body.form_data.remove(index);
                    }
                }
            },
            cx,
        );
        self.rebuild_kv_fields(cx);
    }

    /// 结构变化（删除行）后重建 kv 输入框缓存；url/body 实体不动（否则输入失焦）。
    pub(crate) fn rebuild_kv_fields(&mut self, _cx: &mut Context<Self>) {
        self.fields.kv = HashMap::new();
    }

    /// 切换 multipart 字段的文本/文件类型。
    pub fn form_field_toggle_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_selected_request(
            |r| {
                if let Some(field) = r.body.form_data.get_mut(index) {
                    field.is_file = !field.is_file;
                }
            },
            cx,
        );
    }

    /// 为第 index 个 multipart 文件字段选择文件，并同步到它的 value 输入框。
    pub fn form_field_choose_file(&mut self, index: usize, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(self.t("editor.form.choose_file"))),
        });
        let handle = cx.entity();
        cx.spawn(async move |_window, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                let path = path.to_string_lossy().to_string();
                let _ = handle.update(cx, |this, cx| {
                    this.update_selected_request(
                        |r| {
                            if let Some(field) = r.body.form_data.get_mut(index) {
                                field.value = path.clone();
                                field.is_file = true;
                            }
                        },
                        cx,
                    );
                    let req_id = match &this.selection {
                        Some(Selection::Request(id)) => id.clone(),
                        _ => String::new(),
                    };
                    let key = format!("FD:{}:{}:value", req_id, index);
                    if let Some(field) = this.fields.kv.get(&key) {
                        let text: SharedString = path.clone().into();
                        field.update(cx, |f, cx| {
                            f.content = text.clone();
                            let len = f.content.len();
                            f.selected_range = len..len;
                            cx.notify();
                        });
                    }
                });
            }
        })
        .detach();
    }

    /// 旧 YAML 兼容：Multipart 且 form_data 为空时，把 content 行迁移成结构化字段。
    pub fn ensure_form_data(&mut self, cx: &mut Context<Self>) {
        let needs = self
            .selected_request()
            .map(|r| {
                r.body.kind == BodyKind::Multipart
                    && r.body.form_data.is_empty()
                    && !r.body.content.trim().is_empty()
            })
            .unwrap_or(false);
        if needs {
            self.update_selected_request(
                |r| {
                    let fields = r.body.multipart_fields();
                    r.body.form_data = fields;
                    r.body.content.clear();
                },
                cx,
            );
        }
    }

    /// 正文里左键按下：起点 = 鼠标下面那一列；没 hover 过就从行首起。
    pub(crate) fn resp_sel_begin(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let col = self.resp_hover_col.unwrap_or(0);
        self.resp_sel = Some(crate::model::RespSel {
            a_row: row,
            a_col: col,
            c_row: row,
            c_col: col,
        });
        self.resp_sel_drag = true;
        // 点正文就拿焦点：这样 ⌘C 是「复制选中文本」而不是去拷输入框
        self.resp_focus.focus(window);
        cx.notify();
    }

    /// 拖动中：光标位置跟着走（只有真的变了才重画）。
    /// 双击正文一行：选中这一行里 `"…"` 之间的内容（没有引号就退成一段词 / 整行）。
    pub(crate) fn resp_double_click(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(line) = self.resp_lines.get(row).cloned() else {
            return;
        };
        let col = self.resp_hover_col.unwrap_or(0);
        let range = crate::widgets::double_click_range(&line, col);
        self.resp_sel = Some(crate::model::RespSel {
            a_row: row,
            a_col: range.start,
            c_row: row,
            c_col: range.end,
        });
        self.resp_sel_drag = false;
        cx.notify();
    }

    /// 三击：整个正文全选。
    pub(crate) fn resp_select_all(&mut self, cx: &mut Context<Self>) {
        let Some(end) = self.resp_lines.last().map(|l| l.len()) else {
            return;
        };
        self.resp_sel = Some(crate::model::RespSel {
            a_row: 0,
            a_col: 0,
            c_row: self.resp_lines.len() - 1,
            c_col: end,
        });
        self.resp_sel_drag = false;
        cx.notify();
    }

    pub(crate) fn resp_sel_drag_to(&mut self, row: usize, col: usize, cx: &mut Context<Self>) {
        self.resp_hover_col = Some(col);
        if !self.resp_sel_drag {
            return;
        }
        if let Some(sel) = self.resp_sel.as_mut()
            && (sel.c_row, sel.c_col) != (row, col)
        {
            sel.c_row = row;
            sel.c_col = col;
            cx.notify();
        }
    }

    pub(crate) fn resp_sel_end(&mut self, cx: &mut Context<Self>) {
        if self.resp_sel_drag {
            self.resp_sel_drag = false;
            cx.notify();
        }
    }

    /// 选中的文本（按现在屏幕上看到的行，含折叠/缩进）。
    pub(crate) fn resp_selected_text(&self) -> Option<String> {
        join_resp_selection(&self.resp_lines, self.resp_sel?)
    }

    pub(crate) fn copy_resp_selection(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.resp_selected_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast(self.t("flash.copied_text").to_string(), cx);
    }

    /// ⌘C：有选中就复制选中文本（返回是否处理了）。没选中就不管，让其它处理者去。
    pub(crate) fn resp_copy_shortcut(&mut self, cx: &mut Context<Self>) -> bool {
        if self.resp_selected_text().is_none() {
            return false;
        }
        self.copy_resp_selection(cx);
        true
    }

    pub fn update_preview(&mut self, _cx: &mut Context<Self>) {
        let vars = vars::merge_env(&self.workspace.base_env, self.active_env());
        // 预览里必须带上 enabled 的 query 参数，与实际发送/代码生成保持一致
        // 预览要跟实际发送一致：没写协议的 URL 发送时会补 http://
        self.fields.resolved_url = self
            .selected_request()
            .map(|r| treq_core::http::normalize_url(&vars::resolve_url(r, &vars)))
            .unwrap_or_default();
    }

    // ---- 响应 JSON 折叠 ----
    /// 折/展某一行（按 resp_all_lines 的下标）。
    pub fn toggle_fold(&mut self, line: usize, cx: &mut Context<Self>) {
        if !matches!(self.resp_fold_ends.get(line), Some(Some(end)) if *end > line) {
            return;
        }
        if !self.resp_folded.remove(&line) {
            self.resp_folded.insert(line);
        }
        self.rebuild_visible_lines();
        cx.notify();
    }

    /// 折叠全部（只折最外层）／展开全部。
    pub fn fold_all(&mut self, cx: &mut Context<Self>) {
        self.resp_folded = crate::fold::fold_all(&self.resp_fold_ends);
        self.rebuild_visible_lines();
        cx.notify();
    }

    pub fn unfold_all(&mut self, cx: &mut Context<Self>) {
        if self.resp_folded.is_empty() {
            return;
        }
        self.resp_folded.clear();
        self.rebuild_visible_lines();
        cx.notify();
    }

    /// 折叠集合变了：重算可见行（resp_lines）+ 每行对应的原始行号。
    pub(crate) fn rebuild_visible_lines(&mut self) {
        // fold_ends 只在「JSON 正文 + 没过滤」时按行构建；其余情况（原始视图、过滤结果、
        // 非 JSON 正文）它是空的 —— 此时没有折叠可言，直接用全部行。
        // （否则 visible_indices 按 ends.len() 算，0 条 ends 会算出 0 行，正文一片空白。）
        self.resp_visible_idx = if self.resp_fold_ends.len() == self.resp_all_lines.len() {
            crate::fold::visible_indices(&self.resp_fold_ends, &self.resp_folded)
        } else {
            (0..self.resp_all_lines.len()).collect()
        };
        self.resp_lines = self
            .resp_visible_idx
            .iter()
            .filter_map(|i| self.resp_all_lines.get(*i).cloned())
            .collect();
        // 流式还在收：正文只会往后追加，用户可能正滚在上面看 —— 用 splice 保住
        // 滚动位置和选中行（reset 会把滚动位置清回顶部，每来一块内容就被拽回去）。
        let old_count = self.resp_list.item_count();
        let new_count = self.resp_lines.len();
        // 追加多少行就 splice 多少行（不是换掉全部）：splice 只在「被替换的范围包含
        // 当前顶部行」时才把滚动位置推到范围开头，范围是空的就不会动滚动位置，
        // 顺带也不用重测前面对已经在屏幕上的行。
        if self.stream_live && old_count > 0 && new_count > old_count {
            self.resp_list.splice(old_count..old_count, new_count - old_count);
            return;
        }
        self.resp_list.reset(self.resp_lines.len());
        // 行变了（新响应 / 过滤 / 折叠），旧的选中下标就没意义了
        self.resp_sel = None;
        self.resp_sel_drag = false;
    }

    /// 过滤条有内容时展示的是查询结果、不是原文，折叠没有意义。
    pub(crate) fn folding_active(&self) -> bool {
        self.resp_filter.trim().is_empty()
    }

    // ---- 响应行右键：复制这一行 / 复制值 / 复制路径 / 存为变量 ----
    /// 响应正文某一行的右键菜单（行号是「折叠前的原始行」）。
    pub fn open_resp_line_menu(&mut self, line: usize, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if self.resp_all_lines.get(line).is_none() {
            return;
        }
        let mut items = vec![(
            format!("resp-line:copy:{}", line),
            self.t("response.copy_line").to_string(),
        )];
        // 只有标量行才有「值」可言（对象/数组行是复合值）
        let scalar = self
            .resp_all_lines
            .get(line)
            .and_then(|l| crate::jsonview::line_scalar(l));
        if let Some(v) = scalar {
            let label = if v.chars().count() > 24 {
                format!("{}…", v.chars().take(24).collect::<String>())
            } else {
                v
            };
            items.push((
                format!("resp-line:value:{}", line),
                format!("{}  {}", self.t("response.copy_value"), label),
            ));
            if self.resp_lines_json {
                items.push((
                    format!("resp-line:path:{}", line),
                    self.t("response.copy_path").to_string(),
                ));
                items.push((
                    format!("resp-line:var:{}", line),
                    self.t("vars.save_btn").to_string(),
                ));
            }
        }
        // source 里带上行号：右键另一行要直接换菜单，而不是把上一个菜单关掉
        self.toggle_popup(
            &format!("resp-line:{}", line),
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32,
            items,
            cx,
        );
    }

    /// 这一行的原始文本（按折叠前的下标）。
    pub(crate) fn resp_line_at(&self, line: usize) -> Option<String> {
        self.resp_all_lines.get(line).cloned()
    }

    /// 在响应 JSON 里按值反查路径（太大/不是 JSON 就 None）。
    pub(crate) fn resp_line_path(&self, line: usize) -> Option<String> {
        let needle = crate::jsonview::line_scalar(&self.resp_line_at(line)?)?;
        let body = self.response.as_ref()?.body.clone();
        if body.len() > 8 * 1024 * 1024 {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
        treq_core::json::find_path_by_value(&v, &needle)
    }

    pub(crate) fn copy_resp_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(text) = self.resp_line_at(line) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast(self.t("flash.copied_line").to_string(), cx);
    }

    pub(crate) fn copy_resp_line_value(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(v) = self
            .resp_line_at(line)
            .and_then(|l| crate::jsonview::line_scalar(&l))
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(v));
        self.toast(self.t("flash.copied_value").to_string(), cx);
    }

    pub(crate) fn copy_resp_line_path(&mut self, line: usize, cx: &mut Context<Self>) {
        match self.resp_line_path(line) {
            Some(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.toast(format!("{}{}", self.t("flash.copied_path"), path), cx);
            }
            None => self.toast(self.t("flash.path_not_found").to_string(), cx),
        }
        cx.notify();
    }

    /// 这一行就是变量值：直接把「存为变量」弹窗的路径填好（能查到路径就用查到的）。
    pub(crate) fn save_resp_line_var(&mut self, line: usize, cx: &mut Context<Self>) {
        let path = self.resp_line_path(line);
        let fallback = self
            .resp_line_at(line)
            .and_then(|l| crate::jsonview::line_scalar(&l))
            .map(|_| "$".to_string());
        match path.or(fallback) {
            Some(p) => self.open_save_var_dialog_at(Some(p), cx),
            None => {
                self.toast(self.t("flash.path_not_found").to_string(), cx);
                cx.notify();
            }
        }
    }

    // ---- 每个请求「上次的响应」----
    /// 记住这次响应（切走再切回来还看得到）。
    pub(crate) fn cache_response(&mut self, id: &str, resp: &ResponseData) {
        const MAX_ITEMS: usize = 16;
        const MAX_BYTES: usize = 32 * 1024 * 1024;
        self.resp_cache.retain(|(k, _)| k != id);
        self.resp_cache.push((id.to_string(), resp.clone()));
        loop {
            let bytes: usize = self.resp_cache.iter().map(|(_, r)| r.body.len()).sum();
            if self.resp_cache.len() <= MAX_ITEMS && bytes <= MAX_BYTES {
                break;
            }
            if self.resp_cache.is_empty() || self.resp_cache.len() == 1 {
                break;
            }
            self.resp_cache.remove(0);
        }
    }

    /// 取回某个请求上次的响应（缓存留着，下次切回来还有）。
    pub(crate) fn cached_response(&self, id: &str) -> Option<ResponseData> {
        self.resp_cache
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, r)| r.clone())
    }

    /// ⌘F：聚焦响应过滤条（没有响应时给个提示，别静默）。
    pub fn focus_response_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.response.is_none() {
            self.toast(self.t("flash.no_response").to_string(), cx);
            cx.notify();
            return;
        }
        let field = self.ensure_resp_filter_field(cx);
        let handle = field.read(cx).focus_handle.clone();
        window.focus(&handle);
        cx.notify();
    }
}

/// 按字节切字符串，但把下标夹到字符边界上（鼠标 hover 给的下标理论上就是边界）。
fn slice_chars(line: &str, from: usize, to: usize) -> &str {
    fn boundary(s: &str, mut i: usize) -> usize {
        i = i.min(s.len());
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
    let from = boundary(line, from);
    let to = boundary(line, to.max(from));
    &line[from..to]
}

/// 选中区间 → 文本：行内按字节切，行与行之间补 \n。
/// 抽成纯函数是为了能单测（跨行拼接 / 中文边界 / 反着拖都对）。
pub(crate) fn join_resp_selection(lines: &[String], sel: crate::model::RespSel) -> Option<String> {
    let ((s_row, s_col), (e_row, e_col)) = sel.range();
    if (s_row, s_col) == (e_row, e_col) {
        return None;
    }
    let mut out = String::new();
    for row in s_row..=e_row {
        let Some(line) = lines.get(row) else {
            break;
        };
        let range = sel.row_range(row, line.len())?;
        if row > s_row {
            out.push('\n');
        }
        out.push_str(slice_chars(line, range.0, range.1));
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod selection_text_tests {
    use super::join_resp_selection;
    use crate::model::RespSel;

    fn lines() -> Vec<String> {
        vec![
            "{".to_string(),
            "  \"名称\": \"张三\",".to_string(),
            "  \"n\": 42".to_string(),
            "}".to_string(),
        ]
    }

    #[test]
    fn joins_selected_rows_with_newlines() {
        let sel = RespSel {
            a_row: 1,
            a_col: 2,
            c_row: 2,
            c_col: 6,
        };
        assert_eq!(
            join_resp_selection(&lines(), sel).as_deref(),
            Some("\"名称\": \"张三\",\n  \"n\":")
        );
    }

    #[test]
    fn reversed_drag_gives_the_same_text() {
        let a = RespSel {
            a_row: 2,
            a_col: 6,
            c_row: 1,
            c_col: 2,
        };
        let b = RespSel {
            a_row: 1,
            a_col: 2,
            c_row: 2,
            c_col: 6,
        };
        assert_eq!(
            join_resp_selection(&lines(), a),
            join_resp_selection(&lines(), b)
        );
    }

    #[test]
    fn stray_char_mid_chinese_is_clamped() {
        // 第 2 行：`  "名称": "张三",` —— 列 6 落在中文中间，应回退到字符边界
        let l = lines();
        let sel = RespSel {
            a_row: 1,
            a_col: 6,
            c_row: 1,
            c_col: 100,
        };
        let text = join_resp_selection(&l, sel).unwrap();
        assert!(l[1].ends_with(&text), "从中文中间起选：{text:?}");
        assert!(!text.starts_with('\u{fffd}'));
    }

    #[test]
    fn empty_selection_is_none() {
        let sel = RespSel {
            a_row: 1,
            a_col: 3,
            c_row: 1,
            c_col: 3,
        };
        assert_eq!(join_resp_selection(&lines(), sel), None);
    }
}
