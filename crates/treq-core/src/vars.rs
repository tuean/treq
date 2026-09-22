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
    let mut i = 0;
    while i < template.len() {
        if template[i..].starts_with("{{")
            && let Some(close) = template[i + 2..].find("}}")
        {
            let abs_close = i + 2 + close;
            let name = template[i + 2..abs_close].trim();
            let end = abs_close + 2;
            match vars.get(name) {
                Some(val) => {
                    out.push_str(val);
                    i = end;
                    continue;
                }
                None => {
                    out.push_str(&template[i..end]);
                    i = end;
                    continue;
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

/// `{{ 前缀` 补全的候选结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// 候选项（已按前缀过滤，最多 `limit` 个）
    pub items: Vec<String>,
    /// 要替换掉的区间：`{{` 的起点 .. 光标
    pub replace: std::ops::Range<usize>,
}

/// 光标处是否正在写 `{{ 变量` —— 是就给出候选。
/// 规则：`{{` 之后到光标之间不能有 `}}`、换行、另一个 `{`，也不能有除前缀外/首尾外的空格；
/// `caret` 越界会被夹到字符串末尾，非字符边界一律返回 None（不 panic）。
pub fn complete_at(
    content: &str,
    caret: usize,
    names: &[String],
    limit: usize,
) -> Option<Completion> {
    // 先夹到字符串末尾再看是不是字符边界（越界的 caret 当成末尾，非边界直接不补全）
    let caret = caret.min(content.len());
    if !content.is_char_boundary(caret) {
        return None;
    }
    let before = &content[..caret];
    let open = before.rfind("{{")?;
    let after = &before[open + 2..];
    if after.contains("}}") || after.contains('\n') || after.contains('{') {
        return None;
    }
    let partial = after.trim_start();
    if partial.contains(char::is_whitespace) {
        return None;
    }
    let lower = partial.to_lowercase();
    let mut items: Vec<String> = names
        .iter()
        .filter(|n| n.to_lowercase().starts_with(&lower))
        .cloned()
        .collect();
    if items.is_empty() {
        return None;
    }
    items.truncate(limit.max(1));
    Some(Completion {
        items,
        replace: open..caret,
    })
}

/// 用 `{{ 变量名 }}` 替换掉补全区间的文本，返回（新内容, 新光标位置）。
pub fn apply_completion(
    content: &str,
    replace: &std::ops::Range<usize>,
    name: &str,
) -> (String, usize) {
    let start = replace.start.min(content.len());
    let end = replace.end.min(content.len()).max(start);
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return (content.to_string(), replace.end.min(content.len()));
    }
    let text = format!("{{{{ {} }}}}", name);
    let new = format!("{}{}{}", &content[..start], text, &content[end..]);
    let caret = start + text.len();
    (new, caret)
}

/// 从响应正文里按 JSONPath 取一个值，准备存进环境变量。
/// 返回（值, 命中数）：命中多个（通配符）时取第一个，命中数给界面提示用。
/// 值统一成字符串：字符串原样、数字/布尔按字面、数组/对象转紧凑 JSON（null 报错）。
pub fn value_from_json(body: &[u8], path: &str) -> Result<(String, usize), String> {
    let text =
        std::str::from_utf8(body).map_err(|_| "响应不是文本（没法按 JSON 取值）".to_string())?;
    let root: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("响应不是合法 JSON：{e}"))?;
    let hits = crate::json::query_path(&root, path)?;
    let first = hits
        .first()
        .ok_or_else(|| "路径没有匹配到任何值".to_string())?;
    let value = match first {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => return Err("匹配到的值是 null".to_string()),
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    Ok((value, hits.len()))
}

/// 从响应头里取值（名字大小写不敏感，同名多个按 HTTP 语义用 `, ` 连接）。
pub fn value_from_header(headers: &[(String, String)], name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("头名字不能为空".to_string());
    }
    let values: Vec<&str> = headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .collect();
    if values.is_empty() {
        return Err(format!("响应里没有这个头：{name}"));
    }
    Ok(values.join(", "))
}

/// 猜一个变量名：取路径最后一段（`$.data.token` → `token`），去掉下标与通配符，
/// 小写化；猜不出来就用 `value`。
pub fn var_name_from_path(path: &str) -> String {
    let mut last = "";
    for seg in path.split(['.', '[', ']', '"', '\'']) {
        let seg = seg.trim();
        if seg.is_empty() || seg == "$" || seg == "*" || seg.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        last = seg;
    }
    let name = last.trim().to_lowercase();
    if name.is_empty() {
        "value".to_string()
    } else {
        name
    }
}

/// 变量名看着像密钥吗？（面板里默认打码：token/secret/password/api key/cookie 这类）
pub fn looks_secret(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "passwd",
        "pwd",
        "apikey",
        "api_key",
        "api-key",
        "key",
        "auth",
        "credential",
        "cookie",
        "session",
        "sign",
        "signature",
        "private",
    ]
    .iter()
    .any(|pat| n.contains(pat))
}

/// 收集模板里出现的所有变量名（`{{ name }}`，允许内部空格），保持出现顺序、去重。
pub fn collect_vars(template: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < template.len() {
        if template[i..].starts_with("{{")
            && let Some(close) = template[i + 2..].find("}}")
        {
            let abs_close = i + 2 + close;
            let name = template[i + 2..abs_close].trim();
            if !name.is_empty() && !out.iter().any(|x| x == name) {
                out.push(name.to_string());
            }
            i = abs_close + 2;
            continue;
        }
        let ch = template[i..].chars().next().unwrap();
        i += ch.len_utf8();
    }
    out
}

/// 当前请求用到、但环境里没有值的变量名（发送前提醒用）。
/// 顺序即出现顺序：URL → 参数 → 头 → 认证 → 请求体。
pub fn undefined_vars(req: &RequestItem, vars: &BTreeMap<String, String>) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    let mut add = |text: &str| {
        for name in collect_vars(text) {
            if !all.contains(&name) {
                all.push(name);
            }
        }
    };
    add(&req.url);
    for kv in req.params.iter().chain(req.headers.iter()) {
        add(&kv.key);
        add(&kv.value);
    }
    if let Some(a) = &req.auth {
        for f in treq_core_auth_fields(a) {
            add(&f);
        }
    }
    add(&req.body.content);
    for f in &req.body.form_data {
        add(&f.key);
        add(&f.value);
    }
    all.retain(|n| !vars.contains_key(n));
    all
}

/// 认证里参与解析的模板字段（和 `Auth::resolve` 保持一致）。
fn treq_core_auth_fields(a: &crate::auth::Auth) -> Vec<String> {
    match a {
        crate::auth::Auth::None => vec![],
        crate::auth::Auth::Bearer { token, prefix } => vec![token.clone(), prefix.clone()],
        crate::auth::Auth::Basic { username, password } => vec![username.clone(), password.clone()],
        crate::auth::Auth::ApiKey { key, value, .. } => vec![key.clone(), value.clone()],
    }
}

pub fn resolve_request(req: &RequestItem, vars: &BTreeMap<String, String>) -> RequestItem {
    let resolve_kv = |kvs: &[Kv]| {
        kvs.iter()
            .map(|kv| Kv {
                key: resolve(&kv.key, vars),
                value: resolve(&kv.value, vars),
                enabled: kv.enabled,
                description: kv.description.clone(),
            })
            .collect::<Vec<_>>()
    };
    RequestItem {
        id: req.id.clone(),
        name: req.name.clone(),
        method: req.method.clone(),
        url: resolve(&req.url, vars),
        params: resolve_kv(&req.params),
        headers: resolve_kv(&req.headers),
        description: req.description.clone(),
        docs_open: req.docs_open,
        // 认证模板也用同一套变量解析：发送时取到的是真值
        auth: req.auth.as_ref().map(|a| a.resolve(vars)),
        body: Body {
            kind: req.body.kind.clone(),
            content: resolve(&req.body.content, vars),
            form_data: req
                .body
                .form_data
                .iter()
                .map(|f| FormField {
                    key: resolve(&f.key, vars),
                    value: resolve(&f.value, vars),
                    enabled: f.enabled,
                    is_file: f.is_file,
                })
                .collect(),
        },
        order: None,
    }
}
pub(crate) fn urlencode(s: &str) -> String {
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

/// 把 enabled 的 query 参数拼到 URL 后面（与发送路径共用一份实现，
/// URL 预览 / 代码生成 / 实际请求三处结果一致）。
pub fn apply_params(url: &str, params: &[Kv]) -> String {
    let enabled: Vec<&Kv> = params
        .iter()
        .filter(|p| p.enabled && !p.key.is_empty())
        .collect();
    if enabled.is_empty() {
        return url.to_string();
    }
    let sep = if url.contains('?') { "&" } else { "?" };
    let q = enabled
        .iter()
        .map(|p| format!("{}={}", urlencode(&p.key), urlencode(&p.value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}{}{}", url, sep, q)
}

/// 解析模板 + 拼参数后的最终请求 URL。
pub fn resolve_url(req: &RequestItem, vars: &BTreeMap<String, String>) -> String {
    apply_params(&resolve(&req.url, vars), &req.params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Auth;
    use crate::models::{Body, BodyKind, Kv, RequestItem};

    fn mkv(k: &str, v: &str) -> Kv {
        Kv {
            key: k.into(),
            value: v.into(),
            enabled: true,
            description: String::new(),
        }
    }

    fn req() -> RequestItem {
        RequestItem {
            id: "r1".into(),
            name: "r".into(),
            method: "GET".into(),
            url: "http://{{ host }}/{{path}}?x={{host}}".into(),
            params: vec![mkv("q", "{{ missing_in_params }}")],
            headers: vec![mkv("X-{{ hdr }}", "{{ token }}")],
            body: Body {
                kind: BodyKind::Json,
                content: "{\"k\": \"{{ body_var }}\"}".into(),
                form_data: vec![],
            },
            description: String::new(),
            docs_open: false,
            auth: Some(Auth::Bearer {
                token: "{{ token }}".into(),
                prefix: String::new(),
            }),
            order: None,
        }
    }

    #[test]
    fn complete_at_finds_prefix_after_open_braces() {
        let names: Vec<String> = ["baseUrl", "page", "token", "服务"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // 刚打完 `{{`，候选项全给
        let c = complete_at("http://{{", 9, &names, 8).unwrap();
        assert_eq!(c.items.len(), 4);
        assert_eq!(c.replace, 7..9);
        // 有前缀就过滤
        let c = complete_at("http://{{ pa", 11, &names, 8).unwrap();
        assert_eq!(c.items, vec!["page".to_string()]);
        assert_eq!(c.replace, 7..11);
        // 前缀里有空格（用户往下写）→ 不再补全
        assert!(complete_at("http://{{ pa ge", 13, &names, 8).is_none());
        // 已经闭合 / 换行 / 又开一个 → 不补全
        assert!(complete_at("{{ page }}", 10, &names, 8).is_none());
        assert!(complete_at("{{ pa\nx", 7, &names, 8).is_none());
        // 又开一个 `{{`：按**最后**一个补全（内层那对）
        let c = complete_at("{{ {{", 5, &names, 8).unwrap();
        assert_eq!(c.replace, 3..5);
        // 没有前缀匹配 → None
        assert!(complete_at("{{ zzz", 6, &names, 8).is_none());
        // 大小写不敏感
        assert_eq!(
            complete_at("{{ BASE", 7, &names, 8).unwrap().items[0],
            "baseUrl"
        );
        // 中文变量名（多字节）
        let c = complete_at("{{ 服", 6, &names, 8).unwrap();
        assert_eq!(c.items, vec!["服务".to_string()]);
        assert_eq!(c.replace, 0..6);
        // 上限
        assert_eq!(complete_at("{{", 2, &names, 2).unwrap().items.len(), 2);
        // caret 越界 / 非字符边界都不炸
        assert!(complete_at("{{ 服", 100, &names, 8).is_some());
        assert!(complete_at("{{ 服", 4, &names, 8).is_none());
    }

    #[test]
    fn apply_completion_replaces_the_open_braces() {
        let names: Vec<String> = vec!["page".into()];
        let c = complete_at("http://h/{{ pa", 14, &names, 8).unwrap();
        let (out, caret) = apply_completion("http://h/{{ pa", &c.replace, "page");
        assert_eq!(out, "http://h/{{ page }}");
        assert_eq!(caret, out.len());
        // 只替换给定区间，光标后面的内容原样保留
        let content = "X{{ paY";
        let (out, caret) = apply_completion(content, &(1..6), "page");
        assert_eq!(out, "X{{ page }}Y");
        assert_eq!(caret, 1 + "{{ page }}".len());
        // 中文内容 + 越界区间不 panic
        let (out, _) = apply_completion("{{ 服", &(0..100), "服务");
        assert_eq!(out, "{{ 服务 }}");
    }

    #[test]
    fn extracts_values_from_json_body() {
        let body =
            br#"{"code":0,"data":{"token":"abc-123","n":7,"ok":true,"nil":null,"list":[1,2]}}"#;
        assert_eq!(
            value_from_json(body, "$.data.token").unwrap(),
            ("abc-123".into(), 1)
        );
        assert_eq!(value_from_json(body, "$.data.n").unwrap().0, "7");
        assert_eq!(value_from_json(body, "$.data.ok").unwrap().0, "true");
        assert_eq!(value_from_json(body, "$.data.list").unwrap().0, "[1,2]");
        // 不带 $ 前缀也能用（沿用 core 里 JSONPath 的宽容度）
        assert_eq!(value_from_json(body, "data.token").unwrap().0, "abc-123");
        // 通配符命中多个：取第一个，命中数报出来
        assert_eq!(
            value_from_json(body, "$.data.list[*]").unwrap(),
            ("1".into(), 2)
        );
        // 各种失败都有中文原因
        assert!(
            value_from_json(body, "$.data.nope")
                .unwrap_err()
                .contains("没有匹配")
        );
        assert!(
            value_from_json(body, "$.data.nil")
                .unwrap_err()
                .contains("null")
        );
        assert!(
            value_from_json(b"not json", "$")
                .unwrap_err()
                .contains("合法 JSON")
        );
        assert!(
            value_from_json(&[0xff, 0xfe], "$")
                .unwrap_err()
                .contains("不是文本")
        );
        assert!(value_from_json(body, "  ").unwrap_err().contains("空路径"));
    }

    #[test]
    fn extracts_values_from_headers() {
        let headers = vec![
            ("Set-Cookie".to_string(), "a=1".to_string()),
            ("X-Token".to_string(), "t-1".to_string()),
            ("X-Token".to_string(), "t-2".to_string()),
        ];
        assert_eq!(value_from_header(&headers, "x-token").unwrap(), "t-1, t-2");
        assert_eq!(value_from_header(&headers, "SET-COOKIE").unwrap(), "a=1");
        assert!(
            value_from_header(&headers, "X-None")
                .unwrap_err()
                .contains("没有这个头")
        );
        assert!(
            value_from_header(&headers, " ")
                .unwrap_err()
                .contains("不能为空")
        );
    }

    #[test]
    fn guesses_var_names_from_paths() {
        assert_eq!(var_name_from_path("$.data.token"), "token");
        assert_eq!(var_name_from_path("data.accessToken"), "accesstoken");
        assert_eq!(var_name_from_path("$[0].id"), "id");
        assert_eq!(var_name_from_path("$"), "value");
        assert_eq!(var_name_from_path("$.list[*]"), "list");
        assert_eq!(var_name_from_path("X-Request-Id"), "x-request-id");
    }

    #[test]
    fn looks_secret_matches_common_names() {
        for yes in [
            "token",
            "Token",
            "access_token",
            "API_KEY",
            "api-key",
            "secretKey",
            "password",
            "PWD",
            "Cookie",
            "sign",
            "privateKey",
        ] {
            assert!(looks_secret(yes), "{yes} 应该算敏感");
        }
        for no in ["baseUrl", "host", "userId", "page", "环境", "path"] {
            assert!(!looks_secret(no), "{no} 不应算敏感");
        }
    }

    #[test]
    fn collect_vars_keeps_order_and_dedupes() {
        assert_eq!(
            collect_vars("{{ a }}/{{b}}/{{ a }}"),
            vec!["a".to_string(), "b".to_string()]
        );
        // 中文变量名、空模板、未闭合都安全
        assert_eq!(collect_vars("{{ 服务 }}"), vec!["服务".to_string()]);
        assert_eq!(collect_vars("{{ }}"), Vec::<String>::new());
        assert_eq!(collect_vars("{{ 没闭合"), Vec::<String>::new());
    }

    #[test]
    fn undefined_vars_reports_only_missing_ones() {
        let mut vars = BTreeMap::new();
        vars.insert("host".to_string(), "h".to_string());
        vars.insert("token".to_string(), "t".to_string());
        vars.insert("path".to_string(), "p".to_string());
        vars.insert("hdr".to_string(), "H".to_string());
        let missing = undefined_vars(&req(), &vars);
        assert_eq!(
            missing,
            vec!["missing_in_params".to_string(), "body_var".to_string()]
        );
    }

    #[test]
    fn undefined_vars_empty_when_all_set() {
        let mut vars = BTreeMap::new();
        for k in [
            "host",
            "path",
            "token",
            "hdr",
            "missing_in_params",
            "body_var",
        ] {
            vars.insert(k.to_string(), "v".to_string());
        }
        assert!(undefined_vars(&req(), &vars).is_empty());
    }

    #[test]
    fn undefined_vars_scans_manual_headers_too() {
        // 手写的密钥头（不走认证设置）同样要能提醒
        let mut r = req();
        r.auth = Some(Auth::None);
        r.headers = vec![mkv("X-Api-Key", "{{ api_key }}")];
        let missing = undefined_vars(&r, &BTreeMap::new());
        assert!(missing.contains(&"api_key".to_string()), "{missing:?}");
        assert!(missing.contains(&"host".to_string()), "{missing:?}");
    }
}
