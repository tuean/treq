//! 开发用：把一条示例请求在所有语言/库下的生成代码打到 stdout。
//! 用法：cargo run -p treq-core --example codegen_dump
//!       cargo run -p treq-core --example codegen_dump -- shell-curl   （只看某一个）

use treq_core::codegen::{CodegenLang, generate};
use treq_core::*;

fn sample() -> RequestItem {
    RequestItem {
        id: "demo".into(),
        name: "创建用户".into(),
        method: "POST".into(),
        url: "https://api.example.com/v1/users".into(),
        params: vec![Kv {
            key: "verbose".into(),
            value: "1".into(),
            enabled: true,
            description: String::new(),
        }],
        headers: vec![
            Kv {
                key: "Authorization".into(),
                value: "Bearer demo-token".into(),
                enabled: true,
                description: String::new(),
            },
            Kv {
                key: "X-Disabled".into(),
                value: "nope".into(),
                enabled: false,
                description: String::new(),
            },
        ],
        body: Body {
            kind: BodyKind::Json,
            content: "{\n  \"name\": \"qiwei\",\n  \"dept\": 2629\n}".into(),
            form_data: Vec::new(),
        },
        description: String::new(),
        docs_open: true,
        auth: None,
        order: None,
    }
}

fn main() {
    let only = std::env::args().nth(1);
    let req = sample();
    for lang in CodegenLang::ALL {
        if let Some(only) = &only
            && lang.id() != only
        {
            continue;
        }
        println!(
            "\n{:=<72}\n== {} ({})\n{:=<72}",
            "",
            lang.label(),
            lang.id(),
            ""
        );
        match generate(*lang, &req) {
            Ok(code) => print!("{}", code),
            Err(e) => println!("生成失败: {}", e),
        }
    }
}
