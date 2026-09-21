//! JSON 语法着色 tokenizer：把一行 JSON 切成着色段。
//! 字节扫描实现，不依赖正则；字符串内处理 \ 转义。

use gpui::Rgba;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JsonToken<'a> {
    pub text: &'a str,
    pub kind: TokKind,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TokKind {
    Str,   // "..." 值字符串
    Key,   // "key" 键（含后随冒号）
    Num,   // 数字
    Const, // true/false/null
    Punct, // 标点
    Space, // 空白
}

pub fn token_color(
    kind: TokKind,
    fg: Rgba,
    string: Rgba,
    key: Rgba,
    num: Rgba,
    konst: Rgba,
) -> Rgba {
    match kind {
        TokKind::Str => string,
        TokKind::Key => key,
        TokKind::Num => num,
        TokKind::Const => konst,
        _ => fg,
    }
}

/// 把一行 JSON 切成 tokens。
pub fn tokenize_line(line: &str) -> Vec<JsonToken<'_>> {
    let mut out = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' => {
                let start = i;
                while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
                    i += 1;
                }
                out.push(JsonToken {
                    text: &line[start..i],
                    kind: TokKind::Space,
                });
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                // 键判定：字符串后（忽略空白）紧跟冒号
                let mut j = i;
                while j < b.len() && b[j] == b' ' {
                    j += 1;
                }
                let has_colon = j < b.len() && b[j] == b':';
                let (kind, text_end, next) = if has_colon {
                    // key token 包含冒号，i 推进到冒号后（跳过），冒号不再单独成 token
                    (TokKind::Key, j + 1, j + 1)
                } else {
                    (TokKind::Str, i, i)
                };
                out.push(JsonToken {
                    text: &line[start..text_end],
                    kind,
                });
                i = next;
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while i < b.len() && matches!(b[i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
                {
                    i += 1;
                }
                out.push(JsonToken {
                    text: &line[start..i],
                    kind: TokKind::Num,
                });
            }
            _ => {
                // 字面量 / 标点
                let rest = &line[i..];
                let word_end = rest
                    .find(|c: char| {
                        c == ' ' || c == ',' || c == ']' || c == '}' || c == '\n' || c == '\t'
                    })
                    .unwrap_or(rest.len());
                let word_end = if word_end > 0 { word_end } else { 1 };
                let word = &rest[..word_end];
                let kind = match word {
                    "true" | "false" | "null" => TokKind::Const,
                    _ => TokKind::Punct,
                };
                out.push(JsonToken { text: word, kind });
                i += word_end;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_key_value() {
        let toks = tokenize_line("  \"name\": \"treq\",");
        assert_eq!(toks[0].kind, TokKind::Space);
        assert_eq!(toks[1].kind, TokKind::Key);
        assert_eq!(toks[1].text, "\"name\":");
        assert_eq!(toks[3].kind, TokKind::Str);
        assert_eq!(toks[3].text, "\"treq\""); // 冒号后的字符串：字符串扫描不含跳过空格逻辑……
    }

    #[test]
    fn tokenize_escaped_string() {
        let toks = tokenize_line("\"a\\\"b\": 1");
        assert_eq!(toks[0].kind, TokKind::Key);
        assert_eq!(toks[0].text, "\"a\\\"b\":");
        assert_eq!(toks[2].kind, TokKind::Num);
        assert_eq!(toks[2].text, "1");
    }

    #[test]
    fn tokenize_constants() {
        let toks = tokenize_line("true, false, null");
        assert_eq!(toks[0].kind, TokKind::Const);
        assert_eq!(toks[1].kind, TokKind::Punct);
        assert_eq!(toks[2].kind, TokKind::Space);
        assert_eq!(toks[3].kind, TokKind::Const);
    }

    #[test]
    fn tokenize_punctuation_len() {
        // 标点必须至少 1 字符，不得产生空 token
        let toks = tokenize_line("{");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokKind::Punct);
        assert!(!toks[0].text.is_empty());
    }
}
/// 可编辑状态下的一行 JSON → gpui `TextRun`（编辑器着色，与响应区同一套配色）。
///
/// runs 必须覆盖整行（长度和 = 行字节数），空行用零长 run 占位。
pub fn editor_runs(line: &str, base: &gpui::TextRun) -> Vec<gpui::TextRun> {
    let mut runs: Vec<gpui::TextRun> = Vec::new();
    // 基准色是 Hsla（跟随编辑器主题），转成 Rgba 只是为了跟其它 token 色保持同一入口
    let base_rgba: Rgba = base.color.into();
    for tok in tokenize_line(line) {
        runs.push(gpui::TextRun {
            len: tok.text.len(),
            color: token_color(
                tok.kind,
                base_rgba,
                crate::theme::json_string(),
                crate::theme::json_key(),
                crate::theme::json_num(),
                crate::theme::json_const(),
            )
            .into(),
            ..base.clone()
        });
    }
    if runs.is_empty() {
        runs.push(gpui::TextRun {
            len: 0,
            ..base.clone()
        });
    }
    runs
}

/// 一行 JSON → 逐 token 着色、按字节铺满整行的 runs（供正文渲染）。
pub fn line_runs(line: &str) -> Vec<gpui::TextRun> {
    let mut runs: Vec<gpui::TextRun> = Vec::new();
    for tok in tokenize_line(line) {
        runs.push(gpui::TextRun {
            len: tok.text.len(),
            font: crate::theme::mono(),
            color: token_color(
                tok.kind,
                crate::theme::fg_normal(),
                crate::theme::json_string(),
                crate::theme::json_key(),
                crate::theme::json_num(),
                crate::theme::json_const(),
            )
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    runs
}

/// 选中高亮：把 [from, to) 范围内的 run 片段拆出来加上背景色。
///
/// runs 按字节长度铺满整行，所以直接按字节游标切；切完长度和不变（否则文字会缺/多）。
pub fn apply_selection(
    runs: &mut Vec<gpui::TextRun>,
    selection: Option<(usize, usize)>,
    color: Rgba,
) {
    let Some((from, to)) = selection else {
        return;
    };
    if from >= to {
        return;
    }
    let mut out: Vec<gpui::TextRun> = Vec::with_capacity(runs.len() + 2);
    let mut pos = 0usize;
    for run in runs.drain(..) {
        let start = pos;
        let end = pos + run.len;
        pos = end;
        // 本 run 与选中区的交
        let sel_from = from.max(start);
        let sel_to = to.min(end);
        if sel_from >= sel_to {
            out.push(run);
            continue;
        }
        if sel_from > start {
            out.push(gpui::TextRun {
                len: sel_from - start,
                ..run.clone()
            });
        }
        out.push(gpui::TextRun {
            len: sel_to - sel_from,
            background_color: Some(color.into()),
            ..run.clone()
        });
        if sel_to < end {
            out.push(gpui::TextRun {
                len: end - sel_to,
                ..run
            });
        }
    }
    // 相邻且颜色/背景一样的片段合并：不合并时相邻背景块可能画出 1px 缝
    let mut merged: Vec<gpui::TextRun> = Vec::with_capacity(out.len());
    for r in out {
        match merged.last_mut() {
            Some(prev)
                if prev.color == r.color
                    && prev.background_color == r.background_color
                    && prev.font == r.font
                    && prev.underline.is_none()
                    && r.underline.is_none()
                    && prev.strikethrough.is_none()
                    && r.strikethrough.is_none() =>
            {
                prev.len += r.len;
            }
            _ => merged.push(r),
        }
    }
    *runs = merged;
}

/// 响应体要显示的行 + 是否 JSON 着色 + 提示文案。
///
/// 抽成纯函数：既方便单测（过滤报错/非 JSON/空结果），也让 AppModel 里只留缓存外壳。
/// 单次渲染的正文上限：超过就跳过渲染（由 UI 给「完整」开关强行加载）。
/// 为什么要有：10 MB JSON 实测 body_lines 要 908 ms 且在 UI 线程，
/// 还会产出 72 万行 String（几十 MB 堆）；流式响应另有 2 MB 接收上限。
pub const MAX_RENDER_BYTES: usize = 2 * 1024 * 1024;

/// 一行里的「简单值」——`"key": "value",` 取 `value`；数组元素行 `  "x",` 取 `x`。
///
/// 只认标量：带 `{`/`[` 的行是复合值（对象/数组），返回 None，界面就不给「复制这一行的值」。
/// 键名也在 Punct/Key token 里，所以扫描到的**第一个**值 token 就是值本身。
pub fn line_scalar(line: &str) -> Option<String> {
    let mut value: Option<&str> = None;
    for tok in tokenize_line(line) {
        match tok.kind {
            TokKind::Punct if tok.text.contains(['{', '[', '}', ']']) => return None,
            TokKind::Str | TokKind::Num | TokKind::Const if value.is_none() => {
                value = Some(tok.text);
            }
            _ => {}
        }
    }
    let v = value?.trim();
    let unquoted = v
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .map(unescape)
        .unwrap_or_else(|| v.to_string());
    (!unquoted.is_empty()).then_some(unquoted)
}

/// JSON 字符串里的常见转义还原（复制出来是给人用的，不是给机器解析的）。
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// 正文是否超过渲染上限（UI 据此决定要不要给「完整」开关）。
pub fn over_render_limit(body: &[u8]) -> bool {
    body.len() > MAX_RENDER_BYTES
}

pub fn body_lines(
    body: &[u8],
    content_type: Option<&str>,
    filter: &str,
    pretty: bool,
    force_full: bool,
    t: &dyn Fn(&str) -> String,
) -> (Vec<String>, bool, Option<String>) {
    // 显式过滤是用户主动操作，不套上限；否则大响应先跳过渲染
    if !force_full && filter.trim().is_empty() && over_render_limit(body) {
        let mb = body.len() as f64 / 1048576.0;
        return (
            Vec::new(),
            false,
            Some(format!(
                "{} · {:.1} MB · {}",
                t("response.too_big"),
                mb,
                t("response.too_big_hint")
            )),
        );
    }
    let text = String::from_utf8_lossy(body).to_string();
    let filter = filter.trim();
    if !filter.is_empty() {
        return match serde_json::from_str::<serde_json::Value>(&text) {
            Err(_) => (Vec::new(), false, Some(t("response.filter.not_json"))),
            Ok(v) => match treq_core::json::query_path(&v, filter) {
                Err(e) => (
                    Vec::new(),
                    false,
                    Some(format!("{}: {}", t("response.filter.bad"), e)),
                ),
                Ok(hits) if hits.is_empty() => {
                    (Vec::new(), false, Some(t("response.filter.empty")))
                }
                Ok(hits) => {
                    let arr = serde_json::Value::Array(hits.into_iter().cloned().collect());
                    let pretty_text = serde_json::to_string_pretty(&arr).unwrap_or_default();
                    (
                        pretty_text.lines().map(|l| l.to_string()).collect(),
                        true,
                        None,
                    )
                }
            },
        };
    }
    if pretty
        && treq_core::json::looks_json(content_type, body)
        && let Some(pretty_text) = treq_core::json::pretty_json(body)
    {
        return (
            pretty_text.lines().map(|l| l.to_string()).collect(),
            true,
            None,
        );
    }
    (text.lines().map(|l| l.to_string()).collect(), false, None)
}

#[cfg(test)]
mod scalar_tests {
    use super::line_scalar;

    #[test]
    fn picks_the_value_not_the_key() {
        assert_eq!(line_scalar(r#"  "token": "abc","#).as_deref(), Some("abc"));
        assert_eq!(line_scalar(r#"  "n": 42,"#).as_deref(), Some("42"));
        assert_eq!(line_scalar(r#"    true,"#).as_deref(), Some("true"));
        assert_eq!(line_scalar(r#"  "a": "b:c","#).as_deref(), Some("b:c"));
        // 中文键值、转义
        assert_eq!(
            line_scalar(r#"  "名单": "张三\n李四","#).as_deref(),
            Some("张三\n李四")
        );
    }

    #[test]
    fn compound_and_empty_lines_have_no_scalar() {
        assert_eq!(line_scalar(r#"  "a": {"b": 1},"#), None);
        assert_eq!(line_scalar(r#"  "a": [1, 2],"#), None);
        assert_eq!(line_scalar("  {"), None);
        assert_eq!(line_scalar("  },"), None);
        assert_eq!(line_scalar(""), None);
    }

    #[test]
    fn key_only_line_has_no_scalar() {
        assert_eq!(line_scalar(r#"  "a": {"#), None);
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;

    fn base() -> gpui::TextRun {
        gpui::TextRun {
            len: 0,
            font: crate::theme::mono(),
            color: crate::theme::fg_normal().into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn runs_cover_the_whole_line() {
        for line in [
            "  \"name\": \"treq\",",
            "{\"a\":1}",
            "true",
            "   ",
            "",
            "\"k\": [1, 2.5e3, null]",
            "中文键\": \"值\"",
        ] {
            let runs = editor_runs(line, &base());
            let sum: usize = runs.iter().map(|r| r.len).sum();
            assert_eq!(sum, line.len(), "runs 必须覆盖整行: {line:?} {runs:?}");
            assert!(!runs.is_empty());
        }
    }

    #[test]
    fn editor_runs_color_keys_and_strings_differently() {
        let runs = editor_runs("\"k\": \"v\"", &base());
        let key = &runs[0];
        let value = runs.iter().find(|r| r.len == 3).unwrap();
        assert_eq!(key.color, crate::theme::json_key().into());
        assert_eq!(value.color, crate::theme::json_string().into());
    }
}

#[cfg(test)]
mod body_tests {
    use super::*;

    fn t(k: &str) -> String {
        k.to_string()
    }

    #[test]
    fn pretty_json_is_split_into_lines() {
        let (lines, json, notice) = body_lines(
            b"{\"a\":1,\"b\":[1,2]}",
            Some("application/json"),
            "",
            true,
            false,
            &t,
        );
        assert!(json);
        assert!(notice.is_none());
        assert!(lines.len() >= 3, "美化后应多行: {lines:?}");
        assert!(lines[0].starts_with('{'));
    }

    #[test]
    fn raw_mode_keeps_original_lines() {
        let (lines, json, _) =
            body_lines(b"{\"a\":1}", Some("application/json"), "", false, false, &t);
        assert!(!json);
        assert_eq!(lines, vec!["{\"a\":1}"]);
    }

    #[test]
    fn filter_selects_subset_and_reports_errors() {
        let body = br#"{"data":{"list":[{"id":1},{"id":2}]}}"#;
        let (lines, json, notice) =
            body_lines(body, Some("application/json"), "data.list", true, false, &t);
        assert!(json && notice.is_none());
        assert!(lines.join("").contains("\"id\""));
        // 空结果
        let (lines, _, notice) =
            body_lines(body, Some("application/json"), "data.nope", true, false, &t);
        assert!(lines.is_empty());
        assert_eq!(notice.as_deref(), Some("response.filter.empty"));
        // 非法路径
        let (_, _, notice) = body_lines(body, Some("application/json"), "data[", true, false, &t);
        assert!(notice.unwrap().starts_with("response.filter.bad"));
        // 非 JSON 响应
        let (_, _, notice) = body_lines(b"not json", None, "data", true, false, &t);
        assert_eq!(notice.as_deref(), Some("response.filter.not_json"));
    }
}

#[cfg(test)]
mod size_guard_tests {
    use super::*;

    /// 3 MB 纯文本正文（非 JSON，走「原始」路径，测试跑得快）
    fn big_text() -> String {
        "一行有点内容的正文\n".repeat(200_000)
    }

    #[test]
    fn over_render_limit_only_for_huge_bodies() {
        assert!(!over_render_limit(b"[]"));
        assert!(!over_render_limit(&vec![b'x'; MAX_RENDER_BYTES]));
        assert!(over_render_limit(&vec![b'x'; MAX_RENDER_BYTES + 1]));
    }

    #[test]
    fn huge_body_skips_render_until_forced() {
        let body = big_text();
        assert!(over_render_limit(body.as_bytes()));
        let t = |k: &str| k.to_string();
        let (lines, json, notice) =
            body_lines(body.as_bytes(), Some("text/plain"), "", true, false, &t);
        assert!(lines.is_empty(), "超限时不该产出任何行");
        assert!(!json);
        let notice = notice.expect("应给出提示");
        assert!(notice.contains("response.too_big"), "{notice}");
        assert!(notice.contains("MB"), "{notice}");

        // 点「完整」后照旧渲染
        let (lines, _, notice) =
            body_lines(body.as_bytes(), Some("text/plain"), "", true, true, &t);
        assert_eq!(lines.len(), 200_000);
        assert!(notice.is_none());
    }

    #[test]
    fn small_body_renders_normally() {
        let t = |k: &str| k.to_string();
        let (lines, json, notice) =
            body_lines(br#"{"a":1}"#, Some("application/json"), "", true, false, &t);
        assert!(json);
        assert!(lines.iter().any(|l| l.contains("\"a\"")));
        assert!(notice.is_none());
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn run(len: usize) -> gpui::TextRun {
        gpui::TextRun {
            len,
            font: crate::theme::mono(),
            color: crate::theme::fg_normal().into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    fn runs_of(text: &str) -> Vec<gpui::TextRun> {
        vec![run(text.len())]
    }

    fn painted(runs: &[gpui::TextRun], text: &str) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        let mut pos = 0;
        for r in runs {
            let seg = text[pos..pos + r.len].to_string();
            out.push((seg, r.background_color.is_some()));
            pos += r.len;
        }
        out
    }

    #[test]
    fn splits_runs_and_paints_only_the_selection() {
        let text = "  \"a\": 42,";
        let mut runs = runs_of(text);
        apply_selection(&mut runs, Some((2, 10)), crate::theme::selection());
        assert_eq!(
            painted(&runs, text),
            vec![
                ("  ".to_string(), false),
                ("\"a\": 42,".to_string(), true),
            ]
        );
        // runs 长度和必须仍等于整行（不然渲染会缺字或多字）
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len());
    }

    #[test]
    fn no_selection_and_empty_selection_leave_runs_alone() {
        let text = "abc";
        let mut runs = runs_of(text);
        apply_selection(&mut runs, None, crate::theme::selection());
        assert_eq!(runs.len(), 1);
        assert!(runs[0].background_color.is_none());
        let mut runs = runs_of(text);
        apply_selection(&mut runs, Some((1, 1)), crate::theme::selection());
        assert_eq!(runs.len(), 1, "空选中不动 runs");
    }

    #[test]
    fn selection_across_several_runs_keeps_order() {
        // 两个 run：`"key"` 与 `: 42`（模拟 JSON 着色后的相邻 run）
        let text = "\"key\": 42";
        let mut runs = vec![run(5), run(4)];
        apply_selection(&mut runs, Some((2, 8)), crate::theme::selection());
        assert_eq!(
            painted(&runs, text),
            vec![
                ("\"k".to_string(), false),
                ("ey\": 4".to_string(), true),
                ("2".to_string(), false),
            ]
        );
    }
}

#[cfg(test)]
mod coverage_tests {
    /// runs 必须铺满整行：少一个字节，选中高亮和折行都会错位
    #[test]
    fn line_runs_cover_the_whole_line() {
        for line in [
            r#"  "method": "POST","#,
            r#"  { "#,
            "  }",
            r#"    "body": "{\"x\":  1}","#,
            "plain text 中文",
            "",
        ] {
            let n: usize = super::line_runs(line).iter().map(|r| r.len).sum();
            assert_eq!(n, line.len(), "runs 长度和 != 行长度: {line:?}");
        }
    }
}
