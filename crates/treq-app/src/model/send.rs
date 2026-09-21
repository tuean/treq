//! 发送：普通请求与事件流（SSE）、响应动作（复制/存文件）、流式状态

use super::*;

impl AppModel {
    // ---- 渲染 ----
    /// 发送：普通请求与事件流（SSE）走同一条流式通道；
    /// 响应头说 `text/event-stream` 就边收边显示，可随时「停止」。
    pub fn send_request(&mut self, cx: &mut Context<Self>) {
        self.flash = None;
        let Some(req) = self.selected_request().cloned() else {
            return;
        };
        if self.sending {
            return;
        }
        let vars = vars::merge_env(&self.workspace.base_env, self.active_env());
        let mut resolved = vars::resolve_request(&req, &vars);
        // 认证：模板已在 resolve 时换成真值；用户手写的 Authorization 头优先
        if let Some(a) = resolved.auth.clone() {
            treq_core::auth::apply(&mut resolved, &a);
        }
        // 罐子里有本域 cookie 就自动带上（用户自己写了 Cookie 头则不动）
        self.inject_cookies(&mut resolved);
        // 没写协议的 URL 发送前会补 http://（core 的 normalize_url），预览里显示实际发出的地址
        self.fields.resolved_url = treq_core::http::normalize_url(&resolved.url);
        self.sending = true;
        self.response = None;
        self.resp_gen = self.resp_gen.wrapping_add(1);
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.stream_req = Some(req.id.clone());
        self.stream_cancel = Some(cancel.clone());
        self.stream_live = false;
        self.stream_events = 0;
        self.stream_bytes = 0;
        self.stream_started = Some(std::time::Instant::now());
        self.stream_flush = None;
        self.stream_notice = None;
        self.sse_marks.clear();
        self.sse_lines = 0;
        cx.notify();

        let (tx, mut rx) = treq_core::http::unbounded_channel();
        let handle = cx.entity();
        let (recv_handle, send_handle) = (handle.clone(), handle.clone());
        let (recv_id, send_id) = (req.id.clone(), req.id.clone());
        let snapshot = req.clone();
        // 代理：设置里给了就走代理（本机 Clash 这类），空 = 直连
        let proxy = self.settings.proxy.clone().filter(|p| !p.trim().is_empty());
        let timeout = settings::timeout(&self.settings);
        cx.spawn(async move |_window, cx| {
            let r = treq_core::http::send_stream(
                &resolved,
                Duration::from_secs(timeout),
                proxy.as_deref(),
                tx,
                cancel,
            )
            .await;
            let _ = send_handle.update(cx, |this, cx| this.finish_send(&send_id, r, snapshot, cx));
        })
        .detach();
        cx.spawn(async move |_window, cx| {
            while let Some(ev) = rx.recv().await {
                let _ =
                    recv_handle.update(cx, |this, cx| this.apply_stream_event(&recv_id, ev, cx));
            }
        })
        .detach();
    }

    /// 用户点「停止」：置位取消开关，core 最多 1 秒内断开连接并收尾。
    /// ⌘Enter：发送中则停止，否则发送（当前请求）。
    pub fn send_or_stop(&mut self, cx: &mut Context<Self>) {
        if self.stream_live {
            self.stop_stream(cx);
        } else {
            self.send_request(cx);
        }
    }

    pub fn stop_stream(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = &self.stream_cancel {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.stream_notice = Some(self.t("sse.stopped").to_string());
        cx.notify();
    }

    /// 流式事件：Head 立刻出状态/头部，Chunk 追加正文（节流刷新），Done 交回 finish_send。
    pub fn apply_stream_event(
        &mut self,
        id: &str,
        ev: treq_core::http::StreamEvent,
        cx: &mut Context<Self>,
    ) {
        // 已经切到别的请求、或被新请求顶替：这条流的事件全部丢弃
        if self.stream_req.as_deref() != Some(id) {
            return;
        }
        match ev {
            treq_core::http::StreamEvent::Head {
                status,
                status_text,
                headers,
            } => {
                if treq_core::http::is_event_stream(&headers) {
                    self.stream_live = true;
                    self.response = Some(ResponseData {
                        status,
                        status_text,
                        headers,
                        body: Vec::new(),
                        duration: Duration::ZERO,
                        error: None,
                    });
                    self.response_tab = ResponseTab::Body;
                    self.resp_gen = self.resp_gen.wrapping_add(1);
                }
            }
            treq_core::http::StreamEvent::Chunk(bytes) => {
                if !self.stream_live {
                    return; // 普通请求：等 finish_send 一次性给完整结果
                }
                self.stream_bytes += bytes.len();
                // 事件之间用空行分隔，数到几个空行就是几个事件（跨块切分时略有误差，够用）
                self.stream_events += count_event_breaks(&bytes);
                let truncated = self
                    .response
                    .as_ref()
                    .map(|r| r.body.len() >= MAX_STREAM_BYTES)
                    .unwrap_or(false);
                if let Some(r) = self.response.as_mut()
                    && !truncated
                {
                    // 时间列：记下这块的首行号 + 到达时间（开关关着也记，回头打开能看到前面的）
                    let mark = next_mark_line(&mut self.sse_lines, &bytes);
                    r.body.extend_from_slice(&bytes);
                    self.sse_marks.push((mark, treq_core::history::local_hms_millis(treq_core::now_millis())));
                }
                if truncated && self.stream_notice.is_none() {
                    self.stream_notice = Some(self.t("sse.truncated").to_string());
                }
                let due = self
                    .stream_flush
                    .map(|t| t.elapsed().as_millis() as u64 >= STREAM_FLUSH_MS)
                    .unwrap_or(true);
                if due {
                    self.stream_flush = Some(std::time::Instant::now());
                    // 行缓存按 resp_gen 失效，推一次就能看到新事件
                    self.resp_gen = self.resp_gen.wrapping_add(1);
                }
            }
            treq_core::http::StreamEvent::Done { .. } => {}
        }
        cx.notify();
    }

    /// 请求收尾：写入历史、状态、最终正文（事件流用累计正文）。
    pub(crate) fn finish_send(
        &mut self,
        id: &str,
        mut r: ResponseData,
        req: RequestItem,
        cx: &mut Context<Self>,
    ) {
        // 用户中途切走也要记账：历史照写、「上次响应」照存 —— 否则这一趟就白跑了
        let active = self.stream_req.as_deref() == Some(id);
        let elapsed = self
            .stream_started
            .map(|t| t.elapsed())
            .unwrap_or(r.duration);
        // 事件流可能无限长：界面只留前 2MB。普通响应收完再给，上限放宽到 64MB，
        // 这样「响应大小」显示的是真实值，也能整份完整渲染。
        // 先把 Set-Cookie 收进罐子（后面可能截断 body，头是完整的）
        self.harvest_cookies(&req.url, &r.headers);
        let is_sse = treq_core::http::is_event_stream(&r.headers);
        let cap = if is_sse {
            MAX_STREAM_BYTES
        } else {
            MAX_BODY_BYTES
        };
        if r.body.len() > cap {
            r.body.truncate(cap);
            if active && self.stream_notice.is_none() {
                self.stream_notice = Some(if is_sse {
                    self.t("sse.truncated").to_string()
                } else {
                    format!("{}{}", self.t("response.truncated"), fmt_bytes(cap))
                });
            }
        }
        let _e = self.history_store.insert(&HistoryEntry {
            id: 0,
            sent_at: treq_core::now_millis(),
            status: r.status,
            duration_ms: elapsed.as_millis() as u64,
            error: r.error.as_ref().map(|e| e.to_string()),
            // 快照存模板（未解析），恢复后变量仍可编辑
            request: req,
        });
        self.cache_response(id, &r);
        if active {
            self.sending = false;
            self.stream_req = None;
            self.stream_cancel = None;
            self.stream_live = false;
            self.resp_gen = self.resp_gen.wrapping_add(1);
            self.response = Some(r);
            self.response_tab = ResponseTab::Body;
        }
        self.refresh_last_status();
        if active {
            self.load_history(cx);
        }
        cx.notify();
    }

    /// 建议的保存文件名：`<请求名>-<时间戳>.<按 Content-Type 猜的扩展名>`
    /// 名字里的 `/`、`:`、空格等会换成 `-`（macOS 文件名不允许 `/` 和 `:`）。
    pub fn response_file_name(&self) -> String {
        let name = self
            .selection
            .as_ref()
            .and_then(|s| match s {
                Selection::Request(id) => self
                    .workspace
                    .collections
                    .iter()
                    .flat_map(|c| {
                        c.requests
                            .iter()
                            .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
                    })
                    .find(|r| &r.id == id)
                    .map(|r| r.name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "response".to_string());
        let ct = self
            .response
            .as_ref()
            .and_then(|r| {
                r.headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.clone())
            })
            .unwrap_or_default();
        let body = self
            .response
            .as_ref()
            .map(|r| r.body.as_slice())
            .unwrap_or(&[]);
        crate::files::suggested_file_name(&name, &ct, body)
    }

    /// 把当前响应正文原样写到文件（大响应拿出来看/给别处用）。
    pub fn write_response_body(&self, path: &std::path::Path) -> std::io::Result<usize> {
        let body = self
            .response
            .as_ref()
            .map(|r| r.body.as_slice())
            .unwrap_or(&[]);
        std::fs::write(path, body)?;
        Ok(body.len())
    }

    /// 保存响应正文（弹系统保存对话框，默认下载目录）。
    pub fn save_response_body(&mut self, cx: &mut Context<Self>) {
        if self.response.is_none() {
            return;
        }
        let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
        let name = self.response_file_name();
        let receiver = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let _ = this.update(cx, |m, cx| {
                m.stream_notice = Some(match m.write_response_body(&path) {
                    Ok(n) => format!(
                        "{}{} · {}",
                        m.t("response.saved"),
                        path.display(),
                        fmt_bytes(n)
                    ),
                    Err(e) => format!("{}{}", m.t("response.save_failed"), e),
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// 复制响应正文（原文，不经过格式化）。
    pub fn copy_response_body(&mut self, cx: &mut Context<Self>) {
        let Some(resp) = self.response.as_ref() else {
            return;
        };
        let text = String::from_utf8_lossy(&resp.body).to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let notice = format!(
            "{}{}",
            self.t("response.copied"),
            fmt_bytes(resp.body.len())
        );
        self.stream_notice = Some(notice.clone());
        self.toast(notice, cx);
        cx.notify();
    }

    /// 事件流时间列开关：只影响显示，记进设置下次启动保持。
    pub fn toggle_sse_time(&mut self, cx: &mut Context<Self>) {
        self.sse_show_time = !self.sse_show_time;
        self.settings.sse_show_time = Some(self.sse_show_time);
        settings::save(&self.settings).ok();
        cx.notify();
    }

    /// 状态栏里的事件流摘要（收流中显示进度，结束后只显示提示）。
    pub fn stream_status(&self) -> Option<String> {
        if self.stream_live {
            let secs = self
                .stream_started
                .map(|t| t.elapsed().as_secs_f32())
                .unwrap_or(0.);
            return Some(format!(
                "{} · {} {} · {} · {:.1}s",
                self.t("sse.live"),
                self.stream_events,
                self.t("sse.events"),
                fmt_bytes(self.stream_bytes),
                secs
            ));
        }
        self.stream_notice.clone()
    }
}

/// 收到一块正文：返回它的起始行号（0 基），并把累计行数往后推。
/// 事件流按 `\n\n` 分帧，块首一般正好是行首（服务端把帧切一半时分到上一行的时间，够用）。
fn next_mark_line(total: &mut usize, chunk: &[u8]) -> usize {
    let start = *total;
    *total += chunk.iter().filter(|b| **b == b'\n').count();
    start
}

#[cfg(test)]
mod tests {
    use super::next_mark_line;

    #[test]
    fn marks_follow_the_line_numbers_of_each_chunk() {
        let mut total = 0;
        let mut marks = Vec::new();
        for chunk in [b"id: 1\ndata: a\n\n".as_slice(), b"id: 2\ndata: b\n\n", b"data: c\n\n"] {
            marks.push(next_mark_line(&mut total, chunk));
        }
        assert_eq!(marks, vec![0, 3, 6], "每块的首行号");
        assert_eq!(total, 8, "累计行数 = 各块换行之和（3 + 3 + 2）");
        // 空块不占行也不产生新行号
        assert_eq!(next_mark_line(&mut total, b""), 8);
        assert_eq!(total, 8);
    }
}
