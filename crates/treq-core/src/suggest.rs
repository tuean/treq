//! KV 输入框的「纯文本补全」候选过滤。
//! 和 vars::complete_at 的模板补全不同：这里输入的不是模板，命中后直接用候选
//! 把输入内容整段替换掉（Key 框敲 content -> Content-Type）。
//! 只做数据层面的过滤；候选从哪来（当前请求 / 历史 / 其他请求）由调用方决定顺序。

/// 按输入前缀过滤候选。
/// 规则：大小写不敏感；前缀命中的排在包含命中的前面（type 也能找到 Content-Type）；
/// 去重（大小写不敏感），跳过与输入完全相同的候选；保持调用方给的顺序（优先级），
/// 最多返回 limit 个；输入为空（trim 后）返回空，避免点开空行就弹一堆。
pub fn filter_prefix(candidates: &[String], input: &str, limit: usize) -> Vec<String> {
    let prefix = input.trim().to_lowercase();
    if prefix.is_empty() {
        return Vec::new();
    }
    let limit = limit.max(1);
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    // 第一轮：前缀命中（content -> Content-Type）
    for c in candidates {
        if out.len() >= limit {
            return out;
        }
        let lower = c.to_lowercase();
        if lower == prefix || seen.iter().any(|s| s.as_str() == lower.as_str()) {
            continue;
        }
        if lower.starts_with(&prefix) {
            seen.push(lower);
            out.push(c.clone());
        }
    }
    // 第二轮：包含命中（type -> Content-Type），优先级低，只在前缀没凑满时补。
    // 输入太短（1 个字符）不做包含匹配，否则敲一个字母会捞出一堆不相关的值。
    for c in candidates {
        if out.len() >= limit {
            break;
        }
        let lower = c.to_lowercase();
        if lower == prefix || seen.iter().any(|s| s.as_str() == lower.as_str()) {
            continue;
        }
        if prefix.chars().count() >= 2 && lower.contains(&prefix) {
            seen.push(lower);
            out.push(c.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_input_never_suggests() {
        assert!(filter_prefix(&v(&["a", "b"]), "", 8).is_empty());
        assert!(filter_prefix(&v(&["a", "b"]), "   ", 8).is_empty());
    }

    #[test]
    fn case_insensitive_prefix_first() {
        let cands = v(&["Content-Type", "X-Content-Type-Options", "Accept"]);
        assert_eq!(
            filter_prefix(&cands, "content", 8),
            v(&["Content-Type", "X-Content-Type-Options"])
        );
    }

    #[test]
    fn contains_fallback_when_prefix_short() {
        let cands = v(&["Authorization", "Content-Type", "X-Request-Type"]);
        // type 不是任何候选的前缀，但应作为包含命中兜底
        assert_eq!(
            filter_prefix(&cands, "type", 8),
            v(&["Content-Type", "X-Request-Type"])
        );
    }

    #[test]
    fn exact_match_and_dupes_are_dropped() {
        let cands = v(&["Accept", "accept", "ACCEPT", "Accept-Encoding"]);
        // 输入已经是 Accept：大小写不敏感地排除自身，只留别的
        assert_eq!(filter_prefix(&cands, "Accept", 8), v(&["Accept-Encoding"]));
        // 去重只留第一个大小写形式
        assert_eq!(
            filter_prefix(&cands, "ac", 8),
            v(&["Accept", "Accept-Encoding"])
        );
    }

    #[test]
    fn limit_is_respected_and_never_zero() {
        let cands = v(&["a1", "a2", "a3"]);
        assert_eq!(filter_prefix(&cands, "a", 2), v(&["a1", "a2"]));
        assert_eq!(filter_prefix(&cands, "a", 0).len(), 1);
    }

    #[test]
    fn trims_input_whitespace() {
        let cands = v(&["Content-Type"]);
        assert_eq!(filter_prefix(&cands, "  cont", 8), v(&["Content-Type"]));
    }
    #[test]
    fn one_char_input_skips_contains_fallback() {
        let cands = v(&["Content-Type", "X-Type"]);
        // 单个字符不做包含兜底，避免敲一个字母捞出一堆不相关的值
        assert!(filter_prefix(&cands, "t", 8).is_empty());
        assert_eq!(
            filter_prefix(&cands, "ty", 8),
            v(&["Content-Type", "X-Type"])
        );
    }
}
