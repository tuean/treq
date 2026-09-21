//! 代码框的语法着色：注释 / 字符串 / 数字 / 关键字四类，与具体语言无关。

use gpui::Rgba;

/// 生成代码的高亮：注释 / 字符串 / 数字 / 关键字四类，与具体语言无关（够用即可）。
/// 返回每行的 (文本, 颜色) 片段；片段拼起来必须等于原行。
pub fn code_spans(line: &str) -> Vec<(String, Rgba)> {
    const KEYWORDS: &[&str] = &[
        // Java / JS / TS / Go / Python / Shell 常见词，一个表覆盖所有目标语言
        "public", "private", "static", "class", "void", "new", "return", "import", "package",
        "func", "function", "const", "let", "var", "async", "await", "if", "else", "for", "def",
        "curl", "true", "false", "null", "nil", "try", "catch", "String", "int", "var",
    ];
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<(String, Rgba)> = Vec::new();
    let mut plain = String::new();
    let flush = |out: &mut Vec<(String, Rgba)>, plain: &mut String| {
        if !plain.is_empty() {
            out.push((std::mem::take(plain), crate::theme::fg_normal()));
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // 行注释：// 和 #（Shell/Python）
        if (c == '/' && chars.get(i + 1) == Some(&'/')) || c == '#' {
            flush(&mut out, &mut plain);
            out.push((chars[i..].iter().collect(), crate::theme::fg_dark()));
            return out;
        }
        // 字符串（含转义），单双引号都算
        if c == '"' || c == '\'' {
            flush(&mut out, &mut plain);
            let mut s = String::from(c);
            i += 1;
            while i < chars.len() {
                s.push(chars[i]);
                if chars[i] == '\\' {
                    i += 1;
                    if i < chars.len() {
                        s.push(chars[i]);
                    }
                } else if chars[i] == c {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push((s, crate::theme::json_string()));
            continue;
        }
        // 数字：前面不是字母数字才算（避免把 url1、base64 切碎）
        if c.is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            flush(&mut out, &mut plain);
            let mut s = String::new();
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
            {
                s.push(chars[i]);
                i += 1;
            }
            out.push((s, crate::theme::json_num()));
            continue;
        }
        // 标识符：命中关键字表就单独上色
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if KEYWORDS.contains(&word.as_str()) {
                flush(&mut out, &mut plain);
                out.push((word, crate::theme::json_const()));
            } else {
                plain.push_str(&word);
            }
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut out, &mut plain);
    if out.is_empty() {
        out.push((String::new(), crate::theme::fg_normal()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_spans_reproduce_the_line() {
        for line in [
            "    String url = \"http://x/y?a=1\"; // 注释",
            "\tcurl -X POST 'http://h/p' -H \"A: 1\"",
            "# python 注释",
            "const n = 42; base64url1",
            "if (a) { return null; }",
            "",
            "   ",
            "\"先字符串后注释\" // 尾注释",
        ] {
            let joined: String = code_spans(line).iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(joined, line, "片段拼起来必须等于原行: {line:?}");
        }
    }

    #[test]
    fn code_spans_colorize_tokens() {
        let spans = code_spans("String a = \"x\"; // hi");
        let color_of = |want: &str| {
            spans
                .iter()
                .find(|(t, _)| t == want)
                .map(|(_, c)| *c)
        };
        assert_eq!(color_of("\"x\""), Some(crate::theme::json_string()));
        assert_eq!(color_of("// hi"), Some(crate::theme::fg_dark()));
        assert_eq!(color_of("String"), Some(crate::theme::json_const()));
        // 数字也在里面，unquoted
        assert_eq!(
            code_spans("x = 42").iter().find(|(t, _)| t == "42").map(|(_, c)| *c),
            Some(crate::theme::json_num())
        );
    }
}
