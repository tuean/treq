use treq_core::codegen::{CodegenLang, generate};
use treq_core::*;

fn req() -> RequestItem {
    RequestItem {
        id: "i".into(),
        name: "n".into(),
        method: "GET".into(),
        url: "https://api.example.com/users/1".into(),
        params: vec![],
        headers: vec![Kv {
            key: "X-Test".into(),
            value: "a\"b".into(),
            enabled: true,
            description: String::new(),
        }],
        body: Body {
            kind: BodyKind::None,
            content: String::new(),
            form_data: Vec::new(),
        },
        description: String::new(),
        docs_open: true,
        auth: None,
    }
}

#[test]
fn get_no_body() {
    let code = generate(CodegenLang::JavaOkHttp, &req()).unwrap();
    assert!(code.contains(".url(\"https://api.example.com/users/1\")"));
    assert!(code.contains("builder.header(\"X-Test\", \"a\\\"b\")"));
    assert!(code.contains("Request request = builder.build();"));
    assert!(!code.contains("RequestBody body"));
}

#[test]
fn post_json_body() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Json,
        content: "{\"a\":1}".into(),
        form_data: Vec::new(),
    };
    let code = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    assert!(code.contains("RequestBody body = RequestBody.create("));
    assert!(code.contains("MediaType.parse(\"application/json\")"));
    assert!(code.contains("builder.post(body).build()"));
}

#[test]
fn post_form_body() {
    let mut r = req();
    r.method = "PUT".into();
    r.body = Body {
        kind: BodyKind::Form,
        content: "a=1&b=2".into(),
        form_data: Vec::new(),
    };
    let code = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    // 表单体走 urlencoded 字符串体（与发送路径一致），Content-Type 自动补
    assert!(code.contains("\"a=1&b=2\""));
    assert!(code.contains("application/x-www-form-urlencoded"));
    assert!(code.contains("builder.put(body).build()"));
}

#[test]
fn post_text_body() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Text,
        content: "hello".into(),
        form_data: Vec::new(),
    };
    let code = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    assert!(code.contains("MediaType.parse(\"text/plain; charset=utf-8\")"));
    assert!(code.contains("\"hello\""));
}

#[test]
fn post_file_body() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::File,
        content: "/tmp/a.png".into(),
        form_data: Vec::new(),
    };
    let code = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    assert!(code.contains("new java.io.File(\"/tmp/a.png\")"));
    assert!(code.contains("MediaType.parse(\"image/png\")"));
}

#[test]
fn post_multipart_body() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Multipart,
        content: String::new(),
        form_data: vec![
            FormField {
                key: "name".into(),
                value: "treq".into(),
                enabled: true,
                is_file: false,
            },
            FormField {
                key: "file".into(),
                value: "/tmp/a.png".into(),
                enabled: true,
                is_file: true,
            },
        ],
    };
    let code = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    assert!(code.contains("MultipartBody.Builder mb"));
    assert!(code.contains("mb.addFormDataPart(\"name\", \"treq\")"));
    assert!(code.contains("new java.io.File(\"/tmp/a.png\")"));
}

#[test]
fn delete_with_body() {
    let mut r = req();
    r.method = "DELETE".into();
    r.body = Body {
        kind: BodyKind::Raw,
        content: "x".into(),
        form_data: Vec::new(),
    };
    let code = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    assert!(code.contains("builder.delete(body).build()"));
}

#[test]
fn label_available() {
    assert_eq!(CodegenLang::JavaOkHttp.label(), "Java · OkHttp");
    // 每个语言/库都要有唯一 label 与稳定 id，且 id 可反解
    let mut labels = std::collections::HashSet::new();
    for l in CodegenLang::ALL {
        assert!(labels.insert(l.label()), "label 重复: {}", l.label());
        assert_eq!(
            CodegenLang::from_id(l.id()),
            Some(*l),
            "id 反解失败: {}",
            l.id()
        );
        assert!(!l.label().is_empty());
    }
    assert_eq!(CodegenLang::from_id("nope"), None);
}

/// 所有生成器在同一个 JSON POST 请求上的通用断言：
/// 非空、带完整 URL（含 query）、带用户头、带请求体、带 Content-Type。
#[test]
fn all_langs_share_common_expectations() {
    let mut r = req();
    r.method = "POST".into();
    r.url = "http://x/users".into();
    r.params = vec![Kv {
        key: "q".into(),
        value: "hello world".into(),
        enabled: true,
        description: String::new(),
    }];
    r.body = Body {
        kind: BodyKind::Json,
        content: "{\"a\":1}".into(),
        form_data: Vec::new(),
    };
    for lang in CodegenLang::ALL {
        let code = generate(*lang, &r).unwrap();
        assert!(!code.trim().is_empty(), "{} 生成为空", lang.label());
        assert!(
            code.contains("http://x/users?q=hello%20world"),
            "{} 缺少带 query 的 URL:\n{}",
            lang.label(),
            code
        );
        assert!(code.contains("X-Test"), "{} 缺少自定义头", lang.label());
        assert!(
            code.contains("application/json"),
            "{} 缺少 Content-Type",
            lang.label()
        );
        // 方法名大小写各语言不同（OkHttp 用 .post(...)、RestTemplate 用 HttpMethod.POST）
        assert!(
            code.to_lowercase().contains("post"),
            "{} 缺少方法:\n{}",
            lang.label(),
            code
        );
    }
}

#[test]
fn curl_multipart_and_file() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Multipart,
        content: String::new(),
        form_data: vec![
            FormField {
                key: "name".into(),
                value: "treq".into(),
                enabled: true,
                is_file: false,
            },
            FormField {
                key: "file".into(),
                value: "/tmp/a.png".into(),
                enabled: true,
                is_file: true,
            },
        ],
    };
    let code = generate(CodegenLang::ShellCurl, &r).unwrap();
    assert!(code.contains("-F 'name=treq'"), "{}", code);
    assert!(code.contains("-F 'file=@/tmp/a.png'"), "{}", code);

    let mut r2 = req();
    r2.method = "PUT".into();
    r2.body = Body {
        kind: BodyKind::File,
        content: "/tmp/a.png".into(),
        form_data: Vec::new(),
    };
    let curl = generate(CodegenLang::ShellCurl, &r2).unwrap();
    assert!(curl.contains("--data-binary @'/tmp/a.png'"), "{}", curl);
    let py = generate(CodegenLang::PythonRequests, &r2).unwrap();
    assert!(py.contains("open(\"/tmp/a.png\", \"rb\")"), "{}", py);
    let go = generate(CodegenLang::GoNetHttp, &r2).unwrap();
    assert!(go.contains("os.Open(\"/tmp/a.png\")"), "{}", go);
    let ps = generate(CodegenLang::PowerShell, &r2).unwrap();
    assert!(ps.contains("-InFile '/tmp/a.png'"), "{}", ps);
}

#[test]
fn form_body_langs() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Form,
        content: "a=1&b=2".into(),
        form_data: Vec::new(),
    };
    for lang in [
        CodegenLang::PythonRequests,
        CodegenLang::ShellCurl,
        CodegenLang::PhpCurl,
    ] {
        let code = generate(lang, &r).unwrap();
        assert!(code.contains("a=1&b=2"), "{}:\n{}", lang.label(), code);
        assert!(
            code.contains("application/x-www-form-urlencoded"),
            "{}",
            lang.label()
        );
    }
}

#[test]
fn multipart_note_when_not_supported() {
    let mut r = req();
    r.method = "POST".into();
    r.body = Body {
        kind: BodyKind::Multipart,
        content: String::new(),
        form_data: vec![FormField {
            key: "f".into(),
            value: "/tmp/a.png".into(),
            enabled: true,
            is_file: true,
        }],
    };
    // urllib / wget / ureq 不支持内联 multipart：必须给出部件说明而不是半截代码
    for lang in [
        CodegenLang::PythonUrllib,
        CodegenLang::ShellWget,
        CodegenLang::RustUreq,
    ] {
        let code = generate(lang, &r).unwrap();
        assert!(
            code.contains("multipart"),
            "{} 缺少说明:\n{}",
            lang.label(),
            code
        );
        assert!(
            code.contains("f = @/tmp/a.png"),
            "{} 缺少部件清单:\n{}",
            lang.label(),
            code
        );
    }
}

#[test]
fn escaping_is_language_specific() {
    let mut r = req();
    r.method = "POST".into();
    r.url = "https://x/it's".into();
    r.headers = vec![Kv {
        key: "X-Q".into(),
        value: "a'b\"c".into(),
        enabled: true,
        description: String::new(),
    }];
    r.body = Body {
        kind: BodyKind::Raw,
        content: "line1\nline2".into(),
        form_data: Vec::new(),
    };
    // shell 单引号内要转义 '
    let sh = generate(CodegenLang::ShellCurl, &r).unwrap();
    assert!(sh.contains("it'\\''s"), "{}", sh);
    // PS 单引号里 ' 翻倍
    let ps = generate(CodegenLang::PowerShell, &r).unwrap();
    assert!(ps.contains("it''s"), "{}", ps);
    // 双引号语言：\" 转义
    let java = generate(CodegenLang::JavaOkHttp, &r).unwrap();
    assert!(java.contains("a'b\\\"c"), "{}", java);
    assert!(java.contains("line1\\nline2"), "{}", java);
}
