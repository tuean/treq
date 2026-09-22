//! 从别的工具的描述文件导入接口：Postman v2.x 集合、OpenAPI 3.x、HAR 1.2。
//!
//! 一条原则：**能映尽映，映不了的不假装**。解析结果直接给一个可落盘的 `Collection`。
//! 说明性差异（OpenAPI 的 path 参数、Postman 的 collection 变量等）记在请求的 description 里，
//! 用户点开就能看到而不是默默丢。

use crate::auth::{Auth, KeyIn};
use crate::models::{Body, BodyKind, Collection, FormField, Group, Kv, RequestItem};
use crate::store::new_id;
use anyhow::{Result, anyhow};
use serde_json::Value;

/// 认出来的格式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    Postman,
    OpenApi,
    Har,
}

impl ImportFormat {
    /// i18n key 后缀（app 用来显示「已导入 Postman 集合」这类提示）。
    pub fn id(&self) -> &'static str {
        match self {
            ImportFormat::Postman => "postman",
            ImportFormat::OpenApi => "openapi",
            ImportFormat::Har => "har",
        }
    }
}

/// 导入结果。
#[derive(Debug)]
pub struct Imported {
    pub format: ImportFormat,
    pub collection: Collection,
    pub request_count: usize,
    pub group_count: usize,
}

/// 文件内容 → JSON（JSON 直接读，YAML 转一道；OpenAPI 常常是 YAML）。
fn to_json(text: &str) -> Result<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Ok(v);
    }
    let y: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|e| anyhow!("既不是 JSON 也不是 YAML：{}", e))?;
    serde_json::to_value(y).map_err(|e| anyhow!("YAML 转换失败：{}", e))
}

/// 认格式：看字段形状，不看文件后缀。
pub fn detect(text: &str) -> Option<ImportFormat> {
    let v = to_json(text).ok()?;
    detect_value(&v)
}

fn detect_value(v: &Value) -> Option<ImportFormat> {
    // Postman：info.schema 里有 schema.getpostman.com，或者有 info + item
    if v.get("info").is_some() && v.get("item").is_some() {
        return Some(ImportFormat::Postman);
    }
    if v.get("openapi").is_some() || (v.get("swagger").is_some() && v.get("paths").is_some()) {
        return Some(ImportFormat::OpenApi);
    }
    if v.get("log").and_then(|l| l.get("entries")).is_some() {
        return Some(ImportFormat::Har);
    }
    None
}

/// 导入一段文本。`fallback_name` 是文件名（文件里没有名字时用）。
pub fn import_text(text: &str, fallback_name: &str) -> Result<Imported> {
    let v = to_json(text)?;
    let format = detect_value(&v)
        .ok_or_else(|| anyhow!("认不出的格式（支持 Postman 集合 / OpenAPI 3 / HAR）"))?;
    let (name, groups, requests) = match format {
        ImportFormat::Postman => postman(&v, fallback_name)?,
        ImportFormat::OpenApi => openapi(&v, fallback_name)?,
        ImportFormat::Har => har(&v, fallback_name)?,
    };
    let group_count = groups.len();
    let request_count = requests.len() + groups.iter().map(|g| g.requests.len()).sum::<usize>();
    let collection = Collection {
        id: new_id(),
        name,
        groups,
        requests,
    };
    Ok(Imported {
        format,
        collection,
        request_count,
        group_count,
    })
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

fn kv(key: &str, value: &str) -> Kv {
    Kv {
        key: key.to_string(),
        value: value.to_string(),
        enabled: true,
        description: String::new(),
    }
}

// ---------------------------------------------------------------- Postman

/// Postman 的 auth 形状：`{type: "bearer", bearer: [{key, value}, ...]}` ——
/// 数组挂在**类型名**下，条目里的 `key` 才是字段名。
fn pm_auth_field(auth: &Value, ty: &str, name: &str) -> String {
    auth.get("auth")
        .and_then(|a| a.get(ty))
        .and_then(|arr| arr.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|e| e.get("key").and_then(|k| k.as_str()) == Some(name))
        })
        .and_then(|e| e.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Postman `auth` → 我们的 Auth（认不出就 None，原文记 description）。
fn pm_auth(auth: &Value) -> Option<Auth> {
    let ty = auth
        .get("auth")
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str())?;
    Some(match ty {
        "bearer" => Auth::Bearer {
            token: pm_auth_field(auth, "bearer", "token"),
            prefix: String::new(),
        },
        "basic" => Auth::Basic {
            username: pm_auth_field(auth, "basic", "username"),
            password: pm_auth_field(auth, "basic", "password"),
        },
        "apikey" => {
            let location = if pm_auth_field(auth, "apikey", "in") == "query" {
                KeyIn::Query
            } else {
                KeyIn::Header
            };
            Auth::ApiKey {
                key: pm_auth_field(auth, "apikey", "key"),
                value: pm_auth_field(auth, "apikey", "value"),
                location,
            }
        }
        _ => return None,
    })
}

fn pm_body(req: &Value) -> Body {
    let Some(b) = req.get("body") else {
        return Body::default();
    };
    let mode = s(b, "mode");
    match mode.as_str() {
        "raw" => {
            let raw = s(b, "raw");
            let lang = b
                .get("options")
                .and_then(|o| o.get("raw"))
                .and_then(|r| r.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            let kind = if lang == "json" || raw.trim_start().starts_with(['{', '[']) {
                BodyKind::Json
            } else {
                BodyKind::Text
            };
            Body {
                kind,
                content: raw,
                form_data: vec![],
            }
        }
        "urlencoded" => Body {
            kind: BodyKind::Form,
            content: b
                .get("urlencoded")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter(|e| e.get("disabled").and_then(|d| d.as_bool()) != Some(true))
                        .map(|e| {
                            format!(
                                "{}={}",
                                e.get("key").and_then(|k| k.as_str()).unwrap_or(""),
                                e.get("value").and_then(|v| v.as_str()).unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default(),
            form_data: vec![],
        },
        "formdata" => Body {
            kind: BodyKind::Multipart,
            content: String::new(),
            form_data: b
                .get("formdata")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|e| FormField {
                            key: e
                                .get("key")
                                .and_then(|k| k.as_str())
                                .unwrap_or("")
                                .to_string(),
                            value: e
                                .get("src")
                                .and_then(|v| v.as_str())
                                .or_else(|| e.get("value").and_then(|v| v.as_str()))
                                .unwrap_or("")
                                .to_string(),
                            enabled: e.get("disabled").and_then(|d| d.as_bool()) != Some(true),
                            is_file: e.get("type").and_then(|t| t.as_str()) == Some("file"),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        _ => Body::default(),
    }
}

/// 递归扁平化 Postman 的 `item[]`；嵌套文件夹用 `/` 连起来当分组名。
fn pm_collect(
    items: &[Value],
    prefix: &[String],
    col_auth: &Option<Auth>,
    groups: &mut Vec<Group>,
    top: &mut Vec<RequestItem>,
) {
    for it in items {
        let raw_name = s(it, "name");
        if let Some(children) = it.get("item").and_then(|c| c.as_array()) {
            let mut path = prefix.to_vec();
            path.push(if raw_name.is_empty() {
                "文件夹".to_string()
            } else {
                raw_name
            });
            pm_collect(children, &path, col_auth, groups, top);
            continue;
        }
        let Some(req) = it.get("request") else {
            continue;
        };
        // url 可能是字符串也可能是对象
        let url = match req.get("url") {
            Some(Value::String(u)) => u.clone(),
            Some(o) => {
                let raw = s(o, "raw");
                if !raw.is_empty() {
                    raw
                } else {
                    let host = s(o, "host").trim_matches('"').to_string();
                    let path = o
                        .get("path")
                        .and_then(|p| p.as_array())
                        .map(|segs| {
                            segs.iter()
                                .map(|x| x.as_str().unwrap_or_default())
                                .collect::<Vec<_>>()
                                .join("/")
                        })
                        .unwrap_or_default();
                    format!("{}/{}", host.trim_end_matches('/'), path)
                }
            }
            None => String::new(),
        };
        let mut headers: Vec<Kv> = req
            .get("header")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|h| Kv {
                        key: s(h, "key"),
                        value: s(h, "value"),
                        enabled: h.get("disabled").and_then(|d| d.as_bool()) != Some(true),
                        description: s(h, "description"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        headers.retain(|h| !h.key.is_empty());

        // 认证：请求自己的优先，否则继承集合的
        let auth = pm_auth(it)
            .or_else(|| pm_auth(req))
            .or_else(|| col_auth.clone());
        let mut item = RequestItem {
            id: new_id(),
            name: if raw_name.is_empty() {
                format!("{} {}", s(req, "method"), url)
            } else {
                raw_name
            },
            method: {
                let m = s(req, "method");
                if m.is_empty() {
                    "GET".into()
                } else {
                    m.to_uppercase()
                }
            },
            url,
            params: req
                .get("url")
                .and_then(|u| u.get("query"))
                .and_then(|q| q.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|q| Kv {
                            key: s(q, "key"),
                            value: s(q, "value"),
                            enabled: q.get("disabled").and_then(|d| d.as_bool()) != Some(true),
                            description: String::new(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            headers,
            body: pm_body(req),
            description: it
                .get("description")
                .or_else(|| req.get("description"))
                .map(|d| match d {
                    Value::String(t) => t.clone(),
                    other => other
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .unwrap_or_default(),
            docs_open: false,
            auth,
            order: None,
        };
        item.params.retain(|p| !p.key.is_empty());
        if prefix.is_empty() {
            top.push(item);
        } else {
            let gname = prefix.join(" / ");
            if let Some(g) = groups.iter_mut().find(|g| g.name == gname) {
                g.requests.push(item);
            } else {
                groups.push(Group {
                    id: new_id(),
                    parent: None,
                    name: gname,
                    requests: vec![item],
                    order: None,
                });
            }
        }
    }
}

fn postman(v: &Value, fallback: &str) -> Result<(String, Vec<Group>, Vec<RequestItem>)> {
    let name = {
        let n = v.get("info").map(|i| s(i, "name")).unwrap_or_default();
        if n.is_empty() {
            fallback.to_string()
        } else {
            n
        }
    };
    // 集合级 auth（{type, bearer: [...]}），请求自己没有 auth 时继承
    let col_auth = pm_auth(v);
    let items = v
        .get("item")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_default();
    let mut groups: Vec<Group> = Vec::new();
    let mut top: Vec<RequestItem> = Vec::new();
    pm_collect(&items, &[], &col_auth, &mut groups, &mut top);
    Ok((name, groups, top))
}

// ---------------------------------------------------------------- OpenAPI

/// OpenAPI 的 `securitySchemes` → 我们的 Auth；取需求里的第一个。
fn oa_auth(v: &Value, op: &Value) -> Option<Auth> {
    let sec = op
        .get("security")
        .or_else(|| v.get("security"))
        .and_then(|s| s.as_array())?
        .first()?
        .as_object()?;
    let scheme_name = sec.keys().next()?;
    let schemes = v
        .get("components")
        .and_then(|c| c.get("securitySchemes"))
        .or_else(|| v.get("securityDefinitions"))?;
    let sc = schemes.get(scheme_name)?;
    let ty = s(sc, "type");
    match ty.as_str() {
        "http" if s(sc, "scheme").eq_ignore_ascii_case("bearer") => Some(Auth::Bearer {
            token: "{{ token }}".into(),
            prefix: String::new(),
        }),
        "http" if s(sc, "scheme").eq_ignore_ascii_case("basic") => Some(Auth::Basic {
            username: "{{ username }}".into(),
            password: "{{ password }}".into(),
        }),
        "apiKey" => Some(Auth::ApiKey {
            key: s(sc, "name"),
            value: "{{ api_key }}".into(),
            location: if s(sc, "in") == "query" {
                KeyIn::Query
            } else {
                KeyIn::Header
            },
        }),
        _ => None,
    }
}

/// 按 schema 造一个骨架 JSON（没有 example 时用），够看清字段就行。
fn oa_skeleton(schema: &Value, depth: u8) -> Value {
    if depth > 4 {
        return Value::Null;
    }
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    if let Some(ty) = schema.get("type").and_then(|t| t.as_str()) {
        return match ty {
            "object" => {
                let mut m = serde_json::Map::new();
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    for (k, sub) in props {
                        m.insert(k.clone(), oa_skeleton(sub, depth + 1));
                    }
                }
                Value::Object(m)
            }
            "array" => Value::Array(vec![
                schema
                    .get("items")
                    .map(|i| oa_skeleton(i, depth + 1))
                    .unwrap_or(Value::Null),
            ]),
            "integer" | "number" => Value::from(0),
            "boolean" => Value::from(false),
            _ => Value::from(""),
        };
    }
    Value::Null
}

fn oa_body(op: &Value) -> Body {
    let Some(content) = op
        .get("requestBody")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_object())
    else {
        return Body::default();
    };
    // 优先 json，其次第一个
    let (ct, media) = content
        .iter()
        .find(|(k, _)| k.contains("json"))
        .or_else(|| content.iter().next())
        .map(|(k, v)| (k.clone(), v))
        .unwrap();
    if ct.contains("form-urlencoded") {
        let fields = media
            .get("schema")
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.as_object())
            .map(|p| {
                p.keys()
                    .map(|k| format!("{}=", k))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        return Body {
            kind: BodyKind::Form,
            content: fields,
            form_data: vec![],
        };
    }
    let payload = media
        .get("example")
        .cloned()
        .or_else(|| media.get("schema").map(|s| oa_skeleton(s, 0)))
        .unwrap_or(Value::Null);
    if payload.is_null() {
        return Body::default();
    }
    let pretty = serde_json::to_string_pretty(&payload).unwrap_or_default();
    Body {
        kind: if ct.contains("json") {
            BodyKind::Json
        } else {
            BodyKind::Raw
        },
        content: pretty,
        form_data: vec![],
    }
}

fn openapi(v: &Value, fallback: &str) -> Result<(String, Vec<Group>, Vec<RequestItem>)> {
    let paths = v
        .get("paths")
        .and_then(|p| p.as_object())
        .ok_or_else(|| anyhow!("OpenAPI 里没有 paths"))?;
    let name = {
        let t = v.get("info").map(|i| s(i, "title")).unwrap_or_default();
        if t.is_empty() {
            fallback.to_string()
        } else {
            t
        }
    };
    let base = v
        .get("servers")
        .and_then(|s| s.as_array())
        .and_then(|a| a.first())
        .map(|s| {
            let mut u = s_url_prefix(s);
            u.push_str(&s_url_path(s));
            u
        })
        .unwrap_or_default();
    const METHODS: [&str; 8] = [
        "get", "post", "put", "patch", "delete", "head", "options", "trace",
    ];
    let mut groups: Vec<Group> = Vec::new();
    let mut top: Vec<RequestItem> = Vec::new();
    for (path, item) in paths {
        for m in METHODS {
            let Some(op) = item.get(m) else { continue };
            let mut url = format!("{}{}", base.trim_end_matches('/'), path);
            let mut params: Vec<Kv> = Vec::new();
            let mut headers: Vec<Kv> = Vec::new();
            let mut path_params: Vec<String> = Vec::new();
            for p in op
                .get("parameters")
                .and_then(|p| p.as_array())
                .into_iter()
                .flatten()
                .chain(
                    item.get("parameters")
                        .and_then(|p| p.as_array())
                        .into_iter()
                        .flatten(),
                )
            {
                let pname = s(p, "name");
                let pin = s(p, "in");
                let example = p
                    .get("example")
                    .and_then(|e| e.as_str())
                    .map(|x| x.to_string())
                    .or_else(|| {
                        p.get("schema")
                            .and_then(|s| s.get("example"))
                            .and_then(|e| e.as_str())
                            .map(|x| x.to_string())
                    })
                    .unwrap_or_default();
                match pin.as_str() {
                    "query" => params.push(kv(&pname, &example)),
                    "header" => headers.push(kv(&pname, &example)),
                    "path" => path_params.push(pname),
                    _ => {}
                }
            }
            // 同一模板里可能有前缀（"https://h/v1"）
            url = url.replace("//", "/").replacen(":/", "://", 1);
            let mut desc = s(op, "description");
            let summary = s(op, "summary");
            if !summary.is_empty() {
                desc = format!(
                    "{}

{}",
                    summary, desc
                );
            }
            if !path_params.is_empty() {
                let list = path_params
                    .iter()
                    .map(|p| format!("{{{}}}", p))
                    .collect::<Vec<_>>()
                    .join("、");
                let first = format!("{{{}}}", path_params[0]);
                desc.push_str(&format!(
                    "\n\n> 路径参数待填：{list}\n> URL 里的 {first} 还是原文，可换成 {{{{ 变量 }}}} 让它在环境里配"
                ));
            }
            let auth = oa_auth(v, op);
            let req = RequestItem {
                id: new_id(),
                name: {
                    let n = if summary.is_empty() {
                        s(op, "operationId")
                    } else {
                        summary
                    };
                    if n.is_empty() {
                        format!("{} {}", m.to_uppercase(), path)
                    } else {
                        format!("{} · {}", m.to_uppercase(), n)
                    }
                },
                method: m.to_uppercase(),
                url,
                params,
                headers,
                body: oa_body(op),
                description: desc.trim().to_string(),
                docs_open: false,
                auth,
                order: None,
            };
            let tag = op
                .get("tags")
                .and_then(|t| t.as_array())
                .and_then(|a| a.first())
                .and_then(|t| t.as_str())
                .unwrap_or("默认")
                .to_string();
            if tag == "默认" {
                top.push(req);
            } else if let Some(g) = groups.iter_mut().find(|g| g.name == tag) {
                g.requests.push(req);
            } else {
                groups.push(Group {
                    id: new_id(),
                    parent: None,
                    name: tag,
                    requests: vec![req],
                    order: None,
                });
            }
        }
    }
    Ok((name, groups, top))
}

fn s_url_prefix(s: &Value) -> String {
    s.get("url")
        .and_then(|u| u.as_str())
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_default()
}

fn s_url_path(s: &Value) -> String {
    let p = s.get("variables").and_then(|v| v.as_object());
    if p.is_some() {
        return String::new();
    }
    String::new()
}

// ---------------------------------------------------------------- HAR

fn har(v: &Value, fallback: &str) -> Result<(String, Vec<Group>, Vec<RequestItem>)> {
    let entries = v
        .get("log")
        .and_then(|l| l.get("entries"))
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("HAR 里没有 log.entries"))?;
    let mut groups: Vec<Group> = Vec::new();
    for e in entries {
        let Some(req) = e.get("request") else {
            continue;
        };
        let url = s(req, "url");
        let method = {
            let m = s(req, "method");
            if m.is_empty() {
                "GET".into()
            } else {
                m.to_uppercase()
            }
        };
        let headers: Vec<Kv> = req
            .get("headers")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|h| {
                        // HAR 里的伪头（`:authority` 这类）没法直接发，跳过
                        !s(h, "name").starts_with(':')
                    })
                    .map(|h| kv(&s(h, "name"), &s(h, "value")))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let post = req.get("postData");
        let body = match post {
            Some(p) => {
                let mime = s(p, "mimeType");
                let text = s(p, "text");
                Body {
                    kind: if mime.contains("json") {
                        BodyKind::Json
                    } else if mime.contains("x-www-form-urlencoded") {
                        BodyKind::Form
                    } else {
                        BodyKind::Text
                    },
                    content: text,
                    form_data: vec![],
                }
            }
            None => Body::default(),
        };
        let bucket = host_group(&url);
        let item = RequestItem {
            id: new_id(),
            name: {
                let p = path_of(&url);
                if p.is_empty() {
                    format!("{} {}", method, url)
                } else {
                    format!("{} {}", method, p)
                }
            },
            method,
            url,
            params: vec![],
            headers,
            body,
            description: String::new(),
            docs_open: false,
            auth: None,
            order: None,
        };
        if let Some(g) = groups.iter_mut().find(|g| g.name == bucket) {
            g.requests.push(item);
        } else {
            groups.push(Group {
                id: new_id(),
                parent: None,
                name: bucket,
                requests: vec![item],
                order: None,
            });
        }
    }
    Ok((fallback.to_string(), groups, Vec::new()))
}

/// 分组名：URL 的 host（HAR 是按域名分的，和「按服务分组」一个思路）。
fn host_group(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
    if host.is_empty() {
        "(无 host)".to_string()
    } else {
        host.to_string()
    }
}

/// URL 的路径部分（去掉查询串），拿来做请求名。
fn path_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let after_host = rest.split(['/', '?', '#']).nth(1).unwrap_or("");
    let path = after_host
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    if path.is_empty() {
        String::new()
    } else {
        format!("/{}", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSTMAN: &str = r#"{
      "info": {"name": "支付网关", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
      "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "{{ pm_token }}"}]},
      "item": [
        {"name": "下单", "request": {
            "method": "POST",
            "url": {"raw": "https://pay.example.com/v1/orders", "query": [{"key": "dry", "value": "1"}]},
            "header": [{"key": "X-Trace", "value": "t1"}, {"key": "X-Off", "value": "x", "disabled": true}],
            "body": {"mode": "raw", "raw": "{\"amount\": 1}", "options": {"raw": {"language": "json"}}}},
         "description": "创建订单"},
        {"name": "查询", "item": [
            {"name": "按 id", "request": {"method": "GET", "url": "https://pay.example.com/v1/orders/1"}}
        ]}
      ]
    }"#;

    #[test]
    fn postman_maps_requests_groups_and_auth() {
        let out = import_text(POSTMAN, "fallback").unwrap();
        assert_eq!(out.format, ImportFormat::Postman);
        assert_eq!(out.collection.name, "支付网关");
        assert_eq!(out.request_count, 2);
        assert_eq!(out.group_count, 1);
        let order = &out.collection.requests[0];
        assert_eq!(order.name, "下单");
        assert_eq!(order.method, "POST");
        assert_eq!(order.url, "https://pay.example.com/v1/orders");
        assert_eq!(order.body.kind, BodyKind::Json);
        assert!(order.body.content.contains("\"amount\": 1"));
        assert_eq!(order.params.len(), 1);
        assert_eq!(order.params[0].key, "dry");
        // 禁用的头保留但禁用
        assert_eq!(order.headers.len(), 2);
        assert!(!order.headers[1].enabled);
        assert_eq!(order.description, "创建订单");
        // 集合级 bearer 被继承
        match order.auth.as_ref().unwrap() {
            Auth::Bearer { token, .. } => assert_eq!(token, "{{ pm_token }}"),
            other => panic!("应继承 bearer：{other:?}"),
        }
        // 嵌套文件夹变成分组
        let g = &out.collection.groups[0];
        assert_eq!(g.name, "查询");
        assert_eq!(g.requests[0].name, "按 id");
        assert_eq!(g.requests[0].method, "GET");
        // 分组里的请求也继承认证
        assert!(g.requests[0].auth.is_some());
    }

    #[test]
    fn postman_apikey_and_form() {
        let txt = r#"{"info":{"name":"x"},"item":[{"name":"k","request":{
            "method":"POST","url":"https://a/b",
            "auth":{"type":"apikey","apikey":[{"key":"key","value":"X-Key"},{"key":"value","value":"{{ k }}"},{"key":"in","value":"query"}]},
            "body":{"mode":"urlencoded","urlencoded":[{"key":"a","value":"1"},{"key":"skip","value":"2","disabled":true}]}}}]}"#;
        let out = import_text(txt, "f").unwrap();
        let r = &out.collection.requests[0];
        match r.auth.as_ref().unwrap() {
            Auth::ApiKey {
                key,
                value,
                location,
            } => {
                assert_eq!(key, "X-Key");
                assert_eq!(value, "{{ k }}");
                assert_eq!(*location, KeyIn::Query);
            }
            other => panic!("应是 apikey：{other:?}"),
        }
        assert_eq!(r.body.kind, BodyKind::Form);
        assert_eq!(r.body.content, "a=1"); // 禁用字段不进
    }

    const OPENAPI: &str = r#"{
      "openapi": "3.0.0",
      "info": {"title": "订单服务", "version": "1"},
      "servers": [{"url": "https://api.example.com/v2"}],
      "components": {"securitySchemes": {"bearerAuth": {"type": "http", "scheme": "bearer"}}},
      "security": [{"bearerAuth": []}],
      "paths": {
        "/orders/{id}": {
          "get": {
            "summary": "查订单", "tags": ["orders"],
            "parameters": [
              {"name": "id", "in": "path", "required": true},
              {"name": "verbose", "in": "query", "example": "true"},
              {"name": "X-Tenant", "in": "header"}
            ]
          },
          "delete": {"operationId": "deleteOrder", "tags": ["orders"],
            "parameters": [{"name": "id", "in": "path"}]}
        },
        "/orders": {
          "post": {
            "summary": "建单",
            "requestBody": {"content": {"application/json": {"schema": {"type": "object",
              "properties": {"amount": {"type": "number"}, "note": {"type": "string"}, "items": {"type": "array", "items": {"type": "string"}}}}}}}
          }
        }
      }
    }"#;

    #[test]
    fn openapi_maps_paths_tags_body_and_auth() {
        let out = import_text(OPENAPI, "spec").unwrap();
        assert_eq!(out.format, ImportFormat::OpenApi);
        assert_eq!(out.collection.name, "订单服务");
        assert_eq!(out.request_count, 3);
        // 有 tag 的进分组
        assert_eq!(out.group_count, 1);
        assert_eq!(out.collection.groups[0].name, "orders");
        assert_eq!(out.collection.groups[0].requests.len(), 2);
        // 无 tag 的留在顶层
        let post = &out.collection.requests[0];
        assert_eq!(post.method, "POST");
        assert_eq!(post.url, "https://api.example.com/v2/orders");
        assert_eq!(post.body.kind, BodyKind::Json);
        // 骨架：number→0、string→""、array→[""]
        let v: Value = serde_json::from_str(&post.body.content).unwrap();
        assert_eq!(v["amount"], serde_json::json!(0));
        assert_eq!(v["note"], serde_json::json!(""));
        assert_eq!(v["items"], serde_json::json!([""]));
        // path/query/header 参数
        let get = &out.collection.groups[0].requests[0];
        assert_eq!(get.url, "https://api.example.com/v2/orders/{id}");
        assert_eq!(get.params[0].key, "verbose");
        assert_eq!(get.params[0].value, "true");
        assert_eq!(get.headers[0].key, "X-Tenant");
        assert!(
            get.description.contains("路径参数待填：{id}"),
            "{}",
            get.description
        );
        // security → bearer（token 用变量占位，别编造凭据）
        match get.auth.as_ref().unwrap() {
            Auth::Bearer { token, .. } => assert_eq!(token, "{{ token }}"),
            other => panic!("应映射 bearer：{other:?}"),
        }
        // operationId 兜底命名
        assert!(out.collection.groups[0].requests[1].name.contains("DELETE"));
    }

    #[test]
    fn openapi_apikey_query_scheme() {
        let txt = r#"{"openapi":"3.0.1","info":{"title":"t"},"servers":[{"url":"http://h"}],
          "components":{"securitySchemes":{"k":{"type":"apiKey","name":"api_key","in":"query"}}},
          "paths":{"/a":{"get":{"security":[{"k":[]}]}}}}"#;
        let out = import_text(txt, "f").unwrap();
        match out.collection.requests[0].auth.as_ref().unwrap() {
            Auth::ApiKey { key, location, .. } => {
                assert_eq!(key, "api_key");
                assert_eq!(*location, KeyIn::Query);
            }
            other => panic!("应是 apikey：{other:?}"),
        }
    }

    #[test]
    fn har_groups_by_service() {
        let txt = r#"{"log":{"version":"1.2","entries":[
          {"request":{"method":"POST","url":"http://127.0.0.1:8321/json","headers":[{"name":"Accept","value":"*/*"},{"name":":authority","value":"x"}],
            "postData":{"mimeType":"application/json","text":"{\"a\":1}"}}},
          {"request":{"method":"GET","url":"http://127.0.0.1:8321/json","headers":[]}}
        ]}}"#;
        let out = import_text(txt, "capture.har").unwrap();
        assert_eq!(out.format, ImportFormat::Har);
        assert_eq!(out.collection.name, "capture.har");
        assert_eq!(out.request_count, 2);
        assert_eq!(out.group_count, 1);
        let r = &out.collection.groups[0].requests[0];
        assert_eq!(r.method, "POST");
        assert_eq!(r.body.kind, BodyKind::Json);
        // 伪头 :authority 被丢掉
        assert_eq!(r.headers.len(), 1);
        assert_eq!(r.headers[0].key, "Accept");
        assert!(out.collection.requests.is_empty());
    }

    #[test]
    fn detect_and_reject() {
        assert_eq!(detect(POSTMAN), Some(ImportFormat::Postman));
        assert_eq!(detect(OPENAPI), Some(ImportFormat::OpenApi));
        assert_eq!(detect("just text"), None);
        // YAML 版 OpenAPI 也认（openapi 3 经常写成 yaml）
        let yaml = "openapi: 3.0.0\ninfo:\n  title: T\npaths:\n  /a:\n    get: {}\n";
        assert_eq!(detect(yaml), Some(ImportFormat::OpenApi));
        let e = import_text("curl -X GET http://x", "x.txt").unwrap_err();
        assert!(e.to_string().contains("认不出的格式"), "{e}");
    }
}
