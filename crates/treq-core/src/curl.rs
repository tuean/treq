//! cURL 命令解析：shell 风格分词 + 常见 flag → 请求。
//! 不做：变量展开、拼接、重定向、注释、--data-urlencode 编码。

use crate::models::*;
use anyhow::{Result, anyhow};

/// shell 风格分词：单引号原样、双引号（支持 `\"` `\\`）、反斜杠转义、连续空白分隔。
pub fn tokenize(cmd: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_token = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '"' => {
                in_token = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(&next) = chars.peek() {
                                if next == '"' || next == '\\' {
                                    cur.push(next);
                                    chars.next();
                                } else {
                                    cur.push('\\');
                                }
                            }
                        }
                        _ => cur.push(c),
                    }
                }
            }
            '\\' => {
                in_token = true;
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
            c if c.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            c => {
                in_token = true;
                cur.push(c);
            }
        }
    }
    if in_token {
        tokens.push(cur);
    }
    tokens
}

fn next_arg(tokens: &mut std::vec::IntoIter<String>, name: &str) -> Result<String> {
    tokens
        .next()
        .ok_or_else(|| anyhow!("flag {} missing argument", name))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCurl {
    pub method: String,
    pub url: String,
    pub headers: Vec<Kv>,
    pub body_kind: BodyKind,
    pub body_content: String,
    /// -F/--form 解析出的 multipart 字段（body_kind == Multipart 时使用）
    pub form_data: Vec<FormField>,
}

/// 解析 curl 命令。url 必须存在；未知 flag 忽略（有参数的 flag 会误吞下一个 token，见 ponytail 注释）。
/// ponytail: 未知 flag 一律跳过，带参未知 flag 会丢掉其参数值（如 -o out.txt 会吞 URL）。
/// 需要完整兼容时换 shellwords crate。
pub fn parse_curl(cmd: &str) -> Result<ParsedCurl> {
    let tokens = tokenize(cmd);
    let mut tokens = tokens.into_iter();
    let Some(first) = tokens.next() else {
        return Err(anyhow!("empty command"));
    };
    if first != "curl" {
        if first.starts_with("http://") || first.starts_with("https://") {
            // curl 省略形态：直接是 URL
            return finish_curl(first, vec![]);
        }
        return Err(anyhow!("expected `curl` at start of command"));
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<Kv> = Vec::new();
    let mut data_parts: Vec<String> = Vec::new();
    let mut form_parts: Vec<String> = Vec::new();
    let mut get_mode = false;
    let mut json_style = false;

    while let Some(tok) = tokens.next() {
        match tok.as_str() {
            "-X" | "--request" => {
                method = Some(next_arg(&mut tokens, "method")?);
            }
            "-H" | "--header" => {
                let h = next_arg(&mut tokens, "header")?;
                if let Some((k, v)) = h.split_once(':') {
                    headers.push(Kv {
                        key: k.trim().to_string(),
                        value: v.trim().to_string(),
                        enabled: true,
                        description: String::new(),
                    });
                }
            }
            "-d" | "--data" | "--data-ascii" | "--data-binary" | "--data-raw" => {
                data_parts.push(next_arg(&mut tokens, "data")?);
            }
            "--json" => {
                data_parts.push(next_arg(&mut tokens, "json")?);
                json_style = true;
            }
            "-F" | "--form" => {
                form_parts.push(next_arg(&mut tokens, "form")?);
            }
            "-u" | "--user" => {
                let user = next_arg(&mut tokens, "user")?;
                let base64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    user.as_bytes(),
                );
                headers.push(Kv {
                    key: "Authorization".into(),
                    value: format!("Basic {}", base64),
                    enabled: true,
                    description: String::new(),
                });
            }
            "-A" | "--user-agent" => {
                headers.push(Kv {
                    key: "User-Agent".into(),
                    value: next_arg(&mut tokens, "user-agent")?,
                    enabled: true,
                    description: String::new(),
                });
            }
            "-G" | "--get" => get_mode = true,
            "-I" | "--head" => method = Some("HEAD".into()),
            "-s" | "-S" | "-L" | "-k" | "-g" | "-v" | "-q" | "--silent" | "--show-error"
            | "--location" | "--insecure" | "--compressed" | "--verbose" | "--progress-bar"
            | "--fail" | "-f" | "--http1.1" | "--http2" | "-0" | "--http1.0" => {}
            "--url" => {
                url = Some(next_arg(&mut tokens, "url")?);
            }
            t if t.starts_with("--url=") => {
                url = Some(t["--url=".len()..].to_string());
            }
            t if t.starts_with("--request=") => {
                method = Some(t["--request=".len()..].to_string());
            }
            t if t.starts_with("--form=") => {
                form_parts.push(t["--form=".len()..].to_string());
            }
            t if t.starts_with("--header=") => {
                let h = t["--header=".len()..].to_string();
                if let Some((k, v)) = h.split_once(':') {
                    headers.push(Kv {
                        key: k.trim().to_string(),
                        value: v.trim().to_string(),
                        enabled: true,
                        description: String::new(),
                    });
                }
            }
            t if t.starts_with('-') && t != "-" => {
                // 未知 flag：跳过其参数（--flag=value 形式的值随 flag 一起忽略）
                // 组合短 flag（-sSL）全为无参短 flag 时不吞下一个 token
                if !t.contains('=') && !all_short_no_arg(t) {
                    let _ = next_arg(&mut tokens, "unknown flag value");
                }
            }
            t => {
                url = Some(t.to_string());
            }
        }
    }

    finish(
        url, method, headers, data_parts, form_parts, get_mode, json_style,
    )
}

fn finish_curl(url: String, headers: Vec<Kv>) -> Result<ParsedCurl> {
    finish(Some(url), None, headers, vec![], vec![], false, false)
}

fn finish(
    url: Option<String>,
    method: Option<String>,
    headers: Vec<Kv>,
    data_parts: Vec<String>,
    form_parts: Vec<String>,
    get_mode: bool,
    json_style: bool,
) -> Result<ParsedCurl> {
    let url = url.ok_or_else(|| anyhow!("no URL found in command"))?;
    let mut url = url;
    let body_content = if !form_parts.is_empty() {
        form_parts.join("\n")
    } else {
        data_parts.join("&")
    };
    let has_body = !data_parts.is_empty() || !form_parts.is_empty();

    // -G：data 转 query
    if get_mode && has_body {
        let sep = if url.contains('?') { "&" } else { "?" };
        url.push_str(sep);
        url.push_str(&body_content);
    }

    let method = match method {
        Some(m) => m.to_uppercase(),
        None if has_body && !get_mode => "POST".into(),
        None => "GET".into(),
    };

    // body kind 判定：--json 优先，content-type 次之，curl -d 默认 form-urlencoded
    let ct = headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase());
    let body_kind = if !form_parts.is_empty() {
        BodyKind::Multipart
    } else if !has_body || get_mode {
        BodyKind::None
    } else if json_style {
        BodyKind::Json
    } else if let Some(ct) = ct {
        if ct.contains("json") {
            BodyKind::Json
        } else if ct.contains("form-urlencoded") {
            BodyKind::Form
        } else {
            BodyKind::Raw
        }
    } else {
        // curl 语义：-d 无显式 content-type 时为 form-urlencoded
        BodyKind::Form
    };

    let form_data: Vec<FormField> = form_parts
        .iter()
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            let is_file = value.starts_with('@');
            Some(FormField {
                key: key.trim().to_string(),
                value: if is_file {
                    value[1..].trim().to_string()
                } else {
                    value.trim().to_string()
                },
                enabled: true,
                is_file,
            })
        })
        .collect();

    Ok(ParsedCurl {
        method,
        url,
        headers,
        body_kind,
        body_content,
        form_data,
    })
}

/// 组合短 flag（如 -sSL）中是否全部为无参短 flag。
fn all_short_no_arg(t: &str) -> bool {
    let chars = t.strip_prefix('-').unwrap_or(t).chars();
    let mut has_short = false;
    for c in chars {
        has_short = true;
        if !matches!(
            c,
            's' | 'S' | 'L' | 'k' | 'g' | 'v' | 'i' | 'f' | 'q' | 'G' | 'I' | '0'
        ) {
            return false;
        }
    }
    has_short
}
