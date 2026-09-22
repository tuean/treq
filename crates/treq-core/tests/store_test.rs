use std::fs;
use std::path::PathBuf;
use treq_core::*;

fn tmp_ws(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("treq-test-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
}

#[test]
fn nested_group_parents_round_trip_and_dangling_parent_falls_back_to_top_level() {
    let root = tmp_ws("nested-group");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let a = store.create_group(&col.id, "A", None).unwrap();
    let b = store.create_group(&col.id, "B", Some(a.id.clone())).unwrap();
    let c = store.create_group(&col.id, "C", Some(b.id.clone())).unwrap();
    let deep = store
        .create_request_in(&col.id, Some(&c.id), "deep")
        .unwrap();

    // 盘上还是平铺的：collections/<cid>/groups/<gid>/…，层级只在 group.yml 里
    assert!(
        root.join(format!(
            "collections/{}/groups/{}/requests/{}.yml",
            col.id, c.id, deep.id
        ))
        .is_file()
    );

    let ws = store.load().unwrap();
    let gs = &ws.collections[0].groups;
    assert_eq!(gs.len(), 3);
    let parent_of = |id: &str| {
        gs.iter()
            .find(|g| g.id == id)
            .unwrap()
            .parent
            .clone()
    };
    assert_eq!(parent_of(&a.id), None);
    assert_eq!(parent_of(&b.id), Some(a.id.clone()));
    assert_eq!(parent_of(&c.id), Some(b.id.clone()));
    assert_eq!(
        gs.iter().find(|g| g.id == c.id).unwrap().requests[0].id,
        deep.id
    );

    // 悬空 / 自引用的 parent（手改 yml、回收站只还原子分组）当第一层，不能凭空消失
    let f = root.join(format!("collections/{}/groups/{}/group.yml", col.id, a.id));
    fs::write(&f, format!("id: {}\nname: A\nparent: ghost\n", a.id)).unwrap();
    let ws = store.load().unwrap();
    assert_eq!(
        ws.collections[0]
            .groups
            .iter()
            .find(|g| g.id == a.id)
            .unwrap()
            .parent,
        None
    );
}

#[test]
fn ensure_root_creates_layout() {
    let root = tmp_ws("layout");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    assert!(root.join("collections").is_dir());
    assert!(root.join("environments/base.yml").is_file());
}

#[test]
fn request_yaml_round_trip() {
    let mut req = RequestItem {
        id: new_id(),
        name: "Get User".into(),
        method: "GET".into(),
        url: "https://{{ baseUrl }}/users/1".into(),
        params: vec![Kv {
            key: "limit".into(),
            value: "10".into(),
            enabled: true,
            description: String::new(),
        }],
        headers: vec![Kv {
            key: "Accept".into(),
            value: "application/json".into(),
            enabled: true,
            description: String::new(),
        }],
        body: Body {
            kind: BodyKind::Json,
            content: "{\"a\":1}".into(),
            form_data: Vec::new(),
        },
        description: "说明".into(),
        docs_open: true,
        auth: None,
        order: None,
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    let back: RequestItem = serde_yaml::from_str(&yaml).unwrap();
    assert!(matches!(back.body.kind, BodyKind::Json));
    assert_eq!(back.url, req.url);
    req.id = back.id.clone();
    req.name = back.name.clone();
    assert_eq!(
        serde_yaml::to_value(&back).unwrap(),
        serde_yaml::to_value(&req).unwrap()
    );
}

#[test]
fn create_and_save_request_persists_file() {
    let root = tmp_ws("crud");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("My Collection").unwrap();
    let mut req = store.create_request(&col.id, "Get User").unwrap();
    req.url = "https://example.com/".into();
    req.method = "POST".into();
    store.save_request(&req).unwrap();
    let file = root.join(format!("collections/{}/requests/{}.yml", col.id, req.id));
    assert!(file.is_file());
    let from_disk =
        serde_yaml::from_str::<RequestItem>(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(from_disk.url, "https://example.com/");
    assert_eq!(from_disk.method, "POST");
}

#[test]
fn load_reconstructs_workspace() {
    let root = tmp_ws("load");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C1").unwrap();
    let req = store.create_request(&col.id, "R1").unwrap();
    let _env = store.new_environment("dev", false).unwrap();
    let ws = store.load().unwrap();
    assert_eq!(ws.collections.len(), 1);
    assert_eq!(ws.collections[0].name, "C1");
    assert_eq!(ws.collections[0].requests.len(), 1);
    assert_eq!(ws.collections[0].requests[0].id, req.id);
    assert_eq!(ws.base_env.name, "Base");
    assert_eq!(ws.environments.len(), 1);
    assert_eq!(ws.environments[0].name, "dev");
}

#[test]
fn save_base_environment_writes_base_yml() {
    let root = tmp_ws("base-env");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let mut base = store.load().unwrap().base_env;
    base.variables.insert("baseUrl".into(), "http://x".into());
    store.save_environment(&base).unwrap();
    // base.yml 应有变量；environments/ 下不产生多余文件
    let on_disk: treq_core::Environment =
        serde_yaml::from_str(&fs::read_to_string(root.join("environments/base.yml")).unwrap())
            .unwrap();
    assert_eq!(on_disk.variables.get("baseUrl").unwrap(), "http://x");
    let n_yml = fs::read_dir(root.join("environments")).unwrap().count();
    assert_eq!(n_yml, 1);
    // 重新加载可见
    assert_eq!(
        store
            .load()
            .unwrap()
            .base_env
            .variables
            .get("baseUrl")
            .unwrap(),
        "http://x"
    );
}

#[test]
fn group_round_trip_and_nested_request() {
    let root = tmp_ws("group");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let group = store.create_group(&col.id, "Auth", None).unwrap();
    let req = store
        .create_request_in(&col.id, Some(&group.id), "Login")
        .unwrap();
    save_group_ext(&store, &col.id, &group);

    let ws = store.load().unwrap();
    assert_eq!(ws.collections[0].groups.len(), 1);
    assert_eq!(ws.collections[0].groups[0].name, "Auth");
    assert_eq!(ws.collections[0].groups[0].requests.len(), 1);
    assert_eq!(ws.collections[0].groups[0].requests[0].id, req.id);

    // 分组内请求可删
    store.delete_request(&col.id, &req.id).unwrap();
    assert!(
        !root
            .join(format!(
                "collections/{}/groups/{}/requests/{}.yml",
                col.id, group.id, req.id
            ))
            .exists()
    );
}

fn save_group_ext(store: &WorkspaceStore, col_id: &str, g: &treq_core::Group) {
    store.save_group(col_id, g).unwrap();
}

#[test]
fn delete_request_removes_file() {
    let root = tmp_ws("del");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let req = store.create_request(&col.id, "R").unwrap();
    store.delete_request(&col.id, &req.id).unwrap();
    let f = root.join(format!("collections/{}/requests/{}.yml", col.id, req.id));
    assert!(!f.exists());
}
#[test]
fn docs_open_defaults_true_and_round_trips() {
    // 老 YAML 没有 docs_open 字段：默认展开
    let legacy = "id: x\nname: n\nmethod: GET\nurl: https://x\nparams: []\nheaders: []\nbody:\n  kind: 'none'\n  content: ''\n";
    let req: RequestItem = serde_yaml::from_str(legacy).unwrap();
    assert!(req.docs_open, "老文件应默认展开 DOCS");

    // 折叠后写盘能读回来
    let mut collapsed = req.clone();
    collapsed.docs_open = false;
    let yaml = serde_yaml::to_string(&collapsed).unwrap();
    assert!(yaml.contains("docs_open: false"));
    let back: RequestItem = serde_yaml::from_str(&yaml).unwrap();
    assert!(!back.docs_open);
}

#[test]
fn saving_grouped_request_does_not_duplicate_it() {
    let dir = std::env::temp_dir().join(format!("treq-store-group-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let store = WorkspaceStore::new(dir.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("c").unwrap();
    let group = store.create_group(&col.id, "g", None).unwrap();
    let mut req = store
        .create_request_in(&col.id, Some(&group.id), "r")
        .unwrap();

    req.url = "https://changed".into();
    store.save_request(&req).unwrap();

    let in_group = dir.join(format!(
        "collections/{}/groups/{}/requests/{}.yml",
        col.id, group.id, req.id
    ));
    let at_root = dir.join(format!("collections/{}/requests/{}.yml", col.id, req.id));
    assert!(in_group.exists(), "分组内的文件必须原地更新");
    assert!(
        !at_root.exists(),
        "不得在集合根目录留副本（编辑=复制 的历史 bug）"
    );
    let back: RequestItem = serde_yaml::from_str(&fs::read_to_string(&in_group).unwrap()).unwrap();
    assert_eq!(back.url, "https://changed");
    assert_eq!(store.find_request_file(&req.id).unwrap(), in_group);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn request_path_index_tracks_lifecycle() {
    let dir = std::env::temp_dir().join(format!("treq-store-index-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let store = WorkspaceStore::new(dir.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("c").unwrap();
    let g = store.create_group(&col.id, "g", None).unwrap();
    let a = store.create_request(&col.id, "顶层").unwrap();
    let b = store
        .create_request_in(&col.id, Some(&g.id), "分组内")
        .unwrap();
    assert_eq!(store.indexed_requests(), 2, "创建后立即登记索引");

    // 冷启动（新 store 实例）应能从磁盘把索引建回来
    let store2 = WorkspaceStore::new(dir.clone());
    let ws = store2.load().unwrap();
    assert_eq!(store2.indexed_requests(), 2, "load 时填充索引");
    assert_eq!(
        ws.collections[0].groups[0].requests.len() + ws.collections[0].requests.len(),
        2
    );
    let mut a2 = a.clone();
    a2.url = "https://x".into();
    store2.save_request(&a2).unwrap();
    assert!(store2.find_request_file(&a.id).unwrap().exists());

    // 删除后索引不能留着死指针
    store2.delete_request(&col.id, &b.id).unwrap();
    assert_eq!(store2.indexed_requests(), 1);
    assert!(store2.find_request_file(&b.id).is_err());

    // 删分组/集合要按前缀清理
    store2.delete_group(&col.id, &g.id).unwrap();
    store2.delete_collection(&col.id).unwrap();
    assert_eq!(store2.indexed_requests(), 0, "删集合后索引应清空");

    let _ = fs::remove_dir_all(&dir);
    let _ = g;
}

#[test]
fn move_request_between_collections_and_groups() {
    let dir = std::env::temp_dir().join(format!("treq-store-move-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let store = WorkspaceStore::new(dir.clone());
    store.ensure_root().unwrap();
    let a = store.create_collection("A").unwrap();
    let b = store.create_collection("B").unwrap();
    let g = store.create_group(&b.id, "子分组", None).unwrap();
    let mut req = store.create_request(&a.id, "要挪的").unwrap();
    req.url = "https://x/keep".into();
    store.save_request(&req).unwrap();

    // A 根 → B/子分组
    let moved = store.move_request(&req.id, &b.id, Some(&g.id)).unwrap();
    assert!(moved.exists() && moved.ends_with(format!("requests/{}.yml", req.id)));
    assert!(
        !dir.join(format!("collections/{}/requests/{}.yml", a.id, req.id))
            .exists()
    );
    assert_eq!(
        store.find_request_file(&req.id).unwrap(),
        moved,
        "索引跟着挪"
    );

    // 内容没丢
    let back: RequestItem = serde_yaml::from_str(&fs::read_to_string(&moved).unwrap()).unwrap();
    assert_eq!(back.url, "https://x/keep");

    // 幂等：已经在目标位置再来一次也不炸
    assert_eq!(
        store.move_request(&req.id, &b.id, Some(&g.id)).unwrap(),
        moved
    );

    // 再挪回 A 根目录
    let back_home = store.move_request(&req.id, &a.id, None).unwrap();
    assert!(back_home.exists());
    assert_eq!(store.find_request_file(&req.id).unwrap(), back_home);

    // load() 反映新位置
    let ws = store.load().unwrap();
    let ca = ws.collections.iter().find(|c| c.id == a.id).unwrap();
    assert!(ca.requests.iter().any(|r| r.id == req.id));
    let cb = ws.collections.iter().find(|c| c.id == b.id).unwrap();
    assert!(
        cb.groups
            .iter()
            .all(|g| g.requests.iter().all(|r| r.id != req.id))
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn move_request_between_groups_and_keep_order_on_disk() {
    let root = tmp_ws("move-req");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let g1 = store.create_group(&col.id, "G1", None).unwrap();
    let g2 = store.create_group(&col.id, "G2", None).unwrap();
    let r1 = store.create_request(&col.id, "r1").unwrap();
    let r2 = store.create_request(&col.id, "r2").unwrap();
    let r3 = store.create_request(&col.id, "r3").unwrap();

    // 顶层顺序 r3、r1、r2（复制/拖拽就是这么写的）
    let top: Vec<String> = [&r3, &r1, &r2].iter().map(|r| r.id.clone()).collect();
    store.set_request_orders(&top).unwrap();
    let ws = store.load().unwrap();
    let names: Vec<String> = ws.collections[0]
        .requests
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(names, vec!["r3", "r1", "r2"], "重开也按 order 排");

    // 把 r1、(顺序靠前的) r3 都挪进 G1：G1 里 r3 在前
    store.move_request(&r1.id, &col.id, Some(&g1.id)).unwrap();
    store.move_request(&r3.id, &col.id, Some(&g1.id)).unwrap();
    store.set_request_orders(&[r3.id.clone(), r1.id.clone()]).unwrap();
    let ws = store.load().unwrap();
    let colb = ws.collections.iter().find(|c| c.id == col.id).unwrap();
    let g1b = colb.groups.iter().find(|g| g.id == g1.id).unwrap();
    assert_eq!(
        g1b.requests.iter().map(|r| &r.name).collect::<Vec<_>>(),
        vec!["r3", "r1"]
    );
    assert_eq!(
        colb.requests.iter().map(|r| &r.name).collect::<Vec<_>>(),
        vec!["r2"],
        "挪走的请求不会留在原地"
    );
    // 索引也跟着修好了：还能原地保存回分组目录
    let mut moved = g1b.requests[1].clone();
    moved.name = "r1-改名".into();
    store.save_request(&moved).unwrap();
    assert!(root
        .join(format!(
            "collections/{}/groups/{}/requests/{}.yml",
            col.id, g1.id, r1.id
        ))
        .is_file());
    let _ = g2;
}

#[test]
fn set_group_parent_moves_dir_and_refuses_nothing_else() {
    let root = tmp_ws("move-group");
    let store = WorkspaceStore::new(root.clone());
    store.ensure_root().unwrap();
    let col = store.create_collection("C").unwrap();
    let col2 = store.create_collection("C2").unwrap();
    let parent = store.create_group(&col.id, "P", None).unwrap();
    let child = store.create_group(&col.id, "K", None).unwrap();
    let deep = store
        .create_request_in(&col.id, Some(&child.id), "deep")
        .unwrap();

    // K 挂到 P 下面，并且跨到另一个集合也不丢请求
    store
        .set_group_parent(&child.id, &col.id, Some(&parent.id))
        .unwrap();
    store.set_group_orders(&[child.id.clone(), parent.id.clone()]).unwrap();
    let ws = store.load().unwrap();
    let groups = &ws.collections.iter().find(|c| c.id == col.id).unwrap().groups;
    assert_eq!(
        groups.iter().find(|g| g.id == child.id).unwrap().parent,
        Some(parent.id.clone())
    );
    store.set_group_parent(&parent.id, &col2.id, None).unwrap();
    assert!(root
        .join(format!("collections/{}/groups/{}", col2.id, parent.id))
        .join("group.yml")
        .is_file());
    // 跨集合搬完，分组里的请求还能按 id 找到（索引已修）
    let ws = store.load().unwrap();
    assert_eq!(ws.collections.len(), 2);
    assert!(store.find_request_file(&deep.id).is_ok());
    let found = store.find_group_dir(&child.id).unwrap();
    assert!(found.join("requests").join(format!("{}.yml", deep.id)).is_file());
}
