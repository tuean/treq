//! 导入：cURL / Postman / OpenAPI / HAR 的对话框与落盘

use super::*;

impl AppModel {
    // ---- cURL 导入 ----
    pub fn open_import_dialog(&mut self, cx: &mut Context<Self>) {
        let handle = cx.entity();
        let draft = TextField::new(
            self.import_draft.clone().into(),
            SharedString::from("curl https://api.example.com/v1/users"),
            Arc::new(|_, _| {}),
            cx,
        );
        draft.update(cx, |f, _cx| {
            let h = handle.clone();
            f.on_submit = Some(Arc::new(move |_window, app| {
                // confirm_import 要读这个输入框 → 不能在它自己的 update 里读，推迟一帧
                let h = h.clone();
                app.defer(move |app| {
                    h.update(app, |this, cx| this.confirm_import(cx));
                });
            }));
        });
        self.import_dialog = Some(ImportDialog { draft, error: None });
        cx.notify();
    }

    pub fn close_import_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(dlg) = &self.import_dialog {
            self.import_draft = dlg.draft.read(cx).content.to_string();
        }
        self.import_dialog = None;
        cx.notify();
    }

    pub fn confirm_import(&mut self, cx: &mut Context<Self>) {
        let Some(dlg) = &self.import_dialog else {
            return;
        };
        let cmd = dlg.draft.read(cx).content.to_string();
        match treq_core::parse_curl(&cmd) {
            Ok(parsed) => {
                let name = format!(
                    "{} {}",
                    parsed.method,
                    parsed
                        .url
                        .splitn(3, '/')
                        .nth(2)
                        .map(|h| h.split('/').next().unwrap_or(""))
                        .unwrap_or("")
                );
                // 目标集合：当前选中的集合，否则首个/新建
                let col_id = match &self.selection {
                    Some(Selection::Collection(id)) => id.clone(),
                    _ => self
                        .workspace
                        .collections
                        .first()
                        .map(|c| c.id.clone())
                        .unwrap_or_else(|| {
                            self.store
                                .create_collection(self.t("action.new_collection"))
                                .ok()
                                .map(|c| {
                                    self.workspace.collections.push(c.clone());
                                    c.id
                                })
                                .unwrap_or_default()
                        }),
                };
                if col_id.is_empty() {
                    return;
                }
                if let Ok(mut r) = self.store.create_request(&col_id, &name) {
                    r.method = parsed.method;
                    r.url = parsed.url;
                    r.headers = parsed.headers;
                    r.body = treq_core::Body {
                        kind: parsed.body_kind,
                        content: parsed.body_content,
                        form_data: parsed.form_data,
                    };
                    let _ = self.store.save_request(&r);
                    if let Some(c) = self
                        .workspace
                        .collections
                        .iter_mut()
                        .find(|c| c.id == col_id)
                    {
                        c.requests.push(r.clone());
                    }
                    self.selection = Some(Selection::Request(r.id));
                    self.fields = EditorFields::new();
                    self.load_history(cx);
                }
                self.import_dialog = None;
                cx.notify();
            }
            Err(e) => {
                if let Some(dlg) = &mut self.import_dialog {
                    dlg.error = Some(e.to_string());
                }
                cx.notify();
            }
        }
    }

    /// 从文件导入（Postman 集合 / OpenAPI 3 / HAR）→ 落成一个新集合。
    pub fn import_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(self.t("action.import_file"))),
        });
        let handle = cx.entity();
        cx.spawn(async move |_window, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    let _ = handle.update(cx, |this, cx| {
                        this.toast(format!("{}{}", this.t("import.read_failed"), e), cx);
                    });
                    return;
                }
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "imported".into());
            let _ = handle.update(cx, |this, cx| this.finish_import(text, stem, cx));
        })
        .detach();
        self.popup.close();
        cx.notify();
    }

    pub(crate) fn finish_import(&mut self, text: String, stem: String, cx: &mut Context<Self>) {
        let out = match treq_core::import_text(&text, &stem) {
            Ok(o) => o,
            Err(e) => {
                self.toast(format!("{} {}", self.t("import.failed"), e), cx);
                cx.notify();
                return;
            }
        };
        let (mut requests, mut groups) = (0usize, 0usize);
        if let Ok(col) = self.store.create_collection(&out.collection.name) {
            let col_id = col.id.clone();
            // 分组：先建分组再建请求（请求得写进分组目录）
            let mut saved_groups = Vec::new();
            for g in &out.collection.groups {
                if let Ok(mut sg) = self.store.create_group(&col_id, &g.name, None) {
                    for r in &g.requests {
                        if save_imported_request(&self.store, &col_id, Some(&sg.id), r).is_ok() {
                            requests += 1;
                        }
                        sg.requests.push(r.clone());
                    }
                    groups += 1;
                    let _ = self.store.save_group(&col_id, &sg);
                    saved_groups.push(sg);
                }
            }
            for r in &out.collection.requests {
                if save_imported_request(&self.store, &col_id, None, r).is_ok() {
                    requests += 1;
                }
            }
            if let Ok(ws) = self.store.load()
                && let Some(c) = ws.collections.into_iter().find(|c| c.id == col_id)
            {
                let name = c.name.clone();
                self.workspace.collections.push(c);
                self.selection = Some(Selection::Collection(col_id.clone()));
                self.toast(
                    format!(
                        "{}{}｜{}｜{} {}｜{} {}",
                        self.t("import.done"),
                        name,
                        out.format.id(),
                        requests,
                        self.t("import.requests"),
                        groups,
                        self.t("import.groups"),
                    ),
                    cx,
                );
            }
        }
        cx.notify();
    }
}
