use std::fs;
use std::path::PathBuf;
use treq_core::RequestItem;
use treq_core::history::{HistoryEntry, HistoryStore};

fn entry(method: &str, url: &str) -> HistoryEntry {
    HistoryEntry {
        id: 0,
        sent_at: 1_700_000_000_000,
        status: Some(200),
        duration_ms: 12,
        error: None,
        request: RequestItem {
            id: "req-1".into(),
            name: "n".into(),
            method: method.into(),
            url: url.into(),
            params: vec![],
            headers: vec![],
            body: treq_core::Body {
                kind: treq_core::BodyKind::Json,
                content: "{\"a\":1}".into(),
                form_data: Vec::new(),
            },
            description: String::new(),
            docs_open: true,
            auth: None,
            order: None,
        },
    }
}

fn tmp_db(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("treq-history-{}-{}.db", tag, std::process::id()));
    let _ = fs::remove_file(&p);
    p
}

#[test]
fn insert_list_delete() {
    let db = tmp_db("basic");
    let store = HistoryStore::open(db.clone()).unwrap();
    let e1 = entry("GET", "https://a.com/1");
    let e2 = entry("POST", "https://a.com/2");
    store.insert(&e1).unwrap();
    store.insert(&e2).unwrap();

    let list = store.list(10).unwrap();
    assert_eq!(list.len(), 2);
    // id 倒序：最新的在前
    assert_eq!(list[0].request.url, "https://a.com/2");
    assert_eq!(list[1].request.url, "https://a.com/1");
    assert_eq!(list[0].status, Some(200));
    assert_eq!(list[1].request.body.content, "{\"a\":1}");

    store.delete(list[0].id).unwrap();
    assert_eq!(store.count().unwrap(), 1);
    assert_eq!(store.list(10).unwrap()[0].request.method, "GET");
}

#[test]
fn list_respects_limit() {
    let db = tmp_db("limit");
    let store = HistoryStore::open(db).unwrap();
    for i in 0..5 {
        store
            .insert(&entry("GET", &format!("https://a.com/{}", i)))
            .unwrap();
    }
    let list = store.list(3).unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].request.url, "https://a.com/4");
}

#[test]
fn open_reopens_existing_db() {
    let db = tmp_db("reopen");
    let store = HistoryStore::open(db.clone()).unwrap();
    store.insert(&entry("GET", "https://a.com/")).unwrap();
    drop(store);
    let store2 = HistoryStore::open(db).unwrap();
    assert_eq!(store2.count().unwrap(), 1);
}
#[test]
fn list_for_request_only_returns_that_request() {
    let dir = tmp_db("for_request");
    let store = HistoryStore::open(dir.join("h.db")).unwrap();
    for (req, url) in [
        ("req-a", "http://h/a"),
        ("req-b", "http://h/b"),
        ("req-a", "http://h/a2"),
    ] {
        let mut e = entry("GET", url);
        e.request.id = req.to_string();
        store.insert(&e).unwrap();
    }
    let a = store.list_for_request("req-a", 10).unwrap();
    assert_eq!(a.len(), 2);
    assert!(a.iter().all(|h| h.request.id == "req-a"));
    // id 倒序：最新一条在前
    assert_eq!(a[0].request.url, "http://h/a2");
    assert_eq!(store.list_for_request("req-a", 1).unwrap().len(), 1);
    assert!(store.list_for_request("nope", 10).unwrap().is_empty());
    std::fs::remove_dir_all(dir).ok();
}
