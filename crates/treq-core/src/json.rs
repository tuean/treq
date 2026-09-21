use serde_json::Value;

pub fn pretty_json(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

#[derive(Debug, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
    Wildcard,
}

/// 极简 JSONPath 解析：支持 `$`、`.key`、`["key"]`、`[0]`、`[*]`、`.*`。
/// 不支持过滤器/递归下降/切片——响应面板的筛选条只需要取值路径。
fn parse_path(path: &str) -> Result<Vec<Seg>, String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("空路径".into());
    }
    let rest = p.strip_prefix('$').unwrap_or(p);
    let b = rest.as_bytes();
    let mut segs = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'.' => {
                i += 1;
                if i < b.len() && b[i] == b'*' {
                    segs.push(Seg::Wildcard);
                    i += 1;
                    continue;
                }
                let start = i;
                while i < b.len() && b[i] != b'.' && b[i] != b'[' {
                    i += 1;
                }
                if start == i {
                    return Err("空字段名".into());
                }
                segs.push(Seg::Key(rest[start..i].to_string()));
            }
            b'[' => {
                let start = i + 1;
                let end = rest[start..].find(']').ok_or("缺少 ]")? + start;
                let inner = rest[start..end].trim();
                if inner == "*" {
                    segs.push(Seg::Wildcard);
                } else if let Ok(n) = inner.parse::<usize>() {
                    segs.push(Seg::Index(n));
                } else {
                    let key = inner.trim_matches(|c| c == '"' || c == '\'');
                    if key.is_empty() {
                        return Err("空下标".into());
                    }
                    segs.push(Seg::Key(key.to_string()));
                }
                i = end + 1;
            }
            _ => {
                let start = i;
                while i < b.len() && b[i] != b'.' && b[i] != b'[' {
                    i += 1;
                }
                segs.push(Seg::Key(rest[start..i].to_string()));
            }
        }
    }
    Ok(segs)
}

/// 按路径取值，返回全部命中节点（`[*]` 会展开）；路径非法返回 Err。
pub fn query_path<'a>(root: &'a Value, path: &str) -> Result<Vec<&'a Value>, String> {
    let segs = parse_path(path)?;
    let mut cur = vec![root];
    for seg in segs {
        let mut next: Vec<&Value> = Vec::new();
        for v in cur {
            match &seg {
                Seg::Key(k) => {
                    if let Some(x) = v.get(k) {
                        next.push(x);
                    }
                }
                Seg::Index(i) => {
                    if let Some(x) = v.get(*i) {
                        next.push(x);
                    }
                }
                Seg::Wildcard => match v {
                    Value::Array(a) => next.extend(a.iter()),
                    Value::Object(o) => next.extend(o.values()),
                    _ => {}
                },
            }
        }
        cur = next;
    }
    Ok(cur)
}

/// 按「值」反查它在文档里的路径（响应里看到某个 token，想知道怎么引用它）。
///
/// 深度优先、按文档顺序找**第一个**匹配的标量：字符串比原文，数字/布尔/null 比字面量。
/// 只认标量 —— 拿一个对象去反查没有意义，也会撞车。
pub fn find_path_by_value(root: &Value, needle: &str) -> Option<String> {
    fn walk(v: &Value, path: &str, needle: &str) -> Option<String> {
        match v {
            Value::Object(map) => {
                for (k, child) in map {
                    let p = format!("{}{}", path, seg_for_key(k));
                    if let Some(hit) = walk(child, &p, needle) {
                        return Some(hit);
                    }
                }
                None
            }
            Value::Array(items) => {
                for (i, child) in items.iter().enumerate() {
                    let p = format!("{}[{}]", path, i);
                    if let Some(hit) = walk(child, &p, needle) {
                        return Some(hit);
                    }
                }
                None
            }
            Value::String(s) => (s == needle).then(|| path.to_string()),
            Value::Number(n) => (n.to_string() == needle).then(|| path.to_string()),
            Value::Bool(b) => (b.to_string() == needle).then(|| path.to_string()),
            Value::Null => (needle == "null").then(|| path.to_string()),
        }
    }
    walk(root, "$", needle)
}

/// `$.key` 还是 `$["奇怪的 key"]`（键里有 `.`/`[`/引号/空白就得用括号写法）。
fn seg_for_key(k: &str) -> String {
    let plain = !k.is_empty()
        && !k.contains(['.', '[', ']', '"', '\\'])
        && !k.chars().any(|c| c.is_whitespace());
    if plain {
        format!(".{}", k)
    } else {
        format!("[\"{}\"]", k.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

pub fn looks_json(content_type: Option<&str>, body: &[u8]) -> bool {
    if let Some(ct) = content_type {
        // 有显式 content-type：信任它（text/plain 即使内容是 JSON 也按原文显示）
        return ct.to_ascii_lowercase().contains("json");
    }
    // 无 content-type：按 body 形状嗅探
    let t = String::from_utf8_lossy(body);
    let t = t.trim_start();
    t.starts_with('{') || t.starts_with('[')
}
