//! 从 Insomnium（NeDB）/ Yaak（SQLite）迁移接口到 treq 工作区。
//!
//! 聚合层级：**空间（源工作区/项目）→ 服务（源文件夹或 URL 主机）→ 接口**。
//! - 归属明确的请求：collection = `Insomnium · <工作区>` / `Yaak · <工作区>`，group = 源文件夹；
//! - 无文件夹的请求：按 URL 的 host+一级路径 归到「服务」组；
//! - Insomnium 里没有 parentId 的孤儿请求（界面里也看不到）：去重后按服务归到
//!   `<源> · 未归属接口（按服务）` 这个 collection，不丢东西也不淹没主树；
//! - 同一集合内 (method, url) 重复的只保留第一条。
//!
//! 写入一律走 `WorkspaceStore` 的公开 API，保证与 app 的文件布局一致。

use crate::models::*;
use crate::store::WorkspaceStore;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ImportStats {
    pub collections: usize,
    pub groups: usize,
    pub requests: usize,
    /// 因 (method,url) 重复而跳过的条数
    pub deduped: usize,
    /// 源里没有归属、且重复的条数（已去重）
    pub orphan_records: usize,
    pub skipped_collections: Vec<String>,
}

impl ImportStats {
    pub fn summary(&self) -> String {
        format!(
            "collections +{} / groups +{} / requests +{}（同集合内 (method,url) 去重 {}，孤儿记录 {}）",
            self.collections, self.groups, self.requests, self.deduped, self.orphan_records
        )
    }
}

/// 默认数据位置（macOS）。
pub fn default_insomnium_dir() -> PathBuf {
    dirs_home().join("Library/Application Support/Insomnium")
}
pub fn default_yaak_db() -> PathBuf {
    dirs_home().join("Library/Application Support/app.yaak.desktop/db.sqlite")
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

// ===================== 通用小工具 =====================

/// 服务键：`scheme://host[:port]` + 一级路径（没有路径就用主机）。
/// 例：`http://api-gateway:30000/gw/x/y` → `api-gateway:30000/gw`
pub fn service_key(url: &str) -> String {
    // 没有 scheme 的一律归「无主机」（这类 URL 本来也发不出去）
    let Some((_, rest)) = url.split_once("://") else {
        return "(无主机)".to_string();
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    // 去掉 user:pass@，保留 host:port（端口往往就是不同服务）
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty() {
        return "(无主机)".to_string();
    }
    let seg = path.split('/').find(|s| !s.is_empty()).unwrap_or("");
    if seg.is_empty() {
        host.to_string()
    } else {
        format!("{}/{}", host, seg)
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Insomnia 模板 → treq 模板：`{{ _.foo }}` / `{% ... %}` 里的取变量统一成 `{{ foo }}`。
pub fn normalize_template(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && bytes.get(i + 1) == Some(&b'{')
            && let Some(end) = s[i..].find("}}")
        {
            let inner = &s[i + 2..i + end];
            let name = inner
                .trim()
                .trim_start_matches("_.")
                .trim_start_matches('_')
                .trim();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                out.push_str(&format!("{{{{ {} }}}}", name));
                i += end + 2;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Basic → `Authorization: Basic base64(user:pass)`
fn basic_header(user: &str, pass: &str) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, pass));
    format!("Basic {}", b64)
}

/// 保证 header 列表里没有同名项时再补一个（用户自己写过的优先）。
fn push_header_if_absent(headers: &mut Vec<Kv>, key: &str, value: &str) {
    if headers.iter().any(|h| h.key.eq_ignore_ascii_case(key)) {
        return;
    }
    headers.push(Kv {
        key: key.to_string(),
        value: value.to_string(),
        enabled: true,
        description: String::new(),
    });
}

/// 同 (method,url) 去重；返回是否保留。
fn keep_unique(seen: &mut HashSet<(String, String)>, method: &str, url: &str) -> bool {
    seen.insert((method.to_uppercase(), url.trim().to_string()))
}

// ===================== Insomnium（NeDB） =====================

fn read_nedb(dir: &Path, name: &str) -> Result<Vec<Value>> {
    let p = dir.join(format!("insomnia.{}.db", name));
    if !p.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&p).with_context(|| format!("读 {} 失败", p.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect())
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Insomnia 的 body → treq Body。
pub fn insomnium_body(rec: &Value) -> Body {
    let body = rec.get("body").cloned().unwrap_or(Value::Null);
    let mime = s(&body, "mimeType").to_lowercase();
    let text = s(&body, "text");
    let params = body
        .get("params")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    match mime.as_str() {
        "application/json" => Body {
            kind: BodyKind::Json,
            content: normalize_template(&text),
            form_data: Vec::new(),
        },
        "application/x-www-form-urlencoded" => {
            let content = if !text.is_empty() {
                normalize_template(&text)
            } else {
                params
                    .iter()
                    .filter(|p| p.get("disabled").and_then(|d| d.as_bool()) != Some(true))
                    .map(|p| {
                        format!(
                            "{}={}",
                            percent_encode(&s(p, "name")),
                            percent_encode(&s(p, "value"))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("&")
            };
            Body {
                kind: BodyKind::Form,
                content,
                form_data: Vec::new(),
            }
        }
        "multipart/form-data" => Body {
            kind: BodyKind::Multipart,
            content: String::new(),
            form_data: params
                .iter()
                .map(|p| FormField {
                    key: s(p, "name"),
                    value: s(p, "value"),
                    enabled: p.get("disabled").and_then(|d| d.as_bool()) != Some(true),
                    is_file: s(p, "type") == "file",
                })
                .collect(),
        },
        "text/plain" => Body {
            kind: BodyKind::Text,
            content: normalize_template(&text),
            form_data: Vec::new(),
        },
        "" => Body::default(),
        // 其它 mimeType：按原文发（Content-Type 已在 headers 里）
        _ => Body {
            kind: BodyKind::Raw,
            content: normalize_template(&text),
            form_data: Vec::new(),
        },
    }
}

/// Insomnia 记录 → treq 请求（不含 id/name 之外的归属信息）。
pub fn insomnium_request(rec: &Value) -> RequestItem {
    let mut headers: Vec<Kv> = rec
        .get("headers")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|h| !s(h, "name").is_empty())
                .map(|h| Kv {
                    key: s(h, "name"),
                    value: normalize_template(&s(h, "value")),
                    enabled: h.get("disabled").and_then(|d| d.as_bool()) != Some(true),
                    description: s(h, "description"),
                })
                .collect()
        })
        .unwrap_or_default();

    let params: Vec<Kv> = rec
        .get("parameters")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|p| !s(p, "name").is_empty())
                .map(|p| Kv {
                    key: s(p, "name"),
                    value: normalize_template(&s(p, "value")),
                    enabled: p.get("disabled").and_then(|d| d.as_bool()) != Some(true),
                    description: s(p, "description"),
                })
                .collect()
        })
        .unwrap_or_default();

    // authentication → Authorization / apikey 头
    let auth = rec.get("authentication").cloned().unwrap_or(Value::Null);
    match s(&auth, "type").as_str() {
        "bearer" => {
            let token = s(&auth, "token");
            if !token.is_empty() {
                let prefix = {
                    let p = s(&auth, "prefix");
                    if p.is_empty() {
                        "Bearer".to_string()
                    } else {
                        p
                    }
                };
                push_header_if_absent(
                    &mut headers,
                    "Authorization",
                    &format!("{} {}", prefix, token),
                );
            }
        }
        "basic" => {
            let (u, p) = (s(&auth, "username"), s(&auth, "password"));
            if !u.is_empty() || !p.is_empty() {
                push_header_if_absent(&mut headers, "Authorization", &basic_header(&u, &p));
            }
        }
        "apikey" => {
            let (k, v) = (s(&auth, "key"), s(&auth, "value"));
            if !k.is_empty() {
                // addTo=queryParams 的情况由调用方处理，这里只补 header
                if s(&auth, "addTo") != "queryParams" {
                    push_header_if_absent(&mut headers, &k, &v);
                }
            }
        }
        _ => {}
    }

    RequestItem {
        id: String::new(),
        name: s(rec, "name"),
        method: {
            let m = s(rec, "method");
            if m.is_empty() {
                "GET".into()
            } else {
                m.to_uppercase()
            }
        },
        url: normalize_template(&s(rec, "url")),
        params,
        headers,
        body: insomnium_body(rec),
        description: s(rec, "description"),
        docs_open: true,
        // Insomnia 的 authentication 上面已经折成 Authorization / apikey 头了
        auth: None,
    }
}

/// 一条孤儿请求的服务归属（用于「未归属接口（按服务）」分组）。
pub fn orphan_service(rec: &Value) -> String {
    service_key(&s(rec, "url"))
}

pub fn import_insomnium(
    store: &WorkspaceStore,
    dir: &Path,
    existing: &mut HashSet<String>,
    stats: &mut ImportStats,
) -> Result<()> {
    let workspaces = read_nedb(dir, "workspace")?;
    let groups = read_nedb(dir, "RequestGroup")?;
    let requests = read_nedb(dir, "Request")?;
    if workspaces.is_empty() && requests.is_empty() {
        return Ok(());
    }
    let ws_name: HashMap<String, String> = workspaces
        .iter()
        .map(|w| (s(w, "_id"), s(w, "name")))
        .collect();
    let group_map: HashMap<String, Value> =
        groups.iter().map(|g| (s(g, "_id"), g.clone())).collect();

    // 组链：请求 → (workspace id, 分组名)
    let resolve = |parent: &str| -> Option<(String, String)> {
        let mut cur = parent.to_string();
        let mut chain: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        loop {
            if ws_name.contains_key(&cur) {
                // 分组名取最后两级，保留服务上下文
                let name = match chain.len() {
                    0 => String::new(),
                    1 => chain[0].clone(),
                    _ => format!("{} / {}", chain[chain.len() - 2], chain[chain.len() - 1]),
                };
                return Some((cur, name));
            }
            let g = group_map.get(&cur)?;
            if !seen.insert(cur.clone()) {
                return None;
            }
            chain.push(s(g, "name"));
            cur = s(g, "parentId");
            if cur.is_empty() {
                return None;
            }
        }
    };

    // 按 (collection 名, 组名) 分桶
    let mut buckets: BTreeMap<(String, String), Vec<RequestItem>> = BTreeMap::new();
    for rec in &requests {
        let parent = s(rec, "parentId");
        let located = resolve(&parent);
        let (col_name, group_name) = match located {
            Some((ws_id, gname)) => {
                let ws = ws_name.get(&ws_id).cloned().unwrap_or_default();
                if ws.is_empty() {
                    continue;
                }
                // 没有文件夹的请求按「服务」（host + 一级路径）聚合
                let g = if gname.trim().is_empty() {
                    orphan_service(rec)
                } else {
                    gname
                };
                (format!("Insomnium · {}", ws), g)
            }
            None => {
                stats.orphan_records += 1;
                (
                    "Insomnium · 未归属接口（按服务）".to_string(),
                    orphan_service(rec),
                )
            }
        };
        buckets
            .entry((col_name, group_name))
            .or_default()
            .push(insomnium_request(rec));
    }

    write_buckets(store, existing, buckets, stats)
}

// ===================== Yaak（SQLite） =====================

/// Yaak 的 body JSON + body_type → treq Body。
pub fn yaak_body(body_type: Option<&str>, body: &str) -> Body {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let bt = body_type.unwrap_or("").to_lowercase();
    if bt.contains("multipart") {
        let form = v
            .get("form")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        return Body {
            kind: BodyKind::Multipart,
            content: String::new(),
            form_data: form
                .iter()
                .filter(|f| !s(f, "name").is_empty())
                .map(|f| {
                    let file = s(f, "file");
                    FormField {
                        key: s(f, "name"),
                        value: if file.is_empty() { s(f, "value") } else { file },
                        enabled: f.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                        is_file: !s(f, "file").is_empty(),
                    }
                })
                .collect(),
        };
    }
    let text = s(&v, "text");
    if text.is_empty() && bt.is_empty() {
        return Body::default();
    }
    let kind = if bt.contains("json") {
        BodyKind::Json
    } else if bt.contains("x-www-form-urlencoded") {
        BodyKind::Form
    } else if bt.contains("text/plain") {
        BodyKind::Text
    } else {
        BodyKind::Raw
    };
    Body {
        kind,
        content: text,
        form_data: Vec::new(),
    }
}

fn yaak_kvs(json: &str) -> Vec<Kv> {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter(|h| !s(h, "name").is_empty())
                .map(|h| Kv {
                    key: s(h, "name"),
                    value: s(h, "value"),
                    enabled: h.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true),
                    description: String::new(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Yaak 一行 http_request 的字段（便于单测与调用方拼装）。
#[derive(Debug, Default, Clone)]
pub struct YaakRow {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers_json: String,
    pub params_json: String,
    pub body_type: Option<String>,
    pub body: String,
    pub auth_type: Option<String>,
    pub auth: String,
    pub description: String,
}

/// 把 Yaak 的一行 http_request 转成 treq 请求。
pub fn yaak_request(row: &YaakRow) -> RequestItem {
    let YaakRow {
        name,
        method,
        url,
        headers_json,
        params_json,
        body_type,
        body,
        auth_type,
        auth,
        description,
    } = row;
    let mut headers = yaak_kvs(headers_json);
    match auth_type.as_deref().unwrap_or("") {
        "bearer" => {
            let v: Value = serde_json::from_str(auth).unwrap_or(Value::Null);
            let token = s(&v, "token");
            if !token.is_empty() {
                push_header_if_absent(&mut headers, "Authorization", &format!("Bearer {}", token));
            }
        }
        "basic" => {
            let v: Value = serde_json::from_str(auth).unwrap_or(Value::Null);
            let (u, p) = (s(&v, "username"), s(&v, "password"));
            if !u.is_empty() || !p.is_empty() {
                push_header_if_absent(&mut headers, "Authorization", &basic_header(&u, &p));
            }
        }
        _ => {}
    }
    RequestItem {
        id: String::new(),
        name: name.to_string(),
        method: method.to_uppercase(),
        url: url.to_string(),
        params: yaak_kvs(params_json),
        headers,
        body: yaak_body(body_type.as_deref(), body),
        description: description.to_string(),
        docs_open: true,
        auth: None,
    }
}

pub fn import_yaak(
    store: &WorkspaceStore,
    db_path: &Path,
    existing: &mut HashSet<String>,
    stats: &mut ImportStats,
) -> Result<()> {
    if !db_path.exists() {
        return Ok(());
    }
    // 只读打开，绝不能动用户的 Yaak 库
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("只读打开 {} 失败", db_path.display()))?;

    let workspaces: Vec<(String, String)> = conn
        .prepare("SELECT id, name FROM workspaces WHERE deleted_at IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let folders: Vec<(String, Option<String>, String)> = conn
        .prepare("SELECT id, folder_id, name FROM folders WHERE deleted_at IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let folder_name: HashMap<String, (Option<String>, String)> = folders
        .iter()
        .map(|(id, parent, name)| (id.clone(), (parent.clone(), name.clone())))
        .collect();

    let mut stmt = conn.prepare(
        "SELECT workspace_id, folder_id, name, method, url, headers, url_parameters,
                body_type, body, authentication_type, authentication, description
         FROM http_requests WHERE deleted_at IS NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5).unwrap_or_default(),
            r.get::<_, String>(6).unwrap_or_default(),
            r.get::<_, Option<String>>(7)?,
            r.get::<_, String>(8).unwrap_or_default(),
            r.get::<_, Option<String>>(9)?,
            r.get::<_, String>(10).unwrap_or_default(),
            r.get::<_, String>(11).unwrap_or_default(),
        ))
    })?;

    let ws_names: HashMap<String, String> = workspaces.into_iter().collect();
    let mut buckets: BTreeMap<(String, String), Vec<RequestItem>> = BTreeMap::new();
    for row in rows {
        let (
            ws_id,
            folder_id,
            name,
            method,
            url,
            headers,
            params,
            body_type,
            body,
            auth_type,
            auth,
            description,
        ) = row?;
        let Some(ws) = ws_names.get(&ws_id) else {
            continue;
        };
        // 分组名：folder 链的最内层；没有 folder 的按服务聚合
        let group = folder_id
            .as_deref()
            .and_then(|id| folder_name.get(id))
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| service_key(&url));
        let req = yaak_request(&YaakRow {
            name,
            method,
            url,
            headers_json: headers,
            params_json: params,
            body_type,
            body,
            auth_type,
            auth,
            description,
        });
        buckets
            .entry((format!("Yaak · {}", ws), group))
            .or_default()
            .push(req);
    }

    write_buckets(store, existing, buckets, stats)
}

// ===================== 落盘 =====================

fn write_buckets(
    store: &WorkspaceStore,
    existing: &mut HashSet<String>,
    buckets: BTreeMap<(String, String), Vec<RequestItem>>,
    stats: &mut ImportStats,
) -> Result<()> {
    // collection → group id
    let mut group_ids: HashMap<(String, String), String> = HashMap::new();
    let mut seen: HashMap<String, HashSet<(String, String)>> = HashMap::new();
    for ((col_name, group_name), reqs) in buckets {
        if !existing.contains(&col_name) {
            store.create_collection(&col_name)?;
            existing.insert(col_name.clone());
            stats.collections += 1;
        }
        let col_id = store
            .load()?
            .collections
            .into_iter()
            .find(|c| c.name == col_name)
            .map(|c| c.id)
            .with_context(|| format!("找不到刚建的集合 {}", col_name))?;

        let group_id = if group_name.trim().is_empty() {
            None
        } else {
            let key = (col_name.clone(), group_name.clone());
            if let Some(id) = group_ids.get(&key) {
                Some(id.clone())
            } else {
                let g = store.create_group(&col_id, &group_name, None)?;
                stats.groups += 1;
                group_ids.insert(key, g.id.clone());
                Some(g.id)
            }
        };

        let dedupe_bucket = seen.entry(col_name.clone()).or_default();
        for req in reqs {
            if !keep_unique(dedupe_bucket, &req.method, &req.url) {
                stats.deduped += 1;
                continue;
            }
            let mut created = store.create_request_in(&col_id, group_id.as_deref(), &req.name)?;
            let id = created.id.clone();
            created.method = req.method;
            created.url = req.url;
            created.params = req.params;
            created.headers = req.headers;
            created.body = req.body;
            created.description = req.description;
            created.docs_open = true;
            created.id = id;
            store.save_request(&created)?;
            stats.requests += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn service_key_groups_by_host_and_first_path() {
        assert_eq!(
            service_key("http://api-gateway.example.com:30000/gw/foo/api/r?x=1"),
            "api-gateway.example.com:30000/gw"
        );
        assert_eq!(
            service_key("https://qyapi.weixin.qq.com/cgi-bin/a/b"),
            "qyapi.weixin.qq.com/cgi-bin"
        );
        assert_eq!(service_key("https://host/"), "host");
        assert_eq!(service_key("not a url"), "(无主机)");
    }

    #[test]
    fn template_syntax_is_normalized() {
        assert_eq!(normalize_template("{{ _.baseUrl }}/x"), "{{ baseUrl }}/x");
        assert_eq!(normalize_template("{{base}}/y"), "{{ base }}/y");
        // 非变量模板（函数调用）原样保留
        assert_eq!(normalize_template("{% now %}/x"), "{% now %}/x");
    }

    #[test]
    fn insomnium_maps_params_headers_body_auth() {
        let rec = json!({
            "name": "客户详情", "method": "post",
            "url": "http://h/api/x",
            "parameters": [{"name": "userId", "value": "qiwei"}, {"name": "off", "value": "1", "disabled": true}],
            "headers": [{"name": "Content-Type", "value": "application/json"}, {"name": "X-Skip", "value": "1", "disabled": true}],
            "body": {"mimeType": "application/json", "text": "{\"a\":1}"},
            "authentication": {"type": "bearer", "token": "tk"}
        });
        let r = insomnium_request(&rec);
        assert_eq!(r.method, "POST");
        assert_eq!(r.params.len(), 2);
        assert!(!r.params[1].enabled, "disabled 参数要保留但禁用");
        assert_eq!(r.headers.len(), 3, "两个原头 + 补的 Authorization");
        assert!(
            r.headers
                .iter()
                .any(|h| h.key == "Authorization" && h.value == "Bearer tk")
        );
        assert_eq!(r.body.kind, BodyKind::Json);
        assert_eq!(r.body.content, "{\"a\":1}");
    }

    #[test]
    fn insomnium_form_and_multipart() {
        let form = json!({
            "name": "f", "method": "POST", "url": "http://h/f",
            "body": {"mimeType": "application/x-www-form-urlencoded",
                     "params": [{"name": "a b", "value": "1&2"}]}
        });
        let r = insomnium_request(&form);
        assert_eq!(r.body.kind, BodyKind::Form);
        assert_eq!(r.body.content, "a%20b=1%262", "表单值要 urlencode");

        let mp = json!({
            "name": "m", "method": "POST", "url": "http://h/m",
            "body": {"mimeType": "multipart/form-data",
                     "params": [{"name": "name", "value": "treq", "type": "text"},
                                {"name": "file", "value": "/tmp/a.png", "type": "file"}]}
        });
        let r = insomnium_request(&mp);
        assert_eq!(r.body.kind, BodyKind::Multipart);
        assert!(r.body.form_data[1].is_file);
    }

    #[test]
    fn insomnium_basic_auth_is_base64() {
        let rec = json!({
            "name": "b", "method": "GET", "url": "http://h/b",
            "authentication": {"type": "basic", "username": "u", "password": "p"}
        });
        let r = insomnium_request(&rec);
        assert!(
            r.headers.iter().any(|h| h.value == "Basic dTpw"),
            "u:p 的 base64 应为 dTpw"
        );
    }

    #[test]
    fn yaak_maps_body_variants() {
        let json_body = yaak_body(Some("application/json"), "{\"text\":\"{\\\"a\\\":1}\"}");
        assert_eq!(json_body.kind, BodyKind::Json);
        assert_eq!(json_body.content, "{\"a\":1}");

        let mp = yaak_body(
            Some("multipart/form-data"),
            "{\"form\":[{\"name\":\"secret\",\"value\":\"600109\"},{\"name\":\"file\",\"file\":\"/tmp/a.xlsx\"}]}",
        );
        assert_eq!(mp.kind, BodyKind::Multipart);
        assert!(!mp.form_data[0].is_file);
        assert!(mp.form_data[1].is_file);
        assert_eq!(mp.form_data[1].value, "/tmp/a.xlsx");

        assert_eq!(yaak_body(None, "{}").kind, BodyKind::None);
        assert_eq!(
            yaak_body(Some("text/xml"), "{\"text\":\"<a/>\"}").kind,
            BodyKind::Raw
        );
    }

    #[test]
    fn yaak_maps_headers_params_and_auth() {
        let r = yaak_request(&YaakRow {
            name: "n".into(),
            method: "get".into(),
            url: "http://h/x".into(),
            headers_json: r#"[{"name":"Accept","value":"application/json","enabled":true},{"name":"","value":"","enabled":true}]"#.into(),
            params_json: r#"[{"name":"toSync","value":"true","enabled":false}]"#.into(),
            body_type: Some("application/json".into()),
            body: r#"{"text":"{}"}"#.into(),
            auth_type: Some("bearer".into()),
            auth: r#"{"token":"tk"}"#.into(),
            description: "说明".into(),
        });
        assert_eq!(r.method, "GET");
        assert_eq!(r.headers.len(), 2, "空名头丢弃 + 补 Authorization");
        assert!(!r.params[0].enabled);
        assert_eq!(r.description, "说明");
    }

    #[test]
    fn dedupe_key_is_method_and_url() {
        let mut seen = HashSet::new();
        assert!(keep_unique(&mut seen, "get", "http://h/a"));
        assert!(
            !keep_unique(&mut seen, "GET", "http://h/a"),
            "同 URL 不同大小写方法视为重复"
        );
        assert!(keep_unique(&mut seen, "POST", "http://h/a"));
    }
}
