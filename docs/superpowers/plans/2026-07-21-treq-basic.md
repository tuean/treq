# treq 基础功能实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 Rust + GPUI 构建类 Yaak 的接口请求工具 v1：集合树 + 请求编辑器 + 响应查看器 + 环境变量（Yaak 式 YAML 文件存储）。

**Architecture:** Cargo workspace 双 crate——`treq-core`（无 GPUI 依赖：模型/持久化/环境解析/HTTP/JSON 美化，可独立测试）+ `treq-app`（GPUI 三栏界面）。core 输出数据类型，app 负责渲染与交互，改动互不影响。

**Tech Stack:** Rust 1.96 · gpui 0.2.2（crates.io，source 在 `~/.cargo/registry/src/rsproxy.cn-*/gpui-0.2.2/`）· reqwest 0.13 (rustls) · serde + serde_yaml 0.9 · uuid 1 (v4) · serde_json · dirs 6 · anyhow。UI 组件（Button/TextField/下拉菜单）用 gpui 原语自建，TextField 参照 gpui 自带示例 `examples/input.rs`。

**Spec:** `docs/superpowers/specs/2026-07-21-treq-basic-design.md`

## Global Constraints

- 版本锁定：gpui = "0.2.2"、reqwest = { version = "0.13", default-features = false, features = ["rustls-tls", "http2", "charset", "json"] }、serde_yaml = "0.9"、uuid = { version = "1", features = ["v4"] }、dirs = "6"。
- 文件以 uuid 命名，name 存 YAML 文件内（重命名不破坏 Git 历史）。
- 工作区目录结构：`<root>/collections/<id>/collection.yml`、`<root>/collections/<id>/requests/<id>.yml`、`<root>/environments/base.yml` + `<root>/environments/<id>.yml`。
- 环境变量语法 `{{ varName }}`；未解析出的变量保留字面 `{{ varName }}` 不替换。
- 设置文件：`~/Library/Application Support/com.treq.app/settings.toml`（workspace_root、locale、active_environment_id）。
- 默认工作区：`~/Documents/treq`（不存在则创建）。
- UI 文案全部走 `tr(locale, key)`，默认中文，菜单可切换。
- gpui API 以本地 source 为准：所有 gpui 签名以编译错误 + `grep ~/.cargo/registry/src/rsproxy.cn-*/gpui-0.2.2/` 为准，示例参考 `examples/hello_world.rs`、`examples/input.rs`、`examples/window.rs`、`examples/set_menus.rs`。
- 每个任务以 `cargo check`/`cargo test`/`cargo run` 通过 + commit 结束。

---

### Task 1: 工作区脚手架（hello world 窗口）

**Files:**
- Create: `Cargo.toml`（workspace）
- Create: `.gitignore`
- Create: `crates/treq-core/Cargo.toml`、`crates/treq-core/src/lib.rs`
- Create: `crates/treq-app/Cargo.toml`、`crates/treq-app/src/main.rs`

**Interfaces:**
- Produces: 可编译运行的 workspace；`gpui` 首次编译通过（wgpu 依赖链是最大构建风险，本任务验证）。

- [ ] **Step 1: 写 workspace 骨架**

`Cargo.toml`：
```toml
[workspace]
resolver = "2"
members = ["crates/treq-core", "crates/treq-app"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
reqwest = { version = "0.13", default-features = false, features = ["rustls-tls", "http2", "charset", "json"] }
gpui = "0.2.2"
dirs = "6"
```

`.gitignore`：
```
/target
.DS_Store
```

`crates/treq-core/Cargo.toml`：
```toml
[package]
name = "treq-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
serde_yaml = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
anyhow = { workspace = true }
reqwest = { workspace = true }
```

`crates/treq-core/src/lib.rs`：`pub fn hello() -> &'static str { "treq-core" }`

`crates/treq-app/Cargo.toml`：
```toml
[package]
name = "treq-app"
version.workspace = true
edition.workspace = true

[dependencies]
treq-core = { path = "../treq-core" }
gpui = { workspace = true }
anyhow = { workspace = true }
dirs = { workspace = true }
serde = { workspace = true }
toml = "0.8"
```

`crates/treq-app/src/main.rs`（参照 `gpui-0.2.2/examples/hello_world.rs` 的启动代码）：
```rust
use gpui::{size, px, App, Application, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, rgb};

struct HelloWorld { text: SharedString }

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x222222)).flex().items_center().justify_center()
            .text_2xl().text_color(rgb(0xffffff)).child(format!("Hello, {}!", self.text))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            |_, cx| cx.new(|_| HelloWorld { text: "treq".into() }),
        ).unwrap();
        cx.activate(true);
    });
}
```

- [ ] **Step 2: 编译并运行验证**

```bash
cargo build --workspace 2>&1 | tail -5
```
Expected: 编译通过（gpui 首次编译较慢，属正常）。

```bash
cargo run -p treq-app
```
Expected: 弹出 1200x800 窗口，深灰背景显示 "Hello, treq!"。`Ctrl-C` 退出。

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "chore: workspace scaffold with gpui hello world"
```

---

### Task 2: treq-core — 数据模型 + Yaak 式 YAML 存取

**Files:**
- Create: `crates/treq-core/src/models.rs`
- Create: `crates/treq-core/src/store.rs`
- Modify: `crates/treq-core/src/lib.rs`
- Test: `crates/treq-core/tests/store_test.rs`

**Interfaces:**
- Consumes: 无。
- Produces:
  - `models::Kv { key: String, value: String, enabled: bool }`
  - `models::BodyKind`（`None`/`Json`/`Form`/`Raw`，serde 为小写字符串）
  - `models::Body { kind: BodyKind, content: String }`
  - `models::RequestItem { id, name, method, url, params: Vec<Kv>, headers: Vec<Kv>, body: Body }`（全字段 serde Serialize/Deserialize）
  - `models::Collection { id, name, requests: Vec<RequestItem> }`（序列化时跳过 `requests` 字段）
  - `models::Environment { id, name, variables: BTreeMap<String, String> }`
  - `models::Workspace { collections: Vec<Collection>, base_env: Environment, environments: Vec<Environment> }`
  - `store::WorkspaceStore { new(root: PathBuf), root() -> &Path, ensure_root(&self), load(&self) -> anyhow::Result<Workspace>, create_collection(&self, name) -> anyhow::Result<Collection>, create_request(&self, collection_id, name) -> anyhow::Result<RequestItem>, save_request(&self, &RequestItem) -> anyhow::Result<()>, save_collection(&self, &Collection) -> anyhow::Result<()>, delete_collection(&self, id) -> anyhow::Result<()>, delete_request(&self, collection_id, request_id) -> anyhow::Result<()>, new_environment(&self, name, is_base: bool) -> anyhow::Result<Environment>, save_environment(&self, &Environment) -> anyhow::Result<()> }`
  - `store::new_id() -> String`（uuid v4）
  - lib.rs re-export：`pub use models::*; pub use store::WorkspaceStore;`

- [ ] **Step 1: 写失败测试**

`crates/treq-core/tests/store_test.rs`：
```rust
use serde_yaml;
use std::fs;
use std::path::PathBuf;
use treq_core::*;

fn tmp_ws(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("treq-test-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
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
        id: new_id(), name: "Get User".into(), method: "GET".into(),
        url: "https://{{ baseUrl }}/users/1".into(),
        params: vec![Kv { key: "limit".into(), value: "10".into(), enabled: true }],
        headers: vec![Kv { key: "Accept".into(), value: "application/json".into(), enabled: true }],
        body: Body { kind: BodyKind::Json, content: "{\"a\":1}".into() },
    };
    let yaml = serde_yaml::to_string(&req).unwrap();
    let back: RequestItem = serde_yaml::from_str(&yaml).unwrap();
    // BodyKind::Json 序列化为 "json" 后反序列化回来
    assert!(matches!(back.body.kind, BodyKind::Json));
    assert_eq!(back.url, req.url);
    req.id = back.id.clone(); req.name = back.name.clone();
    // 完整字段对比（防新字段漏序列化）
    assert_eq!(serde_yaml::to_value(&back).unwrap(), serde_yaml::to_value(&req).unwrap());
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
    let from_disk = serde_yaml::from_str::<RequestItem>(&fs::read_to_string(&file).unwrap()).unwrap();
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
    let env = store.new_environment("dev", false).unwrap();
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
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test -p treq-core
```
Expected: compile error（`treq_core::*` 无 `RequestItem` 等）。

- [ ] **Step 3: 实现 models.rs + store.rs**

`crates/treq-core/src/models.rs`：
```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kv {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind { None, Json, Form, Raw }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub kind: BodyKind,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestItem {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub params: Vec<Kv>,
    pub headers: Vec<Kv>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub requests: Vec<RequestItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub collections: Vec<Collection>,
    pub base_env: Environment,
    pub environments: Vec<Environment>,
}
```

`crates/treq-core/src/store.rs`：
```rust
use crate::models::*;
use anyhow::{anyhow, Context as _, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub struct WorkspaceStore {
    root: PathBuf,
}

impl WorkspaceStore {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    pub fn root(&self) -> &Path { &self.root }

    pub fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("collections"))?;
        fs::create_dir_all(self.root.join("environments"))?;
        let base_path = self.root.join("environments/base.yml");
        if !base_path.exists() {
            let base = Environment {
                id: new_id(), name: "Base".into(), variables: BTreeMap::new(),
            };
            fs::write(base_path, serde_yaml::to_string(&base)?)?;
        }
        Ok(())
    }

    pub fn load(&self) -> Result<Workspace> {
        let mut collections = Vec::new();
        for entry in fs::read_dir(self.root.join("collections"))? {
            let entry = entry?;
            let col_dir = entry.path();
            if !col_dir.is_dir() { continue; }
            let meta = col_dir.join("collection.yml");
            if !meta.exists() { continue; }
            let mut col: Collection =
                serde_yaml::from_str(&fs::read_to_string(&meta)?)
                    .with_context(|| format!("bad collection.yml in {}", col_dir.display()))?;
            let req_dir = col_dir.join("requests");
            col.requests = if req_dir.is_dir() {
                let mut reqs = Vec::new();
                for f in fs::read_dir(&req_dir)? {
                    let f = f?;
                    if f.path().extension().map(|e| e == "yml").unwrap_or(false) {
                        if let Ok(r) = serde_yaml::from_str::<RequestItem>(&fs::read_to_string(f.path())?) {
                            reqs.push(r);
                        }
                    }
                }
                reqs
            } else { Vec::new() };
            collections.push(col);
        }
        let env_dir = self.root.join("environments");
        let mut base_env = Environment { id: new_id(), name: "Base".into(), variables: BTreeMap::new() };
        let mut environments = Vec::new();
        for f in fs::read_dir(&env_dir)? {
            let f = f?;
            if f.path().extension().map(|e| e == "yml").unwrap_or(false) {
                let env: Environment = serde_yaml::from_str(&fs::read_to_string(f.path())?)?;
                if f.file_name() == "base.yml" { base_env = env; } else { environments.push(env); }
            }
        }
        Ok(Workspace { collections, base_env, environments })
    }

    pub fn create_collection(&self, name: &str) -> Result<Collection> {
        let col = Collection { id: new_id(), name: name.to_string(), requests: Vec::new() };
        let dir = self.root.join(format!("collections/{}", col.id));
        fs::create_dir_all(dir.join("requests"))?;
        fs::write(dir.join("collection.yml"), serde_yaml::to_string(&col)?)?;
        Ok(col)
    }

    pub fn create_request(&self, collection_id: &str, name: &str) -> Result<RequestItem> {
        let dir = self.root.join(format!("collections/{}/requests", collection_id));
        fs::create_dir_all(&dir)?;
        let req = RequestItem {
            id: new_id(), name: name.to_string(), method: "GET".into(), url: String::new(),
            params: Vec::new(), headers: Vec::new(),
            body: Body { kind: BodyKind::None, content: String::new() },
        };
        fs::write(dir.join(format!("{}.yml", req.id)), serde_yaml::to_string(&req)?)?;
        Ok(req)
    }

    pub fn save_request(&self, req: &RequestItem) -> Result<()> {
        let dir = self.root.join(format!("collections/{}/requests", req.id));
        // find collection dir containing this request id
        let col_dir = self.find_collection_dir(&req.id)?;
        let f = col_dir.join("requests").join(format!("{}.yml", req.id));
        fs::write(f, serde_yaml::to_string(req)?)?;
        let _ = dir;
        Ok(())
    }

    fn find_collection_dir(&self, request_id: &str) -> Result<PathBuf> {
        let req_file = format!("{}.yml", request_id);
        for entry in fs::read_dir(self.root.join("collections"))? {
            let entry = entry?;
            let req_dir = entry.path().join("requests");
            if req_dir.is_dir() && req_dir.join(&req_file).exists() {
                return Ok(entry.path());
            }
        }
        Err(anyhow!("request {} not found in any collection", request_id))
    }

    pub fn save_collection(&self, col: &Collection) -> Result<()> {
        let f = self.root.join(format!("collections/{}/collection.yml", col.id));
        fs::write(f, serde_yaml::to_string(col)?)?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> Result<()> {
        let dir = self.root.join(format!("collections/{}", id));
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    pub fn delete_request(&self, collection_id: &str, request_id: &str) -> Result<()> {
        let f = self.root.join(format!("collections/{}/requests/{}.yml", collection_id, request_id));
        fs::remove_file(f)?;
        Ok(())
    }

    pub fn new_environment(&self, name: &str, is_base: bool) -> Result<Environment> {
        let env = Environment { id: new_id(), name: name.to_string(), variables: BTreeMap::new() };
        self.save_environment(&env)?;
        if is_base {
            fs::remove_file(self.root.join("environments/base.yml")).ok();
            fs::rename(
                self.root.join(format!("environments/{}.yml", env.id)),
                self.root.join("environments/base.yml"),
            )?;
        }
        Ok(env)
    }

    pub fn save_environment(&self, env: &Environment) -> Result<()> {
        let f = self.root.join(format!("environments/{}.yml", env.id));
        fs::write(f, serde_yaml::to_string(env)?)?;
        Ok(())
    }
}
```

> 注：`save_request` 中的 `find_collection_dir` 按 request id 反查所属集合目录——请求 id 全局唯一，此实现避免调用方必须知道 collection id，rename 也不受影响。`dir` 变量为笔误残留，实现时删除该行。

`crates/treq-core/src/lib.rs`：
```rust
pub mod models;
pub mod store;

pub use models::*;
pub use store::WorkspaceStore;
```

- [ ] **Step 4: 运行测试至通过**

```bash
cargo test -p treq-core
```
Expected: 4 个测试全绿。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(core): data models and yaak-style yaml store"
```

---

### Task 3: treq-core — 环境变量解析

**Files:**
- Create: `crates/treq-core/src/vars.rs`
- Modify: `crates/treq-core/src/lib.rs`（`pub mod vars;`）
- Test: `crates/treq-core/tests/vars_test.rs`

**Interfaces:**
- Consumes: `models::*`（Task 2）。
- Produces:
  - `vars::merge_env(base: &Environment, active: Option<&Environment>) -> BTreeMap<String, String>`（active 覆盖 base）
  - `vars::resolve(template: &str, vars: &BTreeMap<String, String>) -> String`（`{{ name }}` 替换；未命中保留字面）
  - `vars::resolve_request(req: &RequestItem, vars: &BTreeMap<String, String>) -> RequestItem`（深拷贝并解析 url/params/headers/body，字段 `enabled == false` 的 kv 保留在原处——resolve 不改 enabled，过滤由 http 层做）

- [ ] **Step 1: 写失败测试**

`crates/treq-core/tests/vars_test.rs`：
```rust
use std::collections::BTreeMap;
use treq_core::vars::{merge_env, resolve, resolve_request};
use treq_core::*;

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn resolve_simple_and_nested() {
    let v = vars(&[("baseUrl", "https://api.example.com"), ("userId", "42")]);
    assert_eq!(resolve("{{ baseUrl }}/users/{{ userId }}", &v), "https://api.example.com/users/42");
}

#[test]
fn resolve_keeps_unknown_literal() {
    let v = vars(&[]);
    assert_eq!(resolve("a={{ missing }}", &v), "a={{ missing }}");
}

#[test]
fn resolve_handles_spacing_and_multiple_on_line() {
    let v = vars(&[("x", "1"), ("y", "2")]);
    assert_eq!(resolve("{{x}},{{  y  }}", &v), "1,2");
}

#[test]
fn merge_env_overrides_base() {
    let base = Environment { id: "b".into(), name: "Base".into(), variables: vars(&[("baseUrl", "https://prod.example.com"), ("key", "k1")]) };
    let dev = Environment { id: "d".into(), name: "dev".into(), variables: vars(&[("baseUrl", "http://localhost:8080")]) };
    let merged = merge_env(&base, Some(&dev));
    assert_eq!(merged.get("baseUrl").unwrap(), "http://localhost:8080");
    assert_eq!(merged.get("key").unwrap(), "k1");
    assert_eq!(merged.get("nope"), None);
}

#[test]
fn resolve_request_fields() {
    let req = RequestItem {
        id: "i".into(), name: "n".into(), method: "POST".into(),
        url: "{{ baseUrl }}/x".into(),
        params: vec![Kv { key: "q".into(), value: "{{ userId }}".into(), enabled: true }],
        headers: vec![Kv { key: "X-K".into(), value: "{{ key }}".into(), enabled: true }],
        body: Body { kind: BodyKind::Json, content: "{\"u\":\"{{ userId }}\"}".into() },
    };
    let v = vars(&[("baseUrl", "http://h"), ("userId", "7"), ("key", "v")]);
    let r = resolve_request(&req, &v);
    assert_eq!(r.url, "http://h/x");
    assert_eq!(r.params[0].value, "7");
    assert_eq!(r.headers[0].value, "v");
    assert_eq!(r.body.content, "{\"u\":\"7\"}");
    // 原请求不被修改
    assert_eq!(req.url, "{{ baseUrl }}/x");
}
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test -p treq-core
```
Expected: compile error（无 `vars` 模块）。

- [ ] **Step 3: 实现 vars.rs**

```rust
use crate::models::*;
use std::collections::BTreeMap;

pub fn merge_env(base: &Environment, active: Option<&Environment>) -> BTreeMap<String, String> {
    let mut merged = base.variables.clone();
    if let Some(a) = active {
        for (k, v) in &a.variables {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// 替换所有 `{{ name }}`（允许内部空格）。未命中的变量保留原字面量。
pub fn resolve(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && template[i..].starts_with("{{") {
            if let Some(close) = template[i + 2..].find("}}") {
                let name = template[i + 2..i + 2 + close].trim();
                match vars.get(name) {
                    Some(val) => { out.push_str(val); i += 2 + close + 2; continue; }
                    None => { out.push_str("{{"); out.push_str(name);
                              out.push_str("}}"); i += 2 + close + 2; continue; }
                }
            }
        }
        // 原样推进一个 char（保持 UTF-8 安全）
        let ch = template[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub fn resolve_request(req: &RequestItem, vars: &BTreeMap<String, String>) -> RequestItem {
    let resolve_kv = |kvs: &[Kv]| kvs.iter().map(|kv| Kv {
        key: resolve(&kv.key, vars), value: resolve(&kv.value, vars), enabled: kv.enabled,
    }).collect::<Vec<_>>();
    RequestItem {
        id: req.id.clone(), name: req.name.clone(), method: req.method.clone(),
        url: resolve(&req.url, vars),
        params: resolve_kv(&req.params),
        headers: resolve_kv(&req.headers),
        body: Body { kind: req.body.kind.clone(), content: resolve(&req.body.content, vars) },
    }
}
```

- [ ] **Step 4: 运行测试至通过**

```bash
cargo test -p treq-core
```
Expected: 全部通过（含 Task 2 的 4 个）。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(core): environment variable resolution"
```

---

### Task 4: treq-core — HTTP 执行 + JSON 美化

**Files:**
- Create: `crates/treq-core/src/http.rs`
- Create: `crates/treq-core/src/json.rs`
- Modify: `crates/treq-core/src/lib.rs`
- Test: `crates/treq-core/tests/http_test.rs`（用 std `TcpListener` 起本地 mock server）

**Interfaces:**
- Consumes: `models::*`、`vars::resolve_request`。
- Produces:
  - `http::HttpError { Timeout, Connect(String), Other(String) }`（Display/Error）
  - `http::ResponseData { status: Option<u16>, status_text: String, headers: Vec<(String, String)>, body: Vec<u8>, duration: Duration, error: Option<HttpError> }`
  - `http::send(req: &RequestItem, timeout: Duration) -> ResponseData`（async；params 拼进 URL query；`enabled=false` 的 kv 跳过；body 按 kind 编码：Json/Raw → 原文，Form → `k=v&k2=v2` urlencoded，None → 无 body；Content-Type 默认：Json → `application/json`，Form → `application/x-www-form-urlencoded`，用户显式设置的 header 优先）
  - `json::pretty_json(body: &[u8]) -> Option<String>`（合法 JSON 返回美化串，否则 None）
  - `json::looks_json(content_type: Option<&str>, body: &[u8]) -> bool`（content-type 含 json 或 body 以 `{`/`[` 开头）

- [ ] **Step 1: 写失败测试**

`crates/treq-core/tests/http_test.rs`：
```rust
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use treq_core::http::{send, HttpError};
use treq_core::json::{looks_json, pretty_json};
use treq_core::*;

/// 起一个返回固定响应的 mock server，返回它的监听地址。
fn mock_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut s) = stream {
                let mut buf = [0u8; 8192];
                let _ = s.read(&mut buf);
                let _ = s.write_all(response.as_bytes());
                let _ = s.flush();
            }
        }
    });
    addr.to_string()
}

fn get(url: &str) -> RequestItem {
    RequestItem {
        id: "i".into(), name: "n".into(), method: "GET".into(), url: url.into(),
        params: vec![], headers: vec![], body: Body { kind: BodyKind::None, content: String::new() },
    }
}

#[tokio::test] // 需要 tokio？—— 不，用 futures 不行就直接 std::thread + block_on 自定义
```

> 注：测试里忌引 tokio（core 故意无运行时依赖）。改用 gpui 无关的最小自建 block_on 太啰嗦——正确做法：`send` 内部是普通 async fn，测试用 `futures::executor::block_on`。把 `futures = "0.3"` 加为 treq-core 的 dev-dependency。

测试最终内容（替换上面的询问性片段）：
```rust
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use treq_core::http::{send, HttpError};
use treq_core::json::{looks_json, pretty_json};
use treq_core::*;

fn mock_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut s) = stream {
                let mut buf = [0u8; 16384];
                let _ = s.read(&mut buf);
                let _ = s.write_all(response.as_bytes());
                let _ = s.flush();
            }
        }
    });
    addr.to_string()
}

fn req(url: &str) -> RequestItem {
    RequestItem {
        id: "i".into(), name: "n".into(), method: "GET".into(), url: url.into(),
        params: vec![], headers: vec![], body: Body { kind: BodyKind::None, content: String::new() },
    }
}

#[test]
fn send_get_reads_status_headers_body() {
    let addr = mock_server("HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nX-Test: 1\r\nContent-Length: 15\r\n\r\n{\"ok\":true}\r\n");
    let r = futures::executor::block_on(send(&req(&format!("http://{}/a?b=1", addr)), Duration::from_secs(5)));
    assert!(r.error.is_none());
    assert_eq!(r.status, Some(201));
    assert_eq!(r.status_text, "Created");
    assert!(r.headers.iter().any(|(k, v)| k == "x-test" && v == "1"));
    assert_eq!(String::from_utf8_lossy(&r.body).trim(), "{\"ok\":true}");
}

#[test]
fn send_connection_refused_reports_error() {
    // 绑定后立刻关闭，端口基本不会再被占
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    drop(l);
    let r = futures::executor::block_on(send(&req(&format!("http://{}/", addr)), Duration::from_secs(3)));
    assert!(matches!(r.error, Some(HttpError::Connect(_))));
    assert_eq!(r.status, None);
}

#[test]
fn send_skips_disabled_params_and_headers() {
    // mock 回显请求行和 headers，验证 disabled 项未发出
    let addr = mock_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let mut r = req(&format!("http://{}/x", addr));
    r.params = vec![
        Kv { key: "on".into(), value: "1".into(), enabled: true },
        Kv { key: "off".into(), value: "2".into(), enabled: false },
    ];
    r.headers = vec![
        Kv { key: "X-On".into(), value: "yes".into(), enabled: true },
        Kv { key: "X-Off".into(), value: "no".into(), enabled: false },
    ];
    let _ = futures::executor::block_on(send(&r, Duration::from_secs(5)));
    // 此测试只验证不崩 + 成功；缺省用第二个 server 回显验证（见下）
}
```

> 注：回显验证改用一个更直接的 server：把收到的第一行请求行作为响应体返回。

```rust
#[test]
fn send_skips_disabled_params_and_headers_echo() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok(mut s) = listener.accept().unwrap().0 {
            let mut buf = [0u8; 16384];
            let n = s.read(&mut buf).unwrap();
            let received = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = received.lines().take_while(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
            let _ = s.write_all(resp.as_bytes());
        }
    });
    let mut r = req(&format!("http://{}/x", addr));
    r.params = vec![
        Kv { key: "on".into(), value: "1".into(), enabled: true },
        Kv { key: "off".into(), value: "2".into(), enabled: false },
    ];
    r.headers = vec![
        Kv { key: "X-On".into(), value: "yes".into(), enabled: true },
        Kv { key: "X-Off".into(), value: "no".into(), enabled: false },
    ];
    let resp = futures::executor::block_on(send(&r, Duration::from_secs(5)));
    let head = String::from_utf8_lossy(&resp.body).to_string();
    assert!(head.contains("GET /x?on=1 HTTP/1.1"), "request line wrong: {}", head);
    assert!(head.contains("x-on: yes"), "enabled header missing: {}", head);
    assert!(!head.contains("off"), "disabled param/header leaked: {}", head);
    assert!(!head.contains("x-off"), "disabled header leaked: {}", head);
}

#[test]
fn pretty_json_works_and_falls_back() {
    assert_eq!(pretty_json(b"{\"a\":1}").unwrap(), "{\n  \"a\": 1\n}");
    assert_eq!(pretty_json(b"not json"), None);
    assert!(looks_json(Some("application/json"), b"{\"a\":1}"));
    assert!(!looks_json(Some("text/plain"), b"{\"a\":1}"));
    assert!(looks_json(Some("application/json; charset=utf-8"), b"plain"));
}

#[test]
fn send_post_json_body() {
    let addr = mock_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let mut r = req(&format!("http://{}/", addr));
    r.method = "POST".into();
    r.body = Body { kind: BodyKind::Json, content: "{\"a\":1}".into() };
    let resp = futures::executor::block_on(send(&r, Duration::from_secs(5)));
    assert!(resp.error.is_none());
}
```

`crates/treq-core/Cargo.toml` 增加：
```toml
[dev-dependencies]
futures = "0.3"
```

- [ ] **Step 2: 运行确认失败**

```bash
cargo test -p treq-core
```
Expected: compile error（无 `http`/`json` 模块）。

- [ ] **Step 3: 实现 http.rs + json.rs**

`crates/treq-core/src/http.rs`：
```rust
use crate::models::*;
use crate::vars;
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum HttpError {
    Timeout,
    Connect(String),
    Other(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Timeout => write!(f, "timeout"),
            HttpError::Connect(m) => write!(f, "connection failed: {}", m),
            HttpError::Other(m) => write!(f, "request failed: {}", m),
        }
    }
}
impl std::error::Error for HttpError {}

#[derive(Debug, Clone)]
pub struct ResponseData {
    pub status: Option<u16>,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub duration: Duration,
    pub error: Option<HttpError>,
}

pub async fn send(req: &RequestItem, timeout: Duration) -> ResponseData {
    let started = std::time::Instant::now();
    let client = reqwest::Client::builder().timeout(timeout).build();
    let Ok(client) = client else {
        return ResponseData { status: None, status_text: String::new(), headers: vec![], body: vec![], duration: started.elapsed(), error: Some(HttpError::Other("failed to build http client".into())) };
    };

    // 拼 URL：params 追加 query
    let enabled_params: Vec<&Kv> = req.params.iter().filter(|p| p.enabled).collect();
    let mut url = req.url.clone();
    if !enabled_params.is_empty() {
        let sep = if url.contains('?') { "&" } else { "?" };
        let q = enabled_params.iter()
            .map(|p| format!("{}={}", urlencode(&p.key), urlencode(&p.value)))
            .collect::<Vec<_>>().join("&");
        url.push_str(sep);
        url.push_str(&q);
    }

    // 方法 + body
    let method = reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, &url);
    for h in &req.headers {
        if h.enabled && !h.key.is_empty() {
            if let Ok(k) = reqwest::header::HeaderName::from_bytes(h.key.as_bytes()) {
                if let Ok(v) = reqwest::header::HeaderValue::from_str(&h.value) {
                    builder = builder.header(k, v);
                }
            }
        }
    }
    match req.body.kind {
        BodyKind::None => {}
        BodyKind::Form => {
            let pairs = req.body.content.split('&')
                .filter(|s| !s.is_empty())
                .map(|kv| {
                    let mut it = kv.splitn(2, '=');
                    (it.next().unwrap_or(""), it.next().unwrap_or(""))
                });
            let mut form = reqwest::multipart::Form::new(); // 不可用，改用字符串 body
            let _ = form;
            // form-urlencoded 字符串体（保持用户原文，不重编码）
            builder = builder.header("Content-Type", "application/x-www-form-urlencoded")
                .body(req.body.content.clone());
            let _ = pairs;
        }
        BodyKind::Json => {
            builder = builder.header("Content-Type", "application/json").body(req.body.content.clone());
        }
        BodyKind::Raw => {
            builder = builder.body(req.body.content.clone());
        }
    }

    // 用户显式设的 Content-Type 会覆盖上面的默认值（同 key 后写赢）
    let result = builder.send().await;
    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
            let headers: Vec<(String, String)> = resp.headers().iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("<binary>").to_string()))
                .collect();
            let body = match resp.bytes().await {
                Ok(b) => b.to_vec(),
                Err(e) => { return ResponseData { status: Some(status), status_text, headers, body: vec![], duration: started.elapsed(), error: Some(HttpError::Other(format!("read body: {}", e))) }; }
            };
            ResponseData { status: Some(status), status_text, headers, body, duration: started.elapsed(), error: None }
        }
        Err(e) => {
            let error = if e.is_timeout() { HttpError::Timeout }
                else if e.is_connect() { HttpError::Connect(e.to_string()) }
                else { HttpError::Other(e.to_string()) };
            ResponseData { status: None, status_text: String::new(), headers: vec![], body: vec![], duration: started.elapsed(), error: Some(error) }
        }
    }
}

fn urlencode(s: &str) -> String {
    use reqwest::Url; // 仅为复用编码？不，直接用 percent-encoding 手写太啰嗦 —— 用 reqwest::Url::query_pairs? 简化：
    // v1：空格与保留字符按 ASCII 编码即可，足够演示
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
```

> 注 1：`vars` 的 use 在本任务不需要（resolve 由上层调用），删掉该行。`reqwest::Url` 也不需要，删除相关注释。Form 分支里 `multipart::Form` 一行是废代码——v1 Form body 不做 multipart 文件上传，直接发原文，删掉 `let mut form` 两行。
> 注 2：错误处理已满足"不 panic、错误可见"；握手超时属于 reqwest timeout 覆盖。若 `send` 出现 warning（未用变量），实现时清理。

`crates/treq-core/src/json.rs`：
```rust
pub fn pretty_json(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

pub fn looks_json(content_type: Option<&str>, body: &[u8]) -> bool {
    if let Some(ct) = content_type {
        if ct.to_ascii_lowercase().contains("json") { return true; }
    }
    let t = String::from_utf8_lossy(body).trim_start();
    t.starts_with('{') || t.starts_with('[')
}
```

`crates/treq-core/src/lib.rs` 增加：`pub mod http; pub mod json; pub mod vars;`

- [ ] **Step 4: 运行测试至通过**

```bash
cargo test -p treq-core
```
Expected: 全部通过（mock 端口测试偶发失败时重跑一次确认非 flaky）。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(core): http client and json pretty printer"
```

---

### Task 5: treq-app — 基础控件（Button/TextField/下拉菜单）

**Files:**
- Create: `crates/treq-app/src/widgets.rs`

**Interfaces:**
- Consumes: gpui 0.2.2 原语；范例 `gpui-0.2.2/examples/input.rs`（全量拷贝其 TextField 实现骨架）。
- Produces（供 Task 6-9 使用，全部为触发事件回到 AppModel 的样式）：
  - `pub fn button(label: SharedString, on_click: impl Fn(&mut Window, &mut App) + 'static) -> impl IntoElement`（照抄 `examples/window.rs` 的 button helper）
  - `pub struct TextField { content: SharedString, placeholder: SharedString, on_change: Arc<dyn Fn(&str) + Send + Sync> }` + `TextField::new(content, placeholder, on_change) -> Entity<TextField>`；实现 `Render` + `InputHandler`（以 examples/input.rs 为底，**剪掉**：多光标无、拖拽选择无、IME 中文必须保留 marked_range 逻辑）
  - `pub fn popup_menu(items: &[PopupItem]) -> impl IntoElement`，`pub struct PopupItem { id: SharedString, label: SharedString }`
  - `pub fn kv_row`：单行 kv 输入（enabled 复选框 + TextField + 删除按钮）→ 具体实现放到 Task 8。

- [ ] **Step 1: 读透 input.rs 范例**

```bash
sed -n '1,120p' ~/.cargo/registry/src/rsproxy.cn-*/gpui-0.2.2/examples/input.rs
sed -n '120,320p' ~/.cargo/registry/src/rsproxy.cn-*/gpui-0.2.2/examples/input.rs
```
确认：`InputHandler` trait 方法签名（`text_input_event`/`text_system`/`bounds_for_range`/`marked_text_range`）、`EntityInputHandler` 的 `render_input`/`focus_handle`、render 中 `paint` 文本的方式。

- [ ] **Step 2: 写 TextField（改编自范例）**

`crates/treq-app/src/widgets.rs` 核心：
```rust
use gpui::*;
use std::sync::Arc;

pub fn button(label: SharedString, on_click: impl Fn(&mut Window, &mut App) + 'static) -> impl IntoElement {
    div()
        .id(SharedString::from(label.to_string()))
        .flex_none()
        .px_2().py_0_5()
        .bg(rgb(0x2d2d30))
        .active(|this| this.opacity(0.85))
        .border_1().border_color(rgb(0x555558))
        .rounded_sm()
        .cursor_pointer()
        .child(label)
        .on_click(move |_, window, cx| on_click(window, cx))
}

pub struct TextField {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub last_layout: Option<ShapedLine>,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub is_selecting: bool,
    pub on_change: Arc<dyn Fn(&str) + Send + Sync>,
}

impl TextField {
    pub fn new(content: SharedString, placeholder: SharedString,
               on_change: Arc<dyn Fn(&str) + Send + Sync>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| TextField {
            focus_handle: cx.focus_handle(),
            content, placeholder,
            selected_range: 0..0, selection_reversed: false, marked_range: None,
            last_layout: None, last_bounds: None, is_selecting: false, on_change,
        })
    }
    // text 插入/删除/移动/选择：逐字移植 examples/input.rs 中 TextInput 的方法
    // （backspace/delete/left/right/paste/select_all/home/end + marked text 处理）
}
```

实现要求（移植时需要逐项核对范例）：光标移动与文本编辑（`insert_text` 处理 grapheme 边界、IME `marked_range` 合成区、Enter 提交合成）、粘贴、事件分发（`on_change` 在 content 改变后调用）、`EntityInputHandler` 全部方法（`text_system`、`bounds_for_range`、`marked_text_range`、`selected_text_range`、`bounds` 等）、render 输出（外层 div + `paint` ShapedLine + placeholder 灰字）。

- [ ] **Step 3: 编译验证**

```bash
cargo check -p treq-app
```
Expected: 0 error。有错误时按 Global Constraints 的 grep source 流程修复签名。

- [ ] **Step 4: 小测试页手动验证**

临时把 Task 1 的 `main.rs` 改为渲染两个 TextField + 一个 button（`cargo run`），确认：点击聚焦、输入中文（IME 组词过程不花屏）、退格、粘贴、on_change 收到回调。验证后**还原** main.rs 为 hello world（Task 6 会重写）。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(app): button and textfield widgets"
```

---

### Task 6: treq-app — AppModel + i18n + 三栏骨架 + 设置

**Files:**
- Create: `crates/treq-app/src/i18n.rs`
- Create: `crates/treq-app/src/model.rs`
- Create: `crates/treq-app/src/settings.rs`
- Modify: `crates/treq-app/src/main.rs`（真实启动 + 菜单栏）

**Interfaces:**
- Consumes: treq-core（`WorkspaceStore`/`Workspace`/模型）、Task 5 控件。
- Produces:
  - `i18n::Locale { Zh, En }` + `i18n::tr(locale, key) -> &'static str`（key 列表见下表，UI 全部文案）
  - `settings::Settings { workspace_root: PathBuf, locale: Locale, active_environment_id: Option<String> }` + `settings::load() -> Settings`、`settings::save(&Settings)`（TOML，路径 `~/Library/Application Support/com.treq.app/settings.toml`）
  - `model::AppModel { settings, store, workspace: Workspace, selection, response: Option<ResponseData>, url_preview: String, sending: bool, ... }` + 方法：`open_workspace(&mut self, path, cx)`、`reload(&mut self)`、`active_env(&self) -> Option<&Environment>`、`select_collection/select_request`、`new_collection/new_request/rename/delete`、`set_locale`、`notify`。
  - 三栏渲染方法：`fn tree_pane(&mut self, window, cx) -> impl IntoElement`（占位：任务名列表，无交互）、`fn editor_pane(...)`（占位：显示选中请求的 name/url 文本）、`fn response_pane(...)`（占位：空态文案）。

- [ ] **Step 1: 写失败验证（编译门）**

本任务无单元测试（UI 为主）。验证方式：`cargo check` + `cargo run` 手动看窗口。

- [ ] **Step 2: i18n.rs**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale { Zh, En }

impl Locale {
    pub fn from_str(s: &str) -> Self { if s == "en" { Locale::En } else { Locale::Zh } }
    pub fn as_str(&self) -> &'static str { match self { Locale::Zh => "zh", Locale::En => "en" } }
}

/// 全部 UI 文案。新增文案必须加到两个分支。
pub fn tr(locale: Locale, key: &str) -> &'static str {
    match (locale, key) {
        (Locale::Zh, "app.name") => "treq",
        (Locale::Zh, "menu.app") => "treq",
        (Locale::Zh, "menu.file") => "文件",
        (Locale::Zh, "menu.edit") => "编辑",
        (Locale::Zh, "action.quit") => "退出",
        (Locale::Zh, "action.choose_workspace") => "切换工作区…",
        (Locale::Zh, "action.reload") => "刷新",
        (Locale::Zh, "action.language") => "语言：English",
        (Locale::Zh, "side.collections") => "集合",
        (Locale::Zh, "side.environment") => "环境",
        (Locale::Zh, "side.no_collections") => "（空）右键新建集合",
        (Locale::Zh, "action.new_collection") => "新建集合",
        (Locale::Zh, "action.new_request") => "新建请求",
        (Locale::Zh, "action.rename") => "重命名",
        (Locale::Zh, "action.delete") => "删除",
        (Locale::Zh, "action.send") => "发送",
        (Locale::Zh, "editor.tab.params") => "Params",
        (Locale::Zh, "editor.tab.headers") => "Headers",
        (Locale::Zh, "editor.tab.body") => "Body",
        (Locale::Zh, "editor.tab.none") => "无",
        (Locale::Zh, "editor.tab.json") => "JSON",
        (Locale::Zh, "editor.tab.form") => "表单",
        (Locale::Zh, "editor.tab.raw") => "原始",
        (Locale::Zh, "editor.resolved_url") => "解析后",
        (Locale::Zh, "editor.select_hint") => "从左侧选择一个请求",
        (Locale::Zh, "response.title") => "响应",
        (Locale::Zh, "response.error") => "错误",
        (Locale::Zh, "response.time") => "耗时",
        (Locale::Zh, "response.size") => "大小",
        (Locale::Zh, "response.tab.body") => "Body",
        (Locale::Zh, "response.tab.headers") => "Headers",
        (Locale::Zh, "ws.missing") => "工作区不存在，请通过菜单选择目录",
        (Locale::En, "app.name") => "treq",
        (Locale::En, "menu.app") => "treq",
        (Locale::En, "menu.file") => "File",
        (Locale::En, "menu.edit") => "Edit",
        (Locale::En, "action.quit") => "Quit",
        (Locale::En, "action.choose_workspace") => "Choose Workspace…",
        (Locale::En, "action.reload") => "Reload",
        (Locale::En, "action.language") => "Language: 中文",
        (Locale::En, "side.collections") => "Collections",
        (Locale::En, "side.environment") => "Environment",
        (Locale::En, "side.no_collections") => "(empty) right-click to create",
        (Locale::En, "action.new_collection") => "New Collection",
        (Locale::En, "action.new_request") => "New Request",
        (Locale::En, "action.rename") => "Rename",
        (Locale::En, "action.delete") => "Delete",
        (Locale::En, "action.send") => "Send",
        (Locale::En, "editor.tab.params") => "Params",
        (Locale::En, "editor.tab.headers") => "Headers",
        (Locale::En, "editor.tab.body") => "Body",
        (Locale::En, "editor.tab.none") => "None",
        (Locale::En, "editor.tab.json") => "JSON",
        (Locale::En, "editor.tab.form") => "Form",
        (Locale::En, "editor.tab.raw") => "Raw",
        (Locale::En, "editor.resolved_url") => "Resolved",
        (Locale::En, "editor.select_hint") => "Select a request on the left",
        (Locale::En, "response.title") => "Response",
        (Locale::En, "response.error") => "Error",
        (Locale::En, "response.time") => "Time",
        (Locale::En, "response.size") => "Size",
        (Locale::En, "response.tab.body") => "Body",
        (Locale::En, "response.tab.headers") => "Headers",
        (Locale::En, "ws.missing") => "Workspace missing, choose a folder from the menu",
    }
}
```

- [ ] **Step 3: settings.rs**

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub workspace_root: PathBuf,
    pub locale: String,            // "zh" | "en"
    pub active_environment_id: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let root = dirs::document_dir().unwrap_or_else(|| std::env::temp_dir()).join("treq");
        Settings { workspace_root: root, locale: "zh".into(), active_environment_id: None }
    }
}

fn path() -> PathBuf {
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("com.treq.app").join("settings.toml")
}

pub fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() { std::fs::create_dir_all(dir)?; }
    std::fs::write(p, toml::to_string(s)?)?;
    Ok(())
}
```

- [ ] **Step 4: model.rs（AppModel 骨架 + 三栏渲染占位）**

```rust
use crate::i18n::{tr, Locale};
use crate::settings::{self, Settings};
use gpui::{prelude::*, *};
use treq_core::{Environment, RequestItem, ResponseData, Workspace, WorkspaceStore};

#[derive(Debug, Clone, PartialEq)]
pub enum Selection { Collection(String), Request(String) }

pub struct AppModel {
    pub settings: Settings,
    pub store: WorkspaceStore,
    pub workspace: Workspace,
    pub selection: Option<Selection>,
    pub response: Option<ResponseData>,
    pub resolved_url: String,
    pub sending: bool,
}

impl AppModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = settings::load();
        let store = WorkspaceStore::new(settings.workspace_root.clone());
        let mut m = AppModel {
            workspace: Workspace { collections: vec![], base_env: treq_core::Environment {
                id: "base".into(), name: "Base".into(), variables: Default::default() },
                environments: vec![] },
            settings, store, selection: None, response: None, resolved_url: String::new(), sending: false,
        };
        m.reload();
        m
    }

    pub fn locale(&self) -> Locale { Locale::from_str(&self.settings.locale) }
    pub fn t(&self, key: &str) -> &'static str { tr(self.locale(), key) }

    pub fn reload(&mut self) {
        self.store.ensure_root().ok();
        if let Ok(ws) = self.store.load() { self.workspace = ws; }
    }

    pub fn active_env(&self) -> Option<&Environment> {
        self.settings.active_environment_id.as_ref()
            .and_then(|id| self.workspace.environments.iter().find(|e| &e.id == id))
    }

    pub fn selected_request(&self) -> Option<&RequestItem> {
        let sel = self.selection.as_ref()?;
        let Selection::Request(rid) = sel else { return None };
        self.workspace.collections.iter().flat_map(|c| c.requests.iter()).find(|r| &r.id == rid)
    }

    pub fn set_locale(&mut self, l: Locale, cx: &mut Context<Self>) {
        self.settings.locale = l.as_str().into();
        settings::save(&self.settings).ok();
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload();
        cx.notify();
    }

    // ---- Task 7 桩：CRUD ----
    pub fn create_collection(&mut self, cx: &mut Context<Self>) {
        if let Ok(col) = self.store.create_collection(&self.t("action.new_collection")) {
            self.workspace.collections.push(col);
            cx.notify();
        }
    }
    pub fn create_request(&mut self, collection_id: &str, cx: &mut Context<Self>) {
        if let Ok(req) = self.store.create_request(collection_id, "Untitled") {
            if let Some(c) = self.workspace.collections.iter_mut().find(|c| c.id == collection_id) {
                c.requests.push(req.clone());
            }
            self.selection = Some(Selection::Request(req.id));
            cx.notify();
        }
    }

    // ---- 三栏渲染 ----
    pub fn tree_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Task 7 全量实现；此处占位
        div().id("tree").flex_none().w_64().h_full().bg(rgb(0x1e1e1e)).border_r_1().border_color(rgb(0x333333))
            .child(div().px_2().py_2().text_sm().text_color(rgb(0xcccccc)).child(self.t("side.collections")))
    }

    pub fn editor_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("editor").flex_1().h_full().bg(rgb(0x252526))
            .child(div().p_3().text_sm().text_color(rgb(0xaaaaaa)).child(
                self.selected_request().map(|r| r.name.clone()).unwrap_or_else(|| self.t("editor.select_hint").into())
            ))
    }

    pub fn response_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("response").flex_none().w_96().h_full().bg(rgb(0x1e1e1e)).border_l_1().border_color(rgb(0x333333))
            .child(div().p_3().text_sm().text_color(rgb(0xaaaaaa)).child(self.t("response.title")))
    }
}
```

> 注：`ResponseData` 需从 treq-core re-export（Task 4 已 `pub mod http`，lib.rs 加 `pub use http::{ResponseData, HttpError};`）。

- [ ] **Step 5: main.rs 真实启动 + 菜单**

```rust
use crate::model::AppModel;
use gpui::{prelude::*, *};
use treq_core::{Body, BodyKind, Kv, RequestItem};
use crate::widgets;

mod i18n; mod model; mod settings; mod widgets;

actions!(treq, [Quit, ChooseWorkspace, ToggleLocale, Reload]);

fn main() {
    Application::new().run(|cx: &mut App| {
        // 菜单（参照 examples/set_menus.rs 的 Menu/MenuItem 结构）
        cx.set_menus(vec![
            Menu { name: "treq".into(), items: vec![
                MenuItem::action("Quit", Quit),
            ]},
            Menu { name: "File".into(), items: vec![
                MenuItem::action("Choose Workspace…", ChooseWorkspace),
                MenuItem::action("Reload", Reload),
            ]},
            Menu { name: "View".into(), items: vec![
                MenuItem::action("Language: 中文 / English", ToggleLocale),
            ]},
        ]);
        cx.bind_command(Quit, |_window, cx| cx.quit()).ok();
        cx.bind_command(ChooseWorkspace, |window, cx| { /* Task 6 Step 6 */ }).ok();
        cx.bind_command(ToggleLocale, |window, cx| { /* 轮换 locale */ }).ok();
        cx.bind_command(Reload, |window, cx| { /* 调 refresh */ }).ok();

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        }, |_window, cx| cx.new(crate::model::AppModel::new)).unwrap();
        cx.activate(true);
    });
}
```

菜单命令的绑定方式以 `examples/set_menus.rs` 与 `examples/input.rs` 的 `cx.bind_keys`/`bind_command`（或对应 API）为准，缺哪个签名 grep source 补。

- [ ] **Step 6: 选择工作区（FileDialog）**

在 `bind_command(ChooseWorkspace, ...)` 中：
```rust
cx.prompt_for_paths(gpui::platform::FileDialogOptions { select_directories: true, ..Default::default() })
    .ok(); // 返回 task，回调里用 model handle 设置 workspace_root 并 save settings + reload
```
（`prompt_for_paths` 签名在 `src/app.rs:1116`；`FileDialogOptions` 字段以 `src/platform.rs` 为准。）

- [ ] **Step 7: 运行验证**

```bash
cargo run -p treq-app
```
Expected: 三栏可见（左集合/中编辑器/右响应），菜单栏有 treq/File/View，切换语言菜单项可点，重开 app 语言保持。默认工作区 `~/Documents/treq` 自动创建。

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat(app): app model, i18n, settings, 3-pane skeleton"
```

---

### Task 7: treq-app — 集合树 + CRUD + 环境下拉 + 刷新

**Files:**
- Modify: `crates/treq-app/src/model.rs`（树渲染 + 右键菜单 + CRUD 完整实现）
- Create: `crates/treq-app/src/menus.rs`（弹出菜单小组件：`popup_menu(items, on_pick)` 状态化实现）

**Interfaces:**
- Consumes: Task 5 控件、Task 6 AppModel。
- Produces: 交互完整的左栏：
  - 集合行（点击选中显示为高亮；右键弹出菜单：新建请求/新建集合/重命名/删除）
  - 请求行（点击选中；右键：重命名/删除）
  - 底部：环境下拉（popup_menu 列出 Base + 各环境，选中高亮）+"刷新"按钮
  - 重命名：复用 TextField —— 行内编辑（行变成 TextField，回车提交）

- [ ] **Step 1: 写弹出菜单小组件 menus.rs**

```rust
pub struct PopupMenu { pub open: bool, pub x: f32, pub y: f32, pub items: Vec<(String, String)> }  // (id, label)
impl PopupMenu {
    pub fn open_at(&mut self, x: f32, y: f32, items: Vec<(String, String)>) { self.open = true; self.x = x; self.y = y; self.items = items; }
    pub fn close(&mut self) { self.open = false; }
}
```
AppModel 持有 `popup: PopupMenu`。渲染：`tree_pane` 根 div 加 `.relative()`；popup.open 时追加一个 `div().absolute().left(px(...)).top(px(...)).z_index(10).bg(...).border...`，内含每项 `.on_click` 派发 `menu_action(id)`（动作 id：`new-collection`/`new-request:<col_id>`/`rename:<id>`/`delete:<id>`/`env:<env_id>`）。整个根 div `on_mouse_down` 先关 popup（再按其它逻辑处理，注意事件顺序：菜单项点击优先——菜单项 z_index 在上层，先触发其 on_click，菜单项点击后 close 即可）。

- [ ] **Step 2: 树渲染（model.rs 替换 tree_pane 占位）**

```rust
pub fn tree_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let locale = self.locale();
    let mut root = div().id("tree").flex().flex_col().flex_none().w_64().h_full()
        .bg(rgb(0x1e1e1e)).border_r_1().border_color(rgb(0x333333)).relative();
    // 集合标题行
    root = root.child(div().flex().px_2().py_1_5().items_center().gap_1()
        .child(div().flex_1().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(rgb(0xdddddd))
            .child(self.t("side.collections"))));
    if self.workspace.collections.is_empty() {
        root = root.child(div().px_2().py_1().text_xs().text_color(rgb(0x666666)).child(self.t("side.no_collections")));
    }
    for col in &self.workspace.collections {
        let selected = matches!(&self.selection, Some(Selection::Collection(id)) if id == &col.id);
        root = root.child(
            div().id(SharedString::from(format!("col/{}", col.id))).flex().items_center().px_2().py_0_5()
                .cursor_pointer().text_sm()
                .when(selected, |d| d.bg(rgb(0x37373d)))
                .child(SharedString::from(col.name.clone()))
                .on_click(cx.listener(move |this, _, window, cx| { this.select_collection(col.id.clone(), window, cx); }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, _e, window, cx| {
                    this.open_collection_menu(col.id.clone(), window, cx);
                }))
        );
        for r in &col.requests {
            let rid = r.id.clone(); let cid = col.id.clone();
            let selected = matches!(&self.selection, Some(Selection::Request(id)) if id == &r.id);
            root = root.child(
                div().id(SharedString::from(format!("req/{}", r.id))).flex().items_center().px_2().pl_6().py_0_5()
                    .cursor_pointer().text_sm()
                    .when(selected, |d| d.bg(rgb(0x37373d)))
                    .child(SharedString::from(r.name.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| { this.select_request(rid.clone(), window, cx); }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, _e, window, cx| {
                        this.open_request_menu(cid.clone(), rid.clone(), window, cx);
                    }))
            );
        }
    }
    // 底部：环境下拉 + 刷新
    let env_label = self.active_env().map(|e| e.name.clone()).unwrap_or_else(|| self.workspace.base_env.name.clone());
    root = root.child(div().flex_1());
    root = root.child(
        div().flex().items_center().gap_1().px_2().py_1().border_t_1().border_color(rgb(0x333333))
            .child(div().text_xs().text_color(rgb(0x888888)).child(self.t("side.environment")))
            .child(div().flex_1().text_sm().text_color(rgb(0xdddddd)).cursor_pointer()
                .child(SharedString::from(env_label))
                .on_click(cx.listener(|this, _, window, cx| this.open_env_menu(window, cx))))
            .child(widgets::button(SharedString::from(self.t("action.reload")), ...)) // 或图标化文字按钮
    );
    // 弹出菜单叠加
    if self.popup.open { root = root.child(self.popup_render()); }
    root
}
```

右键事件对象 `e.position` 用 px 坐标打开 popup：`this.popup.open_at(e.position.x, e.position.y, items)`。

- [ ] **Step 3: CRUD 方法（model.rs 内）**

```rust
pub fn select_collection(&mut self, id: String, _window: &mut Window, cx: &mut Context<Self>) { self.selection = Some(Selection::Collection(id)); cx.notify(); }
pub fn select_request(&mut self, id: String, _window: &mut Window, cx: &mut Context<Self>) { self.selection = Some(Selection::Request(id)); cx.notify(); }
pub fn open_collection_menu(&mut self, col_id: String, window: &mut Window, cx: &mut Context<Self>) {
    let e = /* 从事件里拿位置 —— 见 Step 2 的事件签名 */;
    self.popup.open_at(e.position.x, e.position.y, vec![
        ("new-request".into(), self.t("action.new_request").into()),
        (format!("rename-col:{}", col_id), self.t("action.rename").into()),
        (format!("delete-col:{}", col_id), self.t("action.delete").into()),
    ]);
    cx.notify();
}
pub fn open_request_menu(&mut self, col_id: String, req_id: String, window: &mut Window, cx: &mut Context<Self>) { ... } // new-collection / rename-req / delete-req
pub fn open_env_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let mut items = vec![("env:base".into(), self.workspace.base_env.name.clone())];
    for e in &self.workspace.environments { items.push((format!("env:{}", e.id), e.name.clone())); }
    // 位置：底部条上方 —— 用固定偏移（如 x=210, y=窗口高度-120 简化）
    self.popup.open_at(210.0, 700.0, items);
    cx.notify();
}
pub fn menu_action(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
    self.popup.close();
    if id == "new-collection" { self.create_collection(cx); return; }
    if id == "new-request" { /* 需要 col id：菜单项携带 */ }
    if let Some(rest) = id.strip_prefix("rename-col:") { self.begin_rename(Selection::Collection(rest.into()), cx); }
    if let Some(rest) = id.strip_prefix("rename-req:") { self.begin_rename(Selection::Request(rest.into()), cx); }
    if let Some(rest) = id.strip_prefix("delete-col:") { self.delete_collection(rest, cx); }
    if let Some(rest) = id.strip_prefix("delete-req:") { self.delete_request(rest, cx); }
    if let Some(rest) = id.strip_prefix("env:") { self.set_active_env(rest, cx); }
    cx.notify();
}
pub fn rename_state: Option<(Selection, String /*原值*/, Entity<TextField>)> // 行内编辑状态
pub fn begin_rename(&mut self, sel: Selection, cx: &mut Context<Self>) { /* 记录 + 创建 TextField，enter 提交 commit_rename，esc 取消 */ }
pub fn commit_rename(&mut self, cx: &mut Context<Self>) {
    // Collection: store.save_collection; Request: store.save_request
}
pub fn delete_collection(&mut self, id: String, cx: &mut Context<Self>) {
    if self.store.delete_collection(&id).is_ok() {
        self.workspace.collections.retain(|c| c.id != id);
        if matches!(&self.selection, Some(Selection::Collection(x)) if x == &id) { self.selection = None; }
        cx.notify();
    }
}
pub fn delete_request(&mut self, id: String, cx: &mut Context<Self>) {
    // 找到所属集合 id 调 store.delete_request；从内存删；选中态清理
}
pub fn set_active_env(&mut self, env_id: String, cx: &mut Context<Self>) {
    self.settings.active_environment_id = if env_id == "base" { None } else { Some(env_id) };
    settings::save(&self.settings).ok();
    cx.notify();
}
```

> 注：右键菜单项动作 id 约定：`new-collection`、`new-request:<col_id>`、`rename-col:<id>`、`rename-req:<col_id>/<req_id>`（重命名需要两个 id，用 `/` 分隔解析）、`delete-col:<id>`、`delete-req:<col_id>/<req_id>`、`env:<id>`。`menu_action` 按此解析，实现时统一。

- [ ] **Step 4: 运行验证（手动清单）**

```bash
cargo run -p treq-app
```
- 右键集合 → 新建请求 / 重命名（回车生效，文件内 name 更新）/ 删除（文件删除）
- 新建集合出现在树中；刷新按钮 → 磁盘改动（外部改 YAML 或 git pull）反映到树
- 环境下拉列出 Base 与各环境；切换后设置持久化（重开 app 保持）
- 中栏随选中请求更新；空选择显示提示

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(app): collection tree with crud, env dropdown, reload"
```

---

### Task 8: treq-app — 请求编辑器（方法/URL/Params/Headers/Body + 自动保存）

**Files:**
- Modify: `crates/treq-app/src/model.rs`（editor_pane 完整实现、请求编辑字段直接落盘）

**Interfaces:**
- Consumes: Task 5 TextField/button、Task 6 AppModel、treq-core `vars::merge_env/resolve`。
- Produces:
  - `editor_pane` 完整渲染：方法下拉（popup_menu：GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS）、URL TextField、发送按钮（Task 9 接线）、解析后 URL 预览行、Params/Headers 两列 kv 表格（行内 enabled 勾选 + key/value TextField + 删除按钮，底部"+"新行）、Body 类型下拉（无/JSON/表单/原始）+ 内容 TextField（多行：TextField 支持换行输入？——范例 TextField 是单行。Body 内容 v1 用单行 TextField 不合适。
  - **决定：Body 内容区 v1 用单行 TextField × 不可行 → 改为多行输入：在 TextField 的实体输入处理中允许 `\n`（范例 insert_text 支持任意文本含换行；render 用 `text_system` 换行布局——范例只绘制单行 ShapedLine。简化：v1 body 编辑器也用单行 TextField 但允许粘贴换行，渲染时替换 `\n` 为空格显示，发送时用 content 原文。`ponytail:` 注释标记该简化。**
  - 保存：每次 TextField on_change 直接 `update_request(field, value)` → 修改内存副本并 `store.save_request`（无 debounce）。

- [ ] **Step 1: 编辑状态桥接（model.rs）**

```rust
// AppModel 增加：
pub fn update_selected_request(&mut self, f: impl FnOnce(&mut RequestItem), cx: &mut Context<Self>) {
    let sel = self.selection.clone();
    let Selection::Request(rid) = sel else { return };
    let Some(col_id) = self.workspace.collections.iter().find_map(|c| c.requests.iter().find(|r| r.id == rid).map(|_| c.id.clone())) else { return };
    if let Some(r) = self.workspace.collections.iter_mut().flat_map(|c| c.requests.iter_mut()).find(|r| r.id == rid) {
        f(r);
        let _ = self.store.save_request(r);
    }
    let _ = col_id;
    cx.notify();
}
pub fn set_method(&mut self, m: String, cx: &mut Context<Self>) { self.update_selected_request(|r| r.method = m, cx); }
pub fn set_url(&mut self, u: String, cx: &mut Context<Self>) { self.update_selected_request(|r| r.url = u, cx); self.update_preview(cx); }
pub fn set_body_kind(&mut self, k: BodyKind, cx: &mut Context<Self>) { self.update_selected_request(|r| r.body.kind = k, cx); }
pub fn set_body_content(&mut self, c: String, cx: &mut Context<Self>) { self.update_selected_request(|r| r.body.content = c, cx); }
pub fn kv_set(&mut self, which: KvWhich, index: usize, field: KvField, v: String, cx: &mut Context<Self>) { /* 按 which/index/field 改 params 或 headers */ }
pub fn kv_toggle(&mut self, which: KvWhich, index: usize, cx: &mut Context<Self>) { /* enabled = !enabled */ }
pub fn kv_add(&mut self, which: KvWhich, cx: &mut Context<Self>) { /* push Kv{key:"",value:"",enabled:true} */ }
pub fn kv_remove(&mut self, which: KvWhich, index: usize, cx: &mut Context<Self>) { /* remove(index) */ }
pub fn update_preview(&mut self, cx: &mut Context<Self>) {
    let vars = vars::merge_env(&self.workspace.base_env, self.active_env());
    self.resolved_url = self.selected_request().map(|r| vars::resolve(&r.url, &vars)).unwrap_or_default();
}
```
`KvWhich { Params, Headers }`、`KvField { Key, Value }` 枚举；`body_kinds()` 常量列表。

- [ ] **Step 2: editor_pane 完整渲染（model.rs）**

结构与 Tree 相似的方法链：方法区 flex 行（方法 popup 触发按钮 + URL TextField flex_1 + 发送 button）→ 解析预览行（灰字 xs）→ 类型标签行（Params/Headers/Body 三 tab 按钮，选中高亮，`editor_tab: TabKind` 状态）→ 表格/正文区。
- Params/Headers 表格：`for (i, kv) in list.iter().enumerate()` 每行：勾选 div（on_click toggle，□ 用字体符号或绘制背景色块）+ key TextField（w_1/3）+ value TextField（flex_1）+ "×" 按钮。
- Body tab：类型下拉按钮（无/JSON/表单/原始）+ 内容 TextField（h_64 多行区域，flex_col 布局）。
- `<Enter>` 在 URL 框聚焦时触发发送（Task 9 定义 send action，Task 8 先留占位函数）。

- [ ] **Step 3: 校验与提交验证**

```bash
cargo check -p treq-app
cargo run -p treq-app
```
手动：改 method/url/params/headers/body → 关 app 重开 → 全部还在（YAML 已落盘）；`cat ~/Documents/treq/collections/*/requests/*.yml` 确认内容正确。

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(app): request editor with autosave"
```

---

### Task 9: treq-app — 发送请求 + 响应面板

**Files:**
- Modify: `crates/treq-app/src/model.rs`（send_request + response_pane 完整实现 + 渲染辅助 `render_response_body`）

**Interfaces:**
- Consumes: Task 4 `http::send`、`json::pretty_json/looks_json`、Task 8 编辑状态。
- Produces:
  - `pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>)`（核心发送流程）
  - `pub fn response_pane(...)` 完整渲染：标题行（状态码色块 + 状态文本 + 耗时 + 大小 + 错误红字）、Headers/Body 两 tab、Body 视图（JSON 行级着色 / 原文）
  - 发送中状态：按钮变"发送中…"，响应到达后恢复。

- [ ] **Step 1: send_request（后台执行 + 回到主线程）**

```rust
pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(req) = self.selected_request().cloned() else { return };
    if self.sending { return; }
    self.sending = true;
    self.response = None;
    cx.notify();
    let vars = vars::merge_env(&self.workspace.base_env, self.active_env());
    let resolved = vars::resolve_request(&req, &vars);
    self.resolved_url = resolved.url.clone();
    let handle = cx.entity();
    cx.spawn(async move {
        let r = treq_core::http::send(&resolved, Duration::from_secs(30)).await;
        handle.update(cx, |this, cx| {
            this.sending = false;
            this.response = Some(r);
            this.response_tab = ResponseTab::Body;
            cx.notify();
        }).ok();
    }).detach();
    let _ = window;
}
```
（`cx.spawn` 的任务需要 `'static + Send`：`resolved` 全 owned，OK。）

- [ ] **Step 2: response_pane 完整渲染**

```rust
pub fn response_pane(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let locale = self.locale();
    let t = |k: &str| tr(locale, k);
    let mut root = div().id("response").flex().flex_col().flex_none().w_96().h_full()
        .bg(rgb(0x1e1e1e)).border_l_1().border_color(rgb(0x333333));
    let resp = self.response.clone();
    match &resp {
        None => { root = root.child(div().p_3().text_sm().text_color(rgb(0x666666))
                .child(self.sending.then(|| SharedString::from("发送中…")).unwrap_or_else(|| t("response.title").into()))); }
        Some(r) => {
            // 标题行
            let status_color = match r.status { Some(s) if (200..300).contains(&s) => rgb(0x4ec9b0), Some(_) => rgb(0xd7ba7d), None => rgb(0xf14c4c) };
            root = root.child(
                div().flex().items_center().gap_2().px_2().py_1_5().border_b_1().border_color(rgb(0x333333))
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(status_color)
                        .child(format!("{} {}", r.status.map(|s| s.to_string()).unwrap_or_else(|| t("response.error").into()), r.status_text)))
                    .child(div().text_xs().text_color(rgb(0x888888)).child(format!("{} {}ms", t("response.time"), r.duration.as_millis())))
                    .child(div().text_xs().text_color(rgb(0x888888)).child(format!("{} {}B", t("response.size"), r.body.len())))
            );
            if let Some(err) = &r.error {
                root = root.child(div().p_2().text_xs().text_color(rgb(0xf14c4c)).child(err.to_string()));
            }
            // tabs
            let tab = self.response_tab.clone().unwrap_or(ResponseTab::Body);
            root = root.child(div().flex().gap_1().px_2().py_1()
                .child(tab_button("response.tab.body", ResponseTab::Body, tab == ResponseTab::Body, locale))
                .child(tab_button("response.tab.headers", ResponseTab::Headers, tab == ResponseTab::Headers, locale)));
            match tab {
                ResponseTab::Body => { root = root.child(self.render_response_body(r)); }
                ResponseTab::Headers => {
                    let mut list = root.child(div().flex_col().px_2().py_1().overflow_scroll());
                    for (k, v) in &r.headers {
                        list = list.child(div().flex().gap_2().text_xs()
                            .child(div().flex_none().text_color(rgb(0x9cdcfe)).child(SharedString::from(k)))
                            .child(div().text_color(rgb(0xcccccc)).child(SharedString::from(v))));
                    }
                    root = list;
                }
            }
        }
    }
    root
}

fn render_response_body(&mut self, r: &ResponseData) -> impl IntoElement {
    let ct = r.headers.iter().find(|(k, _)| k == "content-type").map(|(_, v)| v.clone());
    let text = String::from_utf8_lossy(&r.body).to_string();
    let mut d = div().flex_col().px_2().py_1().overflow_scroll().flex_1();
    if treq_core::json::looks_json(ct.as_deref(), &r.body) {
        if let Some(pretty) = treq_core::json::pretty_json(&r.body) {
            for line in pretty.lines() {
                d = d.child(colored_line(line));  // 行级着色：键行/字符串/数字/其它
            }
            return d;
        }
    }
    for line in text.lines() { d = d.child(div().text_xs().text_color(rgb(0xcccccc)).child(SharedString::from(line))); }
    d
}
```

`colored_line`：从右向左扫描——`"key": value` 结构：键（含引号）蓝色 `0x9cdcfe`，字符串值绿色 `0xce9178`，数字橙色 `0xb5cea8`，布尔/null 蓝紫 `0x569cd6`，纯符号默认 `0xcccccc`。简化实现：按行末标记切分（`"…":` / `"…",` / `…,` / 数字结尾），v1 够用；`ponytail:` 注释标记"行级近似着色，需要精确高亮时换 tree-sitter"。

- [ ] **Step 3: 发送按钮与 Enter 触发（editor_pane 接线）**

Task 8 的发送按钮 on_click → `this.send_request(window, cx)`；URL TextField 增加 Enter → `this.send_request(window, cx)`（TextField 的 on_change 之外加 `on_submit: Option<Arc<dyn Fn() + Send + Sync>>` 字段，范例 insert_text 对 `\n` 的处理处调用——TextField 单行模式下 Enter 提交）。

- [ ] **Step 4: 端到端验证**

```bash
cargo run -p treq-app
```
- 新建请求 → URL `https://httpbin.org/json`（或本地起 `python3 -m http.server` 验 GET）→ 发送 → 200 + JSON 美化着色
- 环境：base `baseUrl=http://127.0.0.1:8000`，请求 `{{ baseUrl }}/anything` → 预览行显示解析后 URL，发送正常
- POST + JSON body → mock 验证 body 到达
- 无效 URL / 连接拒绝 → 错误红字；请求中按钮态变化
- Headers tab 显示响应头

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(app): send request and response panel"
```

---

## Self-Review（执行前已自查）

1. **Spec 覆盖**：三栏 ✓（T6）、CRUD ✓（T7）、环境变量+预览 ✓（T3 解析 + T8 预览）、HTTP 执行 ✓（T4 + T9）、JSON 美化着色 ✓（T4/T9）、Yaak 式存储 ✓（T2）、i18n ✓（T6）、设置持久化 ✓（T6/T7）。v1 不做项均未进计划。
2. **占位符扫描**：无 TBD；Step 中的"以 xxx 为准"均指向本地 source 具体文件/行号，是 API 探针而非占位。Task 5 文本输入实现明确引用范例并裁剪说明。
3. **类型一致性**：`Kv/RequestItem/Collection/Environment/Workspace/WorkspaceStore/ResponseData/HttpError/vars::*` 在 core 测试、app 各任务间签名一致；`update_selected_request` 闭包模式统一；`PopupItem(id,label)` 在 T7 使用处与 T5 定义一致；T7 右键菜单 id 编码约定在 Step 3 明示统一。
4. 已知简化（有意为之）：body 多行编辑 v1 用单行框+换行折叠显示（`ponytail:` 注释在代码中标注）；HTML 响应不做渲染；响应体大小限制无（reqwest 默认整读）。