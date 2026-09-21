//! 请求编辑（自动保存）：URL/方法/Body/KV/Docs/认证 的写入

use super::*;

impl AppModel {
    // ---- 请求编辑（自动保存） ----
    pub fn update_selected_request(
        &mut self,
        f: impl FnOnce(&mut RequestItem),
        cx: &mut Context<Self>,
    ) {
        let Some(rid) = (match &self.selection {
            Some(Selection::Request(id)) => Some(id.clone()),
            _ => None,
        }) else {
            return;
        };
        let found = self
            .workspace
            .collections
            .iter_mut()
            .flat_map(|c| {
                c.requests
                    .iter_mut()
                    .chain(c.groups.iter_mut().flat_map(|g| g.requests.iter_mut()))
            })
            .find(|r| r.id == rid);
        if let Some(r) = found {
            f(r);
        }
        if let Some(req) = self.selected_request().cloned() {
            let _ = self.store.save_request(&req);
        }
        cx.notify();
    }

    pub fn set_method(&mut self, m: String, cx: &mut Context<Self>) {
        self.update_selected_request(|r| r.method = m, cx);
    }
    pub fn set_url(&mut self, u: String, cx: &mut Context<Self>) {
        self.update_selected_request(|r| r.url = u, cx);
        self.update_preview(cx);
    }
    pub fn set_body_kind(&mut self, k: BodyKind, cx: &mut Context<Self>) {
        let kind = k.clone();
        self.body_error = None;
        // 只有 JSON 格式的正文才按 JSON 着色
        if let Some(f) = self.fields.body.clone() {
            let hl = kind == BodyKind::Json;
            f.update(cx, |f, cx| {
                if f.json_highlight != hl {
                    f.json_highlight = hl;
                    cx.notify();
                }
            });
        }
        self.update_selected_request(|r| r.body.kind = k, cx);
        // 切到 File 时把路径输入框同步成当前 content，便于直接编辑/选择
        if kind == BodyKind::File
            && let Some(field) = &self.fields.body_file
        {
            let content = self
                .selected_request()
                .map(|r| r.body.content.clone())
                .unwrap_or_default();
            let text: SharedString = content.into();
            field.update(cx, |f, cx| {
                f.content = text.clone();
                let len = f.content.len();
                f.selected_range = len..len;
                cx.notify();
            });
        }
        if kind == BodyKind::Multipart {
            self.ensure_form_data(cx);
        }
    }
    pub fn set_body_content(&mut self, c: String, cx: &mut Context<Self>) {
        self.update_selected_request(|r| r.body.content = c, cx);
        // 内容一变，上一次的 JSON 报错就过期了
        self.body_error = None;
    }

    /// JSON 正文「美化」：合法就重排缩进写回，不合法就报第几行第几列。
    pub fn beautify_body(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.fields.body.clone() else {
            return;
        };
        let text = field.read(cx).content.to_string();
        if text.trim().is_empty() {
            return;
        }
        match pretty_json(&text) {
            Ok(pretty) => {
                let len = pretty.len();
                field.update(cx, |f, cx| {
                    f.content = pretty.clone().into();
                    f.selected_range = len..len;
                    cx.notify();
                });
                self.set_body_content(pretty, cx);
                self.toast(self.t("flash.beautified").to_string(), cx);
            }
            Err((line, col, msg)) => {
                self.body_error = Some(
                    self.t("editor.body.json_error")
                        .replace("{line}", &line.to_string())
                        .replace("{col}", &col.to_string())
                        .replace("{msg}", &msg),
                );
                cx.notify();
            }
        }
        cx.notify();
    }

    /// 为 File 类型的请求体选择文件，并同步到路径输入框。
    pub fn choose_body_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(self.t("editor.choose_file"))),
        });
        let handle = cx.entity();
        cx.spawn(async move |_window, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                let path = path.to_string_lossy().to_string();
                let _ = handle.update(cx, |this, cx| {
                    this.set_body_content(path.clone(), cx);
                    if let Some(field) = &this.fields.body_file {
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

    pub fn kv_toggle(&mut self, which: KvWhich, index: usize, cx: &mut Context<Self>) {
        // enabled 只影响复选框渲染，不重建输入框（否则勾选导致全部失焦）
        self.update_selected_request(
            |r| match which {
                KvWhich::Params => {
                    if let Some(kv) = r.params.get_mut(index) {
                        kv.enabled = !kv.enabled;
                    }
                }
                KvWhich::Headers => {
                    if let Some(kv) = r.headers.get_mut(index) {
                        kv.enabled = !kv.enabled;
                    }
                }
                KvWhich::FormData => {
                    if let Some(field) = r.body.form_data.get_mut(index) {
                        field.enabled = !field.enabled;
                    }
                }
            },
            cx,
        );
    }

    pub fn kv_add(&mut self, which: KvWhich, cx: &mut Context<Self>) {
        self.update_selected_request(
            |r| match which {
                KvWhich::Params => r.params.push(treq_core::kv_new()),
                KvWhich::Headers => r.headers.push(treq_core::kv_new()),
                KvWhich::FormData => r.body.form_data.push(FormField::new()),
            },
            cx,
        );
        // 新行实体由 ensure_kv_field 按需创建，不重建已有输入框
    }

    /// Delete All：清空当前表（Insomnia 的 Delete All）。
    pub fn kv_clear(&mut self, which: KvWhich, cx: &mut Context<Self>) {
        self.update_selected_request(
            |r| match which {
                KvWhich::Params => r.params.clear(),
                KvWhich::Headers => r.headers.clear(),
                KvWhich::FormData => r.body.form_data.clear(),
            },
            cx,
        );
        self.rebuild_kv_fields(cx);
    }

    // ---- Docs 页 ----
    // ---- 认证 ----
    /// 当前请求是不是已经手写了 Authorization 头（认证设置不会覆盖它）。
    pub fn has_manual_auth_header(&self) -> bool {
        self.selected_request().is_some_and(|r| {
            r.headers
                .iter()
                .any(|h| h.enabled && h.key.eq_ignore_ascii_case("authorization"))
        })
    }

    /// 换认证方式（把旧的主值带过去，别让用户重敲）。
    pub fn set_auth_kind(&mut self, kind: &str, cx: &mut Context<Self>) {
        let kind = kind.to_string();
        self.update_selected_request(
            |r| {
                let cur = r.auth.clone().unwrap_or(treq_core::auth::Auth::None);
                let next = cur.with_kind(&kind);
                r.auth = if next.is_none() { None } else { Some(next) };
            },
            cx,
        );
        self.fields.auth.clear(); // 字段含义随类型变，重建输入框
        self.fields.auth_kind = Some(kind);
        cx.notify();
    }

    /// 写认证表单某个槽位（a / b，含义见 AuthField）。
    pub fn auth_set(&mut self, slot: &str, value: String, cx: &mut Context<Self>) {
        let f = if slot == "a" {
            treq_core::auth::AuthField::A
        } else {
            treq_core::auth::AuthField::B
        };
        self.update_selected_request(
            move |r| {
                if let Some(a) = r.auth.as_mut() {
                    a.set_field(f, value);
                }
            },
            cx,
        );
    }

    /// API Key 放头还是放 query。
    pub fn set_auth_key_in(&mut self, loc: treq_core::auth::KeyIn, cx: &mut Context<Self>) {
        self.update_selected_request(
            |r| {
                if let Some(treq_core::auth::Auth::ApiKey { location, .. }) = r.auth.as_mut() {
                    *location = loc;
                }
            },
            cx,
        );
    }

    pub fn set_description(&mut self, s: String, cx: &mut Context<Self>) {
        self.update_selected_request(|r| r.description = s, cx);
    }

    pub(crate) fn ensure_docs_field(&mut self, cx: &mut Context<Self>) -> Entity<TextField> {
        if let Some(f) = &self.fields.docs {
            return f.clone();
        }
        let value = self
            .selected_request()
            .map(|r| r.description.clone())
            .unwrap_or_default();
        let placeholder = self.t("docs.placeholder");
        let handle = cx.entity();
        let field = TextField::new(
            value.into(),
            SharedString::from(placeholder),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| this.set_description(s.to_string(), cx));
            }),
            cx,
        );
        let lh = px(crate::settings::editor_line_h(&self.settings));
        field.update(cx, |f, _cx| {
            f.multiline = true;
            f.line_height = Some(lh);
        });
        self.fields.docs = Some(field.clone());
        field
    }

    /// 外观栏的「编辑器行高」输入框：手动输入 px，12~32。
    pub(crate) fn editor_line_field(&mut self, cx: &mut Context<Self>) -> Entity<TextField> {
        if let Some(f) = &self.editor_line_field {
            return f.clone();
        }
        let cur = crate::settings::editor_line_h(&self.settings);
        let handle = cx.entity();
        let f = TextField::new(
            format!("{cur}").into(),
            SharedString::from("16"),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| this.apply_editor_line_h(s, cx));
            }),
            cx,
        );
        self.editor_line_field = Some(f.clone());
        f
    }

    /// 行高改成多少：非数字忽略（保留原值），合法值夹到 12~32 立刻生效并落盘。
    /// 注意只读回调给进来的字符串，不回读输入框本身（避免 in-update 双重租借）。
    pub fn apply_editor_line_h(&mut self, raw: &str, cx: &mut Context<Self>) {
        let Ok(v) = raw.trim().parse::<f32>() else {
            return;
        };
        if !v.is_finite() {
            return;
        }
        let v = v.clamp(12.0, 32.0);
        if crate::settings::editor_line_h(&self.settings) == v {
            return;
        }
        self.settings.editor_line_h = Some(v);
        settings::save(&self.settings).ok();
        // 全局显示常量 + 已建好的输入框（正文/文档/键值表）都要跟着变
        crate::theme::set_line_h(v);
        let lh = px(v);
        let kv: Vec<Entity<TextField>> = self.fields.kv.values().cloned().collect();
        for f in [self.fields.body.clone(), self.fields.docs.clone()]
            .into_iter()
            .flatten()
            .chain(kv)
        {
            f.update(cx, |f, _| f.line_height = Some(lh));
        }
        cx.notify();
    }
}

/// JSON 美化：合法 → 重排缩进；不合法 → (行, 列, 原始错误)（serde_json 的 line/column 都是 1 起）。
pub(crate) fn pretty_json(text: &str) -> Result<String, (usize, usize, String)> {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) => serde_json::to_string_pretty(&v).map_err(|e| (0, 0, e.to_string())),
        Err(e) => Err((e.line(), e.column(), e.to_string())),
    }
}

#[cfg(test)]
mod pretty_json_tests {
    use super::pretty_json;


    fn reformats_with_indent() {
        let out = pretty_json(r#"{"a":1,"b":[1,2]}"#).unwrap();
        assert_eq!(out, "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}");
    }

    #[test]
    fn reports_line_and_column_for_bad_json() {
        let (line, col, msg) = pretty_json("{\n  \"a\": 1,\n  \"b\" 2\n}").unwrap_err();
        assert_eq!((line, col), (3, 7), "报错位置要指到第 3 行的值上：{msg}");
        assert!(!msg.is_empty());
        let (line, _, _) = pretty_json("{\"a\"}").unwrap_err();
        assert_eq!(line, 1);
    }

    #[test]
    fn empty_input_is_an_error_not_a_panic() {
        assert!(pretty_json("").is_err());
    }
}
