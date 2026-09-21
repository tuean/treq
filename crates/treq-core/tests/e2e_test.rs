//! 端到端链路：YAML 工作区 → 环境解析 → HTTP 发送（tokio runtime）→ 历史入库。

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use treq_core::history::{HistoryEntry, HistoryStore, now_millis};
use treq_core::vars;
use treq_core::*;

fn mock_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let addr = format!("http://{}", addr);
    thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 16384];
            let n = s.read(&mut buf).unwrap();
            let received = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = received
                .lines()
                .take_while(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("|");
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
        }
    });
    addr
}

#[test]
fn end_to_end_send_and_history() {
    let root = std::env::temp_dir().join(format!("treq-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();

    // 1. 工作区：base 环境 + 请求模板（带变量）
    let mut base = store.load().unwrap().base_env;
    base.variables.insert("baseUrl".into(), mock_server());
    store.save_environment(&base).unwrap();
    let col = store.create_collection("E2E").unwrap();
    let mut req = store.create_request(&col.id, "get").unwrap();
    req.url = "{{ baseUrl }}/users/42?full=1".into();
    req.headers.push(Kv {
        key: "X-E2E".into(),
        value: "yes".into(),
        enabled: true,
        description: String::new(),
    });
    store.save_request(&req).unwrap();

    // 2. 加载 + 解析（app 的 send_request 同款流程）
    let ws = store.load().unwrap();
    let vars = vars::merge_env(&ws.base_env, None);
    let resolved = vars::resolve_request(&ws.collections[0].requests[0], &vars);

    // 3. 发送（tokio runtime 修复后，gpui executor 环境外也必须可用）
    let template = ws.collections[0].requests[0].clone();
    let r = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(treq_core::http::send(
            &resolved,
            Duration::from_secs(5),
            None,
        ));
    assert!(r.error.is_none());
    assert_eq!(r.status, Some(200));
    let head = String::from_utf8_lossy(&r.body).to_string();
    assert!(head.contains("GET /users/42?full=1"), "{}", head);
    assert!(head.contains("x-e2e: yes"), "{}", head);

    // 4. 历史入库（app 的 history_store.insert 同款调用）
    let db = root.join("history.db");
    let hstore = HistoryStore::open(db).unwrap();
    hstore
        .insert(&HistoryEntry {
            id: 0,
            sent_at: now_millis(),
            status: r.status,
            duration_ms: r.duration.as_millis() as u64,
            error: r.error.as_ref().map(|e| e.to_string()),
            request: template.clone(),
        })
        .unwrap();
    let list = hstore.list(1).unwrap();
    assert_eq!(list.len(), 1);
    // 快照保留了变量模板（恢复后仍可编辑）
    assert_eq!(list[0].request.url, "{{ baseUrl }}/users/42?full=1");
    assert_eq!(list[0].status, Some(200));
    assert_eq!(list[0].request.headers[0].value, "yes");

    let _ = fs::remove_dir_all(&root);
}
