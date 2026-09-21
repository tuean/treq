//! 跨模块流程测试：把平时点出来的整条链走一遍（新建 → 编辑 → 移动 → 回收站 → 还原），
//! 断言的是**落盘后的真实文件**，不是内存里那一份。
//!
//! 单模块的细节在各自的测试文件里；这里只管「串起来还对不对」。历史上踩过的坑都属于这一类：
//! 分组里的请求改名不落盘、移动之后同一请求出现两个文件、trash 之后重新 load 还看得见。

use std::path::{Path, PathBuf};
use treq_core::trash::{TrashKind, TrashStore};
use treq_core::*;

fn fresh(tag: &str) -> (PathBuf, PathBuf) {
    let ws = std::env::temp_dir().join(format!("treq-flow-ws-{}-{}", tag, std::process::id()));
    let trash = std::env::temp_dir().join(format!("treq-flow-trash-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&ws);
    let _ = std::fs::remove_dir_all(&trash);
    (ws, trash)
}

/// 从磁盘重新读一遍（模拟「重启 app」）
fn reload(ws: &Path) -> Workspace {
    WorkspaceStore::new(ws.to_path_buf()).load().unwrap()
}

/// 请求在哪：(集合 id, 分组 id)。找不到 → None
fn container_of(ws: &Workspace, request_id: &str) -> Option<(String, Option<String>)> {
    for c in &ws.collections {
        if c.requests.iter().any(|r| r.id == request_id) {
            return Some((c.id.clone(), None));
        }
        for g in &c.groups {
            if g.requests.iter().any(|r| r.id == request_id) {
                return Some((c.id.clone(), Some(g.id.clone())));
            }
        }
    }
    None
}

/// 这个请求 id 在磁盘上一共有几个文件（重复落盘会 >1）
fn files_for(ws: &Path, request_id: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![ws.join("collections")];
    while let Some(d) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.file_name().is_some_and(|n| n == format!("{request_id}.yml").as_str()) {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn new_edit_and_reload_keep_one_file_per_request() {
    let (ws, _) = fresh("edit");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();

    let col = store.create_collection("集合A").unwrap();
    let group = store.create_group(&col.id, "分组1", None).unwrap();
    let in_group = store.create_request_in(&col.id, Some(&group.id), "分组内").unwrap();
    let flat = store.create_request(&col.id, "直属").unwrap();

    // 分组里的请求落在分组目录里；保存（改名 / 改 URL / 改方法）不会另起一个文件
    let mut req = reload(&ws)
        .collections
        .iter()
        .flat_map(|c| c.groups.iter().flat_map(|g| g.requests.iter()))
        .find(|r| r.id == in_group.id)
        .unwrap()
        .clone();
    req.name = "分组内（改名）".into();
    req.url = "https://example.com/api/user/{{token}}".into();
    req.method = "POST".into();
    req.body.kind = BodyKind::Json;
    req.body.content = r#"{"a":1}"#.into();
    store.save_request(&req).unwrap();

    assert_eq!(files_for(&ws, &in_group.id).len(), 1, "保存不能产生第二份文件");
    let back = reload(&ws);
    let got = back
        .collections
        .iter()
        .flat_map(|c| c.groups.iter().flat_map(|g| g.requests.iter()))
        .find(|r| r.id == in_group.id)
        .unwrap();
    assert_eq!(got.name, "分组内（改名）");
    assert_eq!(got.method, "POST");
    assert_eq!(got.url, "https://example.com/api/user/{{token}}");
    assert_eq!(got.body.kind, BodyKind::Json);
    assert_eq!(
        container_of(&back, &in_group.id),
        Some((col.id.clone(), Some(group.id.clone())))
    );
    assert_eq!(
        container_of(&back, &flat.id),
        Some((col.id.clone(), None))
    );

    // 变量：请求用到 {{token}}，环境里没有 → undefined；建环境写上 → 有了
    let mut merged = vars::merge_env(&back.base_env, back.environments.first());
    assert_eq!(vars::undefined_vars(got, &merged), vec!["token".to_string()]);
    let mut env = store.new_environment("dev", false).unwrap();
    env.variables.insert("token".into(), "abc".into());
    store.save_environment(&env).unwrap();
    let back2 = reload(&ws);
    let dev = back2
        .environments
        .iter()
        .find(|e| e.id == env.id)
        .expect("环境要落盘");
    merged = vars::merge_env(&back2.base_env, Some(dev));
    assert!(
        vars::undefined_vars(got, &merged).is_empty(),
        "变量补齐后不该再报缺"
    );
}

#[test]
fn moving_a_request_never_leaves_a_copy_behind() {
    let (ws, _) = fresh("move");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();

    let a = store.create_collection("A").unwrap();
    let b = store.create_collection("B").unwrap();
    let gb = store.create_group(&b.id, "B 的分组", None).unwrap();
    let ga = store.create_group(&a.id, "A 的分组", None).unwrap();
    let req = store.create_request(&a.id, "搬来搬去").unwrap();

    for (to_col, to_group) in [
        (&b.id, None),
        (&b.id, Some(gb.id.as_str())),
        (&a.id, Some(ga.id.as_str())),
        (&a.id, None),
        (&b.id, None),
    ] {
        let path = store.move_request(&req.id, to_col, to_group).unwrap();
        assert!(path.is_file(), "移动后文件要在新位置");
        assert_eq!(files_for(&ws, &req.id), vec![path.clone()], "只能有一份");
        assert_eq!(
            store.find_request_file(&req.id).unwrap(),
            path,
            "索引要跟着走"
        );
        let loaded = reload(&ws);
        assert_eq!(
            container_of(&loaded, &req.id),
            Some((
                to_col.clone(),
                to_group.map(|g| g.to_string())
            )),
            "load 后归属要和搬去的目标一致"
        );
    }
}

#[test]
fn trash_restore_round_trip_through_the_disk() {
    let (ws, trash_dir) = fresh("trash");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let t = TrashStore::new(trash_dir.clone());

    let col = store.create_collection("集合").unwrap();
    let group = store.create_group(&col.id, "分组", None).unwrap();
    let kept = store.create_request(&col.id, "留着").unwrap();
    let grouped = store
        .create_request_in(&col.id, Some(&group.id), "分组里")
        .unwrap();

    // 单个请求：删掉 → 列表里看得见 → load 看不见 → 还原回原分组
    t.trash_request(&ws, &col.id, Some(&group.id), &grouped.id)
        .unwrap();
    assert!(!files_for(&ws, &grouped.id).iter().any(|p| p.starts_with(&ws)));
    assert!(container_of(&reload(&ws), &grouped.id).is_none(), "删了就不该在树里");
    let entries = t.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, TrashKind::Request);
    assert_eq!(entries[0].name, "分组里");
    assert_eq!(entries[0].group_id.as_deref(), Some(group.id.as_str()), "要记得原来在哪个分组");

    t.restore(&ws, &col.id, Some(&group.id), Some(&grouped.id))
        .unwrap();
    assert_eq!(
        container_of(&reload(&ws), &grouped.id),
        Some((col.id.clone(), Some(group.id.clone())))
    );
    assert!(t.list().unwrap().is_empty(), "还原后回收站要空");

    // 整个分组（里头的请求一起走）→ 还原
    t.trash_group(&ws, &col.id, &group.id).unwrap();
    let loaded = reload(&ws);
    assert!(loaded.collections[0].groups.is_empty(), "分组没了");
    assert!(container_of(&loaded, &kept.id).is_some(), "同集合的直属请求不受影响");
    // 回收站列表是「分组 + 里面的请求」都列出来（界面上按分组展示），所以这里只看分组在不在
    let entries = t.list().unwrap();
    assert!(
        entries.iter().any(|e| {
            e.kind == TrashKind::Group && e.group_id.as_deref() == Some(group.id.as_str())
        }),
        "分组要进回收站：{entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|e| e.kind == TrashKind::Request && e.request_id.as_deref() == Some(grouped.id.as_str())),
        "分组里的请求跟着一起进"
    );
    t.restore(&ws, &col.id, Some(&group.id), None).unwrap();
    assert!(t.list().unwrap().is_empty(), "连里面的请求一起还原，回收站要空");
    assert_eq!(
        container_of(&reload(&ws), &grouped.id),
        Some((col.id.clone(), Some(group.id.clone()))),
        "分组还原要带着里面的请求回来"
    );

    // 整个集合 → 还原 → 清空回收站
    t.trash_collection(&ws, &col.id).unwrap();
    assert!(reload(&ws).collections.is_empty());
    t.restore(&ws, &col.id, None, None).unwrap();
    let back = reload(&ws);
    assert_eq!(back.collections.len(), 1);
    assert!(container_of(&back, &kept.id).is_some());
    assert!(container_of(&back, &grouped.id).is_some());

    t.trash_request(&ws, &col.id, None, &kept.id).unwrap();
    t.empty().unwrap();
    assert!(t.list().unwrap().is_empty());
    assert!(!files_for(&ws, &kept.id).iter().any(|p| p.starts_with(&ws)));
}

#[test]
fn new_item_target_files_do_not_collide() {
    // 同名请求 / 同名分组连着建，靠 id 区分，谁都不会覆盖谁
    let (ws, _) = fresh("dupe");
    let store = WorkspaceStore::new(ws.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("同名集合").unwrap();
    let col2 = store.create_collection("同名集合").unwrap();
    assert_ne!(col.id, col2.id);

    let a = store.create_request(&col.id, "同名").unwrap();
    let b = store.create_request(&col.id, "同名").unwrap();
    let c = store.create_request(&col2.id, "同名").unwrap();
    assert_eq!(files_for(&ws, &a.id).len(), 1);
    assert_eq!(files_for(&ws, &b.id).len(), 1);
    assert_eq!(files_for(&ws, &c.id).len(), 1);

    // 同名分组也是
    let g1 = store.create_group(&col.id, "同名分组", None).unwrap();
    let g2 = store.create_group(&col.id, "同名分组", None).unwrap();
    assert_ne!(g1.id, g2.id);
    let loaded = reload(&ws);
    let groups = loaded
        .collections
        .iter()
        .find(|x| x.id == col.id)
        .unwrap()
        .groups
        .clone();
    assert_eq!(groups.len(), 2, "两个同名分组都要在");
    assert!(groups.iter().any(|g| g.id == g1.id));
    assert!(groups.iter().any(|g| g.id == g2.id));
    assert_eq!(
        loaded
            .collections
            .iter()
            .find(|x| x.id == col.id)
            .unwrap()
            .requests
            .len(),
        2
    );
}
