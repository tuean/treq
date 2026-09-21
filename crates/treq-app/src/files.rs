//! 响应保存相关的纯逻辑（不依赖 gpui，便于单测）。

/// 保存响应用的默认文件名：`<请求名>-<时间戳>.<按 Content-Type 猜的扩展名>`。
/// 名字里的 `/`、`:`、空格等会换成 `-`（macOS 文件名不允许 `/` 和 `:`）。
pub fn suggested_file_name(request_name: &str, content_type: &str, body: &[u8]) -> String {
    let mut base = String::new();
    for ch in request_name.chars().take(60) {
        // 中日韩字符留原样，其余非字母数字一律换成 '-'（避免 / : 空格 等）
        if ch.is_alphanumeric() || (ch as u32) > 0x2e80 {
            base.push(ch);
        } else if !base.ends_with('-') {
            base.push('-');
        }
    }
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "response" } else { base };
    let ct = content_type.to_ascii_lowercase();
    // 注意：looks_json 见到「显式非 json 的 content-type」会直接返回 false，
    // 所以没有 content-type 时传 None，让它按正文形状嗅探。
    let sniff_ct = if ct.is_empty() {
        None
    } else {
        Some(ct.as_str())
    };
    let ext = if ct.contains("json") || treq_core::json::looks_json(sniff_ct, body) {
        "json"
    } else if ct.contains("html") {
        "html"
    } else if ct.contains("xml") {
        "xml"
    } else if ct.contains("csv") {
        "csv"
    } else if ct.contains("javascript") {
        "js"
    } else if ct.contains("event-stream") {
        "sse.txt"
    } else {
        "txt"
    };
    format!("{}-{}.{}", base, treq_core::backup::stamp(), ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_follows_content_type() {
        let j = |ct: &str, b: &[u8]| suggested_file_name("x", ct, b);
        assert!(j("application/json; charset=utf-8", b"{}").ends_with(".json"));
        assert!(j("text/html", b"<html>").ends_with(".html"));
        assert!(j("application/xml", b"<a/>").ends_with(".xml"));
        assert!(j("text/csv", b"a,b").ends_with(".csv"));
        assert!(j("application/javascript", b"var a").ends_with(".js"));
        assert!(j("text/event-stream", b"data: 1").ends_with(".sse.txt"));
        assert!(j("", b"\x00\x01\x02").ends_with(".txt"));
        // 没标 Content-Type 但正文是 JSON：也认
        assert!(j("", b"{\"a\": 1}").ends_with(".json"));
    }

    #[test]
    fn name_is_sanitized() {
        let n = suggested_file_name("POST /api/v1/订单: 查询?", "application/json", b"{}");
        assert!(!n.contains('/'), "{n}");
        assert!(!n.contains(':'), "{n}");
        assert!(!n.contains(' '), "{n}");
        assert!(n.starts_with("POST-api-v1-订单-查询-"), "{n}");
        assert!(n.ends_with(".json"), "{n}");
        // 长名字截断；全符号时兜底
        assert!(suggested_file_name(&"a".repeat(200), "", b"").starts_with(&"a".repeat(60)));
        assert!(suggested_file_name("///", "application/json", b"{}").starts_with("response-"));
    }
}
