use std::fs;
use std::path::PathBuf;
use treq_core::trash::{TrashKind, TrashStore};
use treq_core::*;

fn tmp(tag: &str) -> (PathBuf, PathBuf) {
    let ws = std::env::temp_dir().join(format!("treq-trash-ws-{}-{}", tag, std::process::id()));
    let trash = std::env::temp_dir().join(format!("treq-trash-dir-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&ws);
    let _ = fs::remove_dir_all(&trash);
    (ws, trash)
}

#[test]
fn trash_restore_collection() {
    let (ws, trash) = tmp("col");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let req = store.create_request(&col.id, "R").unwrap();

    let t = TrashStore::new(trash.clone());
    t.trash_collection(&ws, &col.id).unwrap();
    assert!(!ws.join(format!("collections/{}", col.id)).exists());
    assert!(trash.join(&col.id).is_dir());

    let entries = t.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, TrashKind::Collection);
    assert_eq!(entries[0].collection_id, col.id);
    assert_eq!(entries[0].name, "C");

    t.restore(&ws, &col.id, None, None).unwrap();
    assert!(ws.join(format!("collections/{}", col.id)).is_dir());
    assert!(
        ws.join(format!("collections/{}/requests/{}.yml", col.id, req.id))
            .is_file()
    );
    assert!(t.list().unwrap().is_empty());
}

#[test]
fn trash_restore_request() {
    let (ws, trash) = tmp("req");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let req = store.create_request(&col.id, "R").unwrap();
    save_request_ext(&store, &col.id, &req);

    let t = TrashStore::new(trash.clone());
    t.trash_request(&ws, &col.id, None, &req.id).unwrap();
    assert!(
        !ws.join(format!("collections/{}/requests/{}.yml", col.id, req.id))
            .exists()
    );

    let entries = t.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, TrashKind::Request);
    assert_eq!(entries[0].request_id.as_deref(), Some(req.id.as_str()));
    assert_eq!(entries[0].group_id, None);

    t.restore(&ws, &col.id, None, Some(&req.id)).unwrap();
    assert!(
        ws.join(format!("collections/{}/requests/{}.yml", col.id, req.id))
            .is_file()
    );
}

#[test]
fn trash_request_then_collection_merges_and_restores() {
    let (ws, trash) = tmp("merge");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let r1 = store.create_request(&col.id, "R1").unwrap();
    let r2 = store.create_request(&col.id, "R2").unwrap();

    let t = TrashStore::new(trash.clone());
    // 先删 R1（散件），再删整个集合（目录合并）
    t.trash_request(&ws, &col.id, None, &r1.id).unwrap();
    t.trash_collection(&ws, &col.id).unwrap();
    assert!(!ws.join(format!("collections/{}", col.id)).exists());

    // list：集合 + 散件 R1（R2 在 requests/ 内不算散件）
    let entries = t.list().unwrap();
    let has_col = entries.iter().any(|e| e.kind == TrashKind::Collection);
    assert!(has_col);

    // 恢复集合：R1 归位到 requests/，R2 仍在 requests/
    t.restore(&ws, &col.id, None, None).unwrap();
    let base = ws.join(format!("collections/{}", col.id));
    assert!(base.join("collection.yml").is_file());
    assert!(
        base.join(format!("requests/{}.yml", r1.id)).is_file(),
        "散件必须归位到 requests/"
    );
    assert!(base.join(format!("requests/{}.yml", r2.id)).is_file());
    // load 能看到全部两个请求
    let loaded = store.load().unwrap();
    assert_eq!(loaded.collections[0].requests.len(), 2);
}

#[test]
fn empty_trash() {
    let (ws, trash) = tmp("empty");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let _ = store.create_request(&col.id, "R").unwrap();
    let t = TrashStore::new(trash.clone());
    t.trash_collection(&ws, &col.id).unwrap();
    assert_eq!(t.list().unwrap().len(), 1);
    t.empty().unwrap();
    assert!(t.list().unwrap().is_empty());
    assert!(!trash.exists());
}

fn save_request_ext(store: &WorkspaceStore, _col_id: &str, req: &RequestItem) {
    store.save_request(req).unwrap();
}

/// 组内请求 + 整个分组：删除都进回收站（带 group_id），都能恢复。
#[test]
fn trash_group_and_group_request() {
    let (ws, trash) = tmp("group");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let grp = store.create_group(&col.id, "G", None).unwrap();
    let req = store
        .create_request_in(&col.id, Some(&grp.id), "R")
        .unwrap();
    store.save_request(&req).unwrap();

    let t = TrashStore::new(trash.clone());
    t.trash_request(&ws, &col.id, Some(&grp.id), &req.id).unwrap();
    assert!(
        !ws.join(format!(
            "collections/{}/groups/{}/requests/{}.yml",
            col.id, grp.id, req.id
        ))
        .exists()
    );
    let entries = t.list().unwrap();
    let e = entries.first().expect("回收站里要有这条请求");
    assert_eq!(e.kind, TrashKind::Request);
    assert_eq!(e.group_id.as_deref(), Some(grp.id.as_str()));
    assert_eq!(e.request_id.as_deref(), Some(req.id.as_str()));

    t.restore(&ws, &col.id, Some(&grp.id), Some(&req.id)).unwrap();
    assert!(
        ws.join(format!(
            "collections/{}/groups/{}/requests/{}.yml",
            col.id, grp.id, req.id
        ))
        .is_file()
    );

    // 整个分组删除 → 列表里同时能看到「分组」和它里面的请求
    t.trash_group(&ws, &col.id, &grp.id).unwrap();
    assert!(
        !ws.join(format!("collections/{}/groups/{}", col.id, grp.id))
            .exists()
    );
    let entries = t.list().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.kind == TrashKind::Group && e.group_id.as_deref() == Some(grp.id.as_str()))
    );
    assert!(
        entries
            .iter()
            .any(|e| e.request_id.as_deref() == Some(req.id.as_str()))
    );

    // 恢复分组：请求跟着回来
    t.restore(&ws, &col.id, Some(&grp.id), None).unwrap();
    assert!(
        ws.join(format!(
            "collections/{}/groups/{}/requests/{}.yml",
            col.id, grp.id, req.id
        ))
        .is_file()
    );
    assert!(t.list().unwrap().is_empty());
}
