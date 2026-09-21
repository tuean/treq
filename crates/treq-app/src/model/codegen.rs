//! 代码生成弹窗：语言/库选择、按请求（或树右键钉住的那个）出代码

use super::*;

impl AppModel {
    // ---- 插件 · 代码生成 ----
    /// 打开代码生成弹窗：默认用上次选的语言/库（settings 记忆）。
    pub fn open_codegen(&mut self, cx: &mut Context<Self>) {
        if self.codegen.is_none() {
            self.codegen = Some(
                self.settings
                    .codegen_lang
                    .as_deref()
                    .and_then(CodegenLang::from_id)
                    .unwrap_or_default(),
            );
        }
        cx.notify();
    }

    /// 代码生成弹窗里的语言下拉：列出全部语言/库。
    pub fn open_codegen_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let items = CodegenLang::ALL
            .iter()
            .map(|l| (format!("codegen:{}", l.id()), l.label().to_string()))
            .collect();
        let p = (pos.x.to_f64() as f32, pos.y.to_f64() as f32 + 4.0);
        self.toggle_popup("codegen", p.0, p.1, items, cx)
    }

    /// 切换代码生成目标并记住选择。
    pub fn set_codegen_lang(&mut self, lang: CodegenLang, cx: &mut Context<Self>) {
        self.codegen = Some(lang);
        self.settings.codegen_lang = Some(lang.id().to_string());
        settings::save(&self.settings).ok();
        cx.notify();
    }

    pub fn close_codegen(&mut self, cx: &mut Context<Self>) {
        self.codegen = None;
        self.codegen_pin = None;
        cx.notify();
    }

    /// 生成代码（当前选中请求）。无选中请求返回空。
    /// 生成代码的文本：优先用「钉住」的请求（树右键点了某一行时），否则用当前选中。
    pub fn codegen_text(&mut self) -> String {
        let lang = self.codegen.unwrap_or(CodegenLang::JavaOkHttp);
        let pinned = self
            .codegen_pin
            .clone()
            .and_then(|id| self.request_by_id(&id).cloned())
            .or_else(|| self.selected_request().cloned());
        let Some(req) = pinned else {
            return String::new();
        };
        self.codegen_for(&req, lang)
    }

    /// 按 id 找请求（树 / 菜单用，不依赖当前选中）。
    pub fn request_by_id(&self, id: &str) -> Option<&RequestItem> {
        self.workspace.collections.iter().find_map(|c| {
            c.requests
                .iter()
                .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
                .find(|r| r.id == id)
        })
    }

    /// 生成某个请求的代码（认证也写进去；用模板原文，环境变量不进代码）。
    pub(crate) fn codegen_for(&self, req: &RequestItem, lang: CodegenLang) -> String {
        let mut req = req.clone();
        if let Some(a) = req.auth.clone() {
            treq_core::auth::apply(&mut req, &a);
        }
        treq_core::generate(lang, &req).unwrap_or_else(|e| format!("// 生成失败: {}", e))
    }

    /// ⌘D：把当前选中的请求复制为新请求。
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(Selection::Request(id)) = self.selection.clone() {
            self.duplicate_request(&id, cx);
        }
    }

    /// ⌘,：在左下角弹出偏好菜单（和状态栏那颗按钮同一个菜单）。
    pub fn open_prefs_at_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let h = window.viewport_size().height;
        let pos = point(theme::sp4(), h - theme::sp4() * 5.0);
        self.open_prefs_menu(pos, cx);
    }

    /// 复制某个请求的 URL 模板到剪贴板。
    pub fn copy_request_url(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(url) = self.request_by_id(id).map(|r| r.url.clone()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
        self.toast(format!("{}{}", self.t("flash.copied_url"), url), cx);
    }

    /// 复制某个请求的 cURL（带认证，不含环境变量真值）。
    pub fn copy_request_curl(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(req) = self.request_by_id(id).cloned() else {
            return;
        };
        let code = self.codegen_for(&req, CodegenLang::ShellCurl);
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        self.toast(self.t("flash.copied_curl").to_string(), cx);
    }

    /// 复制为新请求：建在同集合同分组里，名字加「副本」后缀（不改当前选中，避免清掉正在看的响应）。
    pub fn duplicate_request(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((col_id, group_id, src)) = self.workspace.collections.iter().find_map(|c| {
            let top = c
                .requests
                .iter()
                .find(|r| r.id == id)
                .map(|r| (c.id.clone(), None, r.clone()));
            top.or_else(|| {
                c.groups.iter().find_map(|g| {
                    g.requests
                        .iter()
                        .find(|r| r.id == id)
                        .map(|r| (c.id.clone(), Some(g.id.clone()), r.clone()))
                })
            })
        }) else {
            return;
        };
        let mut copy = src.clone();
        copy.name = format!("{}{}", src.name, self.t("flash.copy_suffix"));
        match save_imported_request(&self.store, &col_id, group_id.as_deref(), &copy) {
            Ok(created) => {
                let name = created.name.clone();
                if let Some(c) = self
                    .workspace
                    .collections
                    .iter_mut()
                    .find(|c| c.id == col_id)
                {
                    match group_id {
                        Some(gid) => {
                            if let Some(g) = c.groups.iter_mut().find(|g| g.id == gid) {
                                g.requests.push(created);
                            }
                        }
                        None => c.requests.push(created),
                    }
                }
                self.toast(format!("{}{}", self.t("flash.duplicated"), name), cx);
            }
            Err(e) => self.toast(format!("{} {}", self.t("flash.duplicate_failed"), e), cx),
        }
        cx.notify();
    }

    /// 树右键「生成代码…」：钉住这一行，不动当前选中。
    pub fn open_codegen_for(&mut self, id: &str, cx: &mut Context<Self>) {
        self.codegen_pin = Some(id.to_string());
        self.open_codegen(cx);
    }

    pub fn copy_code(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let code = self.codegen_text();
        cx.write_to_clipboard(ClipboardItem::new_string(code));
        self.toast(self.t("flash.copied_code").to_string(), cx);
    }
}
