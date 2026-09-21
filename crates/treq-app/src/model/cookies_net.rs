//! cookie 罐对话框 + 网络设置（代理/超时）+ 发请求时的 cookie 注入与收取

use super::*;

impl AppModel {
    // ---- cookie 罐对话框 ----
    pub fn open_cookie_dialog(&mut self, cx: &mut Context<Self>) {
        self.cookie_dialog = true;
        self.cookie_message = None;
        cx.notify();
    }

    pub fn close_cookie_dialog(&mut self, cx: &mut Context<Self>) {
        self.cookie_dialog = false;
        cx.notify();
    }

    /// 清 cookie：给域只清那个域，否则全清（都落盘）。
    pub fn clear_cookies(&mut self, domain: Option<&str>, cx: &mut Context<Self>) {
        let n = self
            .cookies
            .lock()
            .map(|mut j| j.clear(domain))
            .unwrap_or(0);
        self.save_cookie_jar();
        self.cookie_message = Some(format!(
            "{}{}{}",
            if domain.is_some() {
                self.t("cookies.cleared_host")
            } else {
                self.t("cookies.cleared_all")
            },
            n,
            self.t("cookies.unit")
        ));
        cx.notify();
    }

    /// 罐子内容（按域分组，给对话框用）。
    pub fn cookie_groups(&self) -> Vec<(String, Vec<treq_core::cookies::Cookie>)> {
        self.cookies
            .lock()
            .map(|j| {
                j.grouped(treq_core::cookies::now_unix())
                    .into_iter()
                    .map(|(h, list)| (h, list.into_iter().cloned().collect()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 配置页：打开（默认样式栏），网络栏需先备好输入框实体。
    pub fn open_settings_page(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.popup.close();
        if tab == SettingsTab::Network {
            self.ensure_net_fields(cx);
        }
        self.settings_page = Some(tab);
        cx.notify();
    }

    /// 关配置页：网络栏输入存回草稿（与旧网络弹窗一致）。
    pub fn close_settings_page(&mut self, cx: &mut Context<Self>) {
        if let Some(dlg) = &self.net_dialog {
            self.proxy_draft = dlg.proxy.read(cx).content.to_string();
            self.timeout_draft = dlg.timeout.read(cx).content.to_string();
        }
        self.net_dialog = None;
        self.settings_page = None;
        self.style_preview = None;
        self.popup.close();
        cx.notify();
    }

    pub fn save_net_settings(&mut self, cx: &mut Context<Self>) {
        let (raw_proxy, raw_timeout) = match &self.net_dialog {
            Some(d) => (
                d.proxy.read(cx).content.to_string(),
                d.timeout.read(cx).content.to_string(),
            ),
            None => return,
        };
        let proxy = raw_proxy.trim().to_string();
        let timeout_raw = raw_timeout.trim().to_string();
        // 先全量校验，错值不入库、对话框保持打开
        let proxy_check = if proxy.is_empty() {
            Ok(())
        } else {
            treq_core::http::check_proxy(&proxy)
        };
        let timeout_check =
            treq_core::http::parse_timeout(&timeout_raw, settings::DEFAULT_TIMEOUT_SEC);
        let fail = match (proxy_check, timeout_check) {
            (Ok(()), Ok(_)) => None,
            (Err(e), _) => Some(e),
            (_, Err(e)) => Some(e),
        };
        if let Some(e) = fail {
            if let Some(dlg) = &mut self.net_dialog {
                dlg.error = Some(e);
            }
            cx.notify();
            return;
        }
        let secs = treq_core::http::parse_timeout(&timeout_raw, settings::DEFAULT_TIMEOUT_SEC)
            .unwrap_or(settings::DEFAULT_TIMEOUT_SEC);
        self.proxy_draft = proxy.clone();
        self.timeout_draft = if secs == settings::DEFAULT_TIMEOUT_SEC {
            String::new()
        } else {
            secs.to_string()
        };
        self.settings.proxy = if proxy.is_empty() {
            None
        } else {
            Some(proxy.clone())
        };
        self.settings.timeout_sec = Some(secs);
        settings::save(&self.settings).ok();
        // 配置页打开时保留输入框（关页时统一丢弃）；否则与旧弹窗一样收起
        if self.settings_page.is_some() {
            if let Some(dlg) = &mut self.net_dialog {
                dlg.error = None;
            }
        } else {
            self.net_dialog = None;
        }
        let shown = if proxy.is_empty() {
            self.t("proxy.none").to_string()
        } else {
            proxy
        };
        self.stream_notice = Some(format!(
            "{}{} · {}{}s",
            self.t("net.saved"),
            shown,
            self.t("net.timeout_short"),
            secs
        ));
        cx.notify();
    }

    /// cookie 罐里的 cookie 数（界面用）。
    pub fn cookie_count(&self) -> usize {
        self.cookies.lock().map(|j| j.len()).unwrap_or(0)
    }

    pub(crate) fn save_cookie_jar(&self) {
        if let Ok(jar) = self.cookies.lock() {
            let p = settings::cookie_path();
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match serde_json::to_string_pretty(&*jar) {
                Ok(txt) => {
                    let _ = std::fs::write(&p, txt);
                }
                Err(e) => eprintln!("treq: cookie 罐序列化失败：{e}"),
            }
        }
    }

    /// 该请求要不要自动带 cookie：用户自己写了 Cookie 头就不插手。
    pub(crate) fn inject_cookies(&self, req: &mut RequestItem) {
        let has_manual = req.headers.iter().any(|h| {
            h.enabled
                && (h.key.eq_ignore_ascii_case("cookie")
                    || h.key.eq_ignore_ascii_case("set-cookie"))
        });
        if has_manual {
            return;
        }
        let header = self
            .cookies
            .lock()
            .ok()
            .and_then(|mut j| j.header_for(&req.url, treq_core::cookies::now_unix()));
        if let Some(v) = header {
            req.headers.push(Kv {
                key: "Cookie".into(),
                value: v,
                enabled: true,
                description: String::new(),
            });
        }
    }

    /// 响应里的 Set-Cookie 收进罐子（有变化则落盘）。
    pub(crate) fn harvest_cookies(&self, url: &str, headers: &[(String, String)]) {
        let Ok(mut jar) = self.cookies.lock() else {
            return;
        };
        let before = jar.len();
        let n = jar.store_from_headers(url, headers, treq_core::cookies::now_unix());
        let after = jar.len();
        drop(jar);
        if n > 0 || before != after {
            self.save_cookie_jar();
        }
    }

    /// 网络设置里的单行输入框（回车 = 保存）。
    pub(crate) fn net_field(
        &self,
        content: String,
        placeholder: &str,
        cx: &mut Context<Self>,
    ) -> Entity<TextField> {
        let handle = cx.entity();
        let f = TextField::new(
            content.into(),
            SharedString::from(placeholder.to_string()),
            Arc::new(|_, _| {}),
            cx,
        );
        f.update(cx, |f, _cx| {
            f.on_submit = Some(Arc::new(move |_window, app| {
                // save_net_settings 要读这两个输入框 → 不能在它们自己的 update 里读，推迟一帧
                let handle = handle.clone();
                app.defer(move |app| {
                    handle.update(app, |this, cx| this.save_net_settings(cx));
                });
            }));
        });
        f
    }

    /// 网络设置：代理地址 + 单请求超时。
    pub fn open_net_dialog(&mut self, cx: &mut Context<Self>) {
        self.open_settings_page(SettingsTab::Network, cx);
    }

    /// 备好网络栏的输入框实体（只建一次，避免每次渲染重建丢焦点）。
    pub fn ensure_net_fields(&mut self, cx: &mut Context<Self>) {
        if self.net_dialog.is_some() {
            return;
        }
        // 第一次打开时把当前值填进去，方便改
        if self.proxy_draft.is_empty() {
            self.proxy_draft = self.settings.proxy.clone().unwrap_or_default();
        }
        if self.timeout_draft.is_empty() {
            self.timeout_draft = self
                .settings
                .timeout_sec
                .map(|t| t.to_string())
                .unwrap_or_default();
        }
        let proxy = self.net_field(self.proxy_draft.clone(), "http://127.0.0.1:7897", cx);
        let timeout = self.net_field(self.timeout_draft.clone(), "30", cx);
        self.net_dialog = Some(NetDialog {
            proxy,
            timeout,
            error: None,
        });
    }
}
