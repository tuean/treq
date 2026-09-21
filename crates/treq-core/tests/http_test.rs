use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use treq_core::http::{HttpError, send, send_stream};
use treq_core::json::{looks_json, pretty_json};
use treq_core::*;

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

fn mock_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 16384];
            let _ = s.read(&mut buf);
            let _ = s.write_all(response.as_bytes());
            let _ = s.flush();
        }
    });
    addr.to_string()
}

/// 极简 HTTP 代理桩：只回答「你走了代理」，并记下每条请求行（应含绝对 URI）。
/// 返回 (代理地址, 请求行记录)。
fn proxy_stub() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let seen2 = seen.clone();
    thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let line = head.lines().next().unwrap_or("").to_string();
            seen2.lock().unwrap().push(line);
            let body = "via-proxy";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    (format!("http://{}", addr), seen)
}

/// 拖 N 秒才回响应头的服务器（用来验证「等头」也受超时约束）。
fn slow_server(delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            thread::sleep(delay);
            let _ =
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            let _ = s.flush();
        }
    });
    addr.to_string()
}

#[test]
fn stream_headers_respect_timeout() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let addr = slow_server(Duration::from_secs(2));
    let chan = || treq_core::http::unbounded_channel();
    // 超时 500ms，服务器 2s 才回：必须报超时，且不等满 2 秒
    let (tx, _rx) = chan();
    let started = std::time::Instant::now();
    let r = block_on(send_stream(
        &req(&format!("http://{}/slow", addr)),
        Duration::from_millis(500),
        None,
        tx,
        Arc::new(AtomicBool::new(false)),
    ));
    assert!(
        matches!(r.error, Some(HttpError::Timeout)),
        "应报超时：{:?}",
        r.error
    );
    assert!(
        started.elapsed() < Duration::from_millis(1600),
        "不该干等到服务器回包：{:?}",
        started.elapsed()
    );
    // 超时给 5s：正常拿到 200 —— 设置真的起作用，不是把请求废掉
    let (tx2, _rx2) = chan();
    let r2 = block_on(send_stream(
        &req(&format!("http://{}/slow", addr)),
        Duration::from_secs(5),
        None,
        tx2,
        Arc::new(AtomicBool::new(false)),
    ));
    assert!(r2.error.is_none(), "{:?}", r2.error);
    assert_eq!(r2.status, Some(200));
}

#[test]
fn proxy_is_used_when_set() {
    let (proxy, seen) = proxy_stub();
    // 目标域名不存在：只有真的走了代理才可能拿到 200
    let r = block_on(send(
        &req("http://treq-proxy-test.invalid/hello"),
        Duration::from_secs(5),
        Some(&proxy),
    ));
    assert!(r.error.is_none(), "走代理不该报错：{:?}", r.error);
    assert_eq!(r.status, Some(200));
    assert_eq!(String::from_utf8_lossy(&r.body), "via-proxy");
    // 请求行必须是绝对 URI（代理协议要求），且指向原目标
    let lines = seen.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "代理应只收到一条请求：{lines:?}");
    assert!(
        lines[0].starts_with("GET http://treq-proxy-test.invalid/hello"),
        "请求行应含绝对 URI：{}",
        lines[0]
    );
}

#[test]
fn proxy_off_goes_direct() {
    // 同一个不存在的域名：不设代理 ⇒ 直连失败（DNS），不会碰到代理桩
    let (_proxy, seen) = proxy_stub();
    let r = block_on(send(
        &req("http://treq-proxy-test.invalid/hello"),
        Duration::from_secs(5),
        None,
    ));
    assert!(r.error.is_some(), "直连不存在的域名应该报错");
    assert!(seen.lock().unwrap().is_empty(), "不该碰代理");
    // 空串与 None 等价（设置里清空代理）
    let r2 = block_on(send(
        &req("http://treq-proxy-test.invalid/hello"),
        Duration::from_secs(5),
        Some("   "),
    ));
    assert!(r2.error.is_some(), "空白代理串应等价于直连");
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn timeout_parse_rules() {
    use treq_core::http::{MAX_TIMEOUT_SEC, parse_timeout};
    // 空 = 默认
    assert_eq!(parse_timeout("", 30).unwrap(), 30);
    assert_eq!(parse_timeout("   ", 30).unwrap(), 30);
    assert_eq!(parse_timeout("45", 30).unwrap(), 45);
    assert_eq!(
        parse_timeout(&MAX_TIMEOUT_SEC.to_string(), 30).unwrap(),
        MAX_TIMEOUT_SEC
    );
    for bad in ["0", "-1", "abc", "30s", "601", "1.5"] {
        assert!(parse_timeout(bad, 30).is_err(), "{bad} 应被拒");
    }
}

#[test]
fn bad_proxy_url_is_reported() {
    // 错值必须是「代理地址无效」这种能自查的报错，不能静默变成直连
    for bad in [
        "不是个代理地址",
        "http://",
        "http:// 127.0.0.1:7897",
        "socks5://",
    ] {
        let r = block_on(send(
            &req("http://127.0.0.1:1/"),
            Duration::from_secs(3),
            Some(bad),
        ));
        match &r.error {
            Some(HttpError::Other(m)) => assert!(m.contains("代理地址无效"), "{bad} → {m}"),
            other => panic!("{bad} 应报「代理地址无效」：{other:?}"),
        }
    }
    // 只填 host:port 也要能用（自动补 http://）；这里指向不存在的端口 ⇒ 连接被拒而非报「无效」
    let r = block_on(send(
        &req("http://127.0.0.1:1/"),
        Duration::from_secs(3),
        Some("127.0.0.1:1"),
    ));
    assert!(
        !matches!(&r.error, Some(HttpError::Other(m)) if m.contains("无效")),
        "host:port 应被接受：{:?}",
        r.error
    );
}

#[test]
fn check_proxy_strips_the_transport_prefix() {
    // 对话框里保存前自查用的入口：报错文案要能直接给用户看，不带 "request failed: "
    let e = treq_core::http::check_proxy("http://").unwrap_err();
    assert!(e.contains("代理地址无效"), "{e}");
    assert!(!e.contains("request failed"), "别把内部前缀带进界面：{e}");
    assert!(treq_core::http::check_proxy("127.0.0.1:7897").is_ok());
    assert!(treq_core::http::check_proxy("  http://127.0.0.1:7897  ").is_ok(), "两侧空格要 trim");
}

fn req(url: &str) -> RequestItem {
    RequestItem {
        id: "i".into(),
        name: "n".into(),
        method: "GET".into(),
        url: url.into(),
        params: vec![],
        headers: vec![],
        body: Body {
            kind: BodyKind::None,
            content: String::new(),
            form_data: Vec::new(),
        },
        description: String::new(),
        docs_open: true,
        auth: None,
    }
}

#[test]
fn send_get_reads_status_headers_body() {
    let addr = mock_server(
        "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nX-Test: 1\r\nContent-Length: 13\r\n\r\n{\"ok\":true}\r\n",
    );
    let r = block_on(send(
        &req(&format!("http://{}/a?b=1", addr)),
        Duration::from_secs(5),
        None,
    ));
    assert!(r.error.is_none());
    assert_eq!(r.status, Some(201));
    assert_eq!(r.status_text, "Created");
    assert!(r.headers.iter().any(|(k, v)| k == "x-test" && v == "1"));
    assert_eq!(String::from_utf8_lossy(&r.body).trim(), "{\"ok\":true}");
}

#[test]
fn send_connection_refused_reports_error() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    let r = block_on(send(
        &req(&format!("http://{}/", addr)),
        Duration::from_secs(3),
        None,
    ));
    assert!(matches!(r.error, Some(HttpError::Connect(_))));
    assert_eq!(r.status, None);
}

#[test]
fn send_skips_disabled_params_and_headers_echo() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let mut s = listener.accept().unwrap().0;
        let mut buf = [0u8; 16384];
        let n = s.read(&mut buf).unwrap();
        let received = String::from_utf8_lossy(&buf[..n]).to_string();
        let body = received
            .lines()
            .take_while(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = s.write_all(resp.as_bytes());
    });
    let mut r = req(&format!("http://{}/x", addr));
    r.params = vec![
        Kv {
            key: "on".into(),
            value: "1".into(),
            enabled: true,
            description: String::new(),
        },
        Kv {
            key: "off".into(),
            value: "2".into(),
            enabled: false,
            description: String::new(),
        },
    ];
    r.headers = vec![
        Kv {
            key: "X-On".into(),
            value: "yes".into(),
            enabled: true,
            description: String::new(),
        },
        Kv {
            key: "X-Off".into(),
            value: "no".into(),
            enabled: false,
            description: String::new(),
        },
    ];
    let resp = block_on(send(&r, Duration::from_secs(5), None));
    let head = String::from_utf8_lossy(&resp.body).to_string();
    assert!(
        head.contains("GET /x?on=1 HTTP/1.1"),
        "request line wrong: {}",
        head
    );
    assert!(
        head.contains("x-on: yes"),
        "enabled header missing: {}",
        head
    );
    assert!(
        !head.contains("off"),
        "disabled param/header leaked: {}",
        head
    );
    assert!(!head.contains("x-off"), "disabled header leaked: {}", head);
}

#[test]
fn pretty_json_works_and_falls_back() {
    assert_eq!(pretty_json(b"{\"a\":1}").unwrap(), "{\n  \"a\": 1\n}");
    assert_eq!(pretty_json(b"not json"), None);
    assert!(looks_json(Some("application/json"), b"{\"a\":1}"));
    assert!(!looks_json(Some("text/plain"), b"{\"a\":1}"));
    assert!(looks_json(
        Some("application/json; charset=utf-8"),
        b"plain"
    ));
}

#[test]
fn user_content_type_wins_over_default() {
    // 回显请求头，验证用户显式 Content-Type 不被默认值重复追加
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let mut s = listener.accept().unwrap().0;
        let mut buf = [0u8; 16384];
        let n = s.read(&mut buf).unwrap();
        let received = String::from_utf8_lossy(&buf[..n]).to_string();
        let ct_count = received
            .lines()
            .take_while(|l| !l.is_empty())
            .filter(|l| l.to_ascii_lowercase().starts_with("content-type:"))
            .count();
        let msg = format!("ct headers: {}", ct_count);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            msg.len(),
            msg
        );
        let _ = s.write_all(resp.as_bytes());
    });
    let mut r = req(&format!("http://{}/", addr));
    r.method = "POST".into();
    r.headers = vec![Kv {
        key: "Content-Type".into(),
        value: "text/plain".into(),
        enabled: true,
        description: String::new(),
    }];
    r.body = Body {
        kind: BodyKind::Json,
        content: "{}".into(),
        form_data: Vec::new(),
    };
    let resp = futures::executor::block_on(send(&r, Duration::from_secs(5), None));
    assert!(resp.error.is_none());
    let body = String::from_utf8_lossy(&resp.body).to_string();
    assert_eq!(
        body, "ct headers: 1",
        "user Content-Type must not be duplicated"
    );
}

#[test]
fn send_post_json_body() {
    let addr = mock_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let mut r = req(&format!("http://{}/", addr));
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Json,
        content: "{\"a\":1}".into(),
        form_data: Vec::new(),
    };
    let resp = block_on(send(&r, Duration::from_secs(5), None));
    assert!(resp.error.is_none());
}

/// 起一个本地服务把收到的原始请求原样回显，用于断言 Content-Type/body 编码。
fn echo_raw_request(r: &RequestItem) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            // 本地请求一般一次就到齐；用短超时把可用字节读全，避免 POST body 分片
            s.set_read_timeout(Some(Duration::from_millis(300))).ok();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match s.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
            }
            let received = String::from_utf8_lossy(&buf).to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                received.len(),
                received
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    let mut req = r.clone();
    req.url = format!("http://{}/", addr);
    let resp = block_on(send(&req, Duration::from_secs(5), None));
    assert!(resp.error.is_none(), "{:?}", resp.error);
    String::from_utf8_lossy(&resp.body).to_string()
}

#[test]
fn send_post_text_sets_plain_content_type() {
    let mut r = req("http://127.0.0.1:1/");
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Text,
        content: "hello".into(),
        form_data: Vec::new(),
    };
    let raw = echo_raw_request(&r);
    assert!(
        raw.to_ascii_lowercase()
            .contains("content-type: text/plain; charset=utf-8"),
        "{}",
        raw
    );
    assert!(raw.contains("hello"), "{}", raw);
}

#[test]
fn send_post_file_body_reads_file_bytes() {
    let dir = std::env::temp_dir().join(format!("treq-http-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("payload.bin");
    std::fs::write(&path, b"file-bytes").unwrap();
    let mut r = req("http://127.0.0.1:1/");
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::File,
        content: path.to_string_lossy().to_string(),
        form_data: Vec::new(),
    };
    let raw = echo_raw_request(&r);
    assert!(
        raw.to_ascii_lowercase()
            .contains("content-type: application/octet-stream"),
        "{}",
        raw
    );
    assert!(raw.contains("file-bytes"), "{}", raw);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_post_multipart_includes_fields_and_file() {
    let dir = std::env::temp_dir().join(format!("treq-http-multipart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("x.txt");
    std::fs::write(&path, b"FILE").unwrap();
    let mut r = req("http://127.0.0.1:1/");
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Multipart,
        content: String::new(),
        form_data: vec![
            FormField {
                key: "name".into(),
                value: "treq".into(),
                enabled: true,
                is_file: false,
            },
            FormField {
                key: "upload".into(),
                value: path.to_string_lossy().to_string(),
                enabled: true,
                is_file: true,
            },
        ],
    };
    let raw = echo_raw_request(&r);
    let lower = raw.to_ascii_lowercase();
    assert!(
        lower.contains("content-type: multipart/form-data; boundary="),
        "{}",
        raw
    );
    assert!(raw.contains("name=\"name\""), "{}", raw);
    assert!(raw.contains("treq"), "{}", raw);
    assert!(raw.contains("filename=\"x.txt\""), "{}", raw);
    assert!(raw.contains("FILE"), "{}", raw);
    let _ = std::fs::remove_dir_all(&dir);
}
// ---- 流式 / SSE ----

/// 分块发一个 SSE 响应：3 个事件，之间有小延时，最后不带 Content-Length 直接断开
fn sse_server(events: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 16384];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n",
            );
            for i in 0..events {
                let payload = format!("event: tick\ndata: {{\"i\":{}}}\n\n", i);
                let _ = s.write_all(format!("{:x}\r\n{}\r\n", payload.len(), payload).as_bytes());
                let _ = s.flush();
                thread::sleep(Duration::from_millis(20));
            }
            let _ = s.write_all(b"0\r\n\r\n");
            let _ = s.flush();
        }
    });
    addr.to_string()
}

#[test]
fn is_event_stream_matches_content_type() {
    let h = |v: &str| vec![("content-type".to_string(), v.to_string())];
    assert!(treq_core::http::is_event_stream(&h("text/event-stream")));
    assert!(treq_core::http::is_event_stream(&h(
        "text/event-stream; charset=utf-8"
    )));
    assert!(!treq_core::http::is_event_stream(&h("application/json")));
    assert!(!treq_core::http::is_event_stream(&[]));
}

#[test]
fn send_stream_pushes_events_live() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use treq_core::http::StreamEvent;

    let addr = sse_server(3);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();
    let cancel = Arc::new(AtomicBool::new(false));
    let url = format!("http://{}/sse", addr);
    let task = std::thread::spawn(move || {
        block_on(send_stream(
            &req(&url),
            Duration::from_secs(5),
            None,
            tx,
            cancel,
        ))
    });

    let started = std::time::Instant::now();
    let mut chunks: Vec<String> = Vec::new();
    let mut first_chunk_at = None;
    let mut head = None;
    while let Some(ev) = rx.blocking_recv() {
        match ev {
            StreamEvent::Head {
                status, headers, ..
            } => {
                head = Some(status);
                assert!(
                    treq_core::http::is_event_stream(&headers),
                    "Head 要告诉调用方这是事件流"
                );
            }
            StreamEvent::Chunk(c) => {
                if first_chunk_at.is_none() {
                    first_chunk_at = Some(started.elapsed());
                }
                chunks.push(String::from_utf8_lossy(&c).to_string());
            }
            StreamEvent::Done { error, .. } => {
                assert!(error.is_none(), "正常结束不该有错");
            }
        }
    }
    let r = task.join().unwrap();
    assert_eq!(head, Some(Some(200u16)));
    assert_eq!(r.status, Some(200));
    assert_eq!(
        chunks.len(),
        3,
        "3 个事件应该是 3 次推送（不是等全部收完才给）"
    );
    assert!(chunks.join("").contains("\"i\":2"));
    assert_eq!(
        String::from_utf8_lossy(&r.body),
        chunks.join(""),
        "返回值应等于累计正文"
    );
    let first = first_chunk_at.expect("应有分块");
    assert!(
        first < Duration::from_millis(150),
        "第一块应在整个流结束前就到（实测 {:?}）",
        first
    );
}

#[test]
fn send_stream_cancel_stops_reading() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use treq_core::http::StreamEvent;

    let addr = sse_server(50);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let url = format!("http://{}/sse", addr);
    let task = std::thread::spawn(move || {
        block_on(send_stream(
            &req(&url),
            Duration::from_secs(5),
            None,
            tx,
            cancel2,
        ))
    });

    // 收到第一块就取消
    let mut got = 0;
    while let Some(ev) = rx.blocking_recv() {
        if matches!(ev, StreamEvent::Chunk(_)) {
            got += 1;
            if got == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    let r = task.join().unwrap();
    assert!(got < 50, "取消后不该继续收满 50 块（收到 {}）", got);
    assert_eq!(r.status, Some(200));
    let _ = got;
}

/// 错误归因：reqwest 的英文链路 → 一句人话 + 建议。
#[cfg(test)]
mod error_attribution {
    use super::{block_on, req};
    use std::time::Duration;
    use treq_core::http::{ConnectKind, HttpError, conn_detail, send};

    #[test]
    fn classifies_real_connect_chains() {
        // 端口没人听（真实 reqwest 链路，本机 1 端口一般没人用）
        let r = block_on(send(
            &req("http://127.0.0.1:1/"),
            Duration::from_secs(3),
            None,
        ));
        let Some(e) = r.error else {
            panic!("应当连接失败")
        };
        assert_eq!(ConnectKind::of(e.raw().unwrap()), ConnectKind::Refused);
        let msg = e.to_string();
        assert!(msg.contains("拒绝连接"), "{msg}");
        assert!(msg.contains("服务没起"), "{msg}");
        assert!(msg.starts_with("连接失败："), "{msg}");
    }

    #[test]
    fn classifies_synthetic_chains() {
        let cases = [
            (
                "error sending request -> client error (Connect) -> tcp connect error -> Connection refused (os error 61)",
                ConnectKind::Refused,
            ),
            (
                "error sending request -> dns error: failed to lookup address information: Name or service not known",
                ConnectKind::Dns,
            ),
            (
                "error sending request -> tcp connect error -> Connection timed out (os error 60)",
                ConnectKind::TimedOut,
            ),
            (
                "error sending request -> invalid peer certificate: UnknownIssuer",
                ConnectKind::Tls,
            ),
            (
                "error sending request -> proxy connect error: Connection refused",
                ConnectKind::Proxy,
            ),
            ("weird thing happened", ConnectKind::Other),
        ];
        for (raw, kind) in cases {
            assert_eq!(ConnectKind::of(raw), kind, "{raw}");
            let msg = HttpError::Connect(raw.to_string()).to_string();
            assert!(msg.contains(kind.label()), "{raw} → {msg}");
            assert!(msg.contains(kind.hint()), "{raw} → {msg}");
        }
    }

    #[test]
    fn normalizes_scheme_less_urls() {
        use treq_core::http::normalize_url;
        assert_eq!(normalize_url("example.com/api"), "http://example.com/api");
        assert_eq!(
            normalize_url("127.0.0.1:8321/x?y=1"),
            "http://127.0.0.1:8321/x?y=1"
        );
        assert_eq!(normalize_url("  example.com  "), "http://example.com");
        // 已有协议 / 变量模板 / 协议相对 / 空串都不动
        assert_eq!(normalize_url("https://a/b"), "https://a/b");
        assert_eq!(normalize_url("ftp://a"), "ftp://a");
        assert_eq!(normalize_url("{{ baseUrl }}/x"), "{{ baseUrl }}/x");
        assert_eq!(normalize_url("//a/b"), "//a/b");
        assert_eq!(normalize_url(""), "");
    }

    #[test]
    fn scheme_less_url_actually_sends() {
        use std::io::Write;
        // 起个最小 server，确认「没写协议」也能打通
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = std::io::Read::read(&mut s, &mut buf);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            }
        });
        let r = block_on(send(
            &req(&format!("{addr}/ping")),
            Duration::from_secs(3),
            None,
        ));
        assert_eq!(r.status, Some(200), "{:?}", r.error);
        assert_eq!(r.body, b"ok");
    }

    #[test]
    fn unsupported_scheme_says_so() {
        let r = block_on(send(
            &req("ftp://127.0.0.1:1/"),
            Duration::from_secs(3),
            None,
        ));
        let msg = r.error.expect("应报错").to_string();
        assert!(msg.contains("协议不支持"), "{msg}");
    }

    #[test]
    fn timeout_and_detail_are_friendly() {
        let msg = HttpError::Timeout.to_string();
        assert!(msg.contains("超时") && msg.contains("设置 → 网络"), "{msg}");
        assert!(HttpError::Timeout.raw().is_none());
        let raw = "a -> b -> c -> d";
        assert_eq!(conn_detail(raw), "c → d");
        assert_eq!(conn_detail("only one"), "only one");
    }
}
