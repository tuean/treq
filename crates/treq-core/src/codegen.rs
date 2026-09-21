//! 代码生成插件：从请求生成各语言 / 各主流库的示例代码。
//! 新增目标：加 enum 变体 → 加进 `ALL` → `generate()` 分支 → 写一个 `xxx()` 生成函数。
//!
//! 约定：请求体先统一解析成 `BodyPlan`（None / Text / File / Multipart），
//! 各语言只照自己的惯用 API 展开，避免每种语言重复判断 kind。
//! 某个库不支持某种体（如 urllib 的 multipart）时生成**注释说明**而不是半截错代码。

use crate::models::*;
use anyhow::Result;

/// 支持的代码生成目标（语言 + 主流库）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodegenLang {
    #[default]
    JavaOkHttp,
    JavaRestTemplate,
    JavaHttpClient,
    PythonRequests,
    PythonUrllib,
    RustReqwest,
    RustUreq,
    GoNetHttp,
    JsFetch,
    JsAxios,
    ShellCurl,
    ShellWget,
    PowerShell,
    CSharpHttpClient,
    PhpCurl,
}

impl CodegenLang {
    /// 菜单/下拉里的展示顺序：按语言聚合，库跟在语言后面。
    pub const ALL: &'static [CodegenLang] = &[
        CodegenLang::JavaOkHttp,
        CodegenLang::JavaRestTemplate,
        CodegenLang::JavaHttpClient,
        CodegenLang::PythonRequests,
        CodegenLang::PythonUrllib,
        CodegenLang::RustReqwest,
        CodegenLang::RustUreq,
        CodegenLang::GoNetHttp,
        CodegenLang::JsFetch,
        CodegenLang::JsAxios,
        CodegenLang::ShellCurl,
        CodegenLang::ShellWget,
        CodegenLang::PowerShell,
        CodegenLang::CSharpHttpClient,
        CodegenLang::PhpCurl,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            CodegenLang::JavaOkHttp => "Java · OkHttp",
            CodegenLang::JavaRestTemplate => "Java · RestTemplate",
            CodegenLang::JavaHttpClient => "Java · HttpClient (JDK11+)",
            CodegenLang::PythonRequests => "Python · requests",
            CodegenLang::PythonUrllib => "Python · urllib (标准库)",
            CodegenLang::RustReqwest => "Rust · reqwest",
            CodegenLang::RustUreq => "Rust · ureq",
            CodegenLang::GoNetHttp => "Go · net/http",
            CodegenLang::JsFetch => "JavaScript · fetch",
            CodegenLang::JsAxios => "JavaScript · axios",
            CodegenLang::ShellCurl => "Shell · curl",
            CodegenLang::ShellWget => "Shell · wget",
            CodegenLang::PowerShell => "PowerShell · Invoke-RestMethod",
            CodegenLang::CSharpHttpClient => "C# · HttpClient",
            CodegenLang::PhpCurl => "PHP · cURL",
        }
    }

    /// 存 settings / 菜单 id 用的稳定标识。
    pub fn id(&self) -> &'static str {
        match self {
            CodegenLang::JavaOkHttp => "java-okhttp",
            CodegenLang::JavaRestTemplate => "java-resttemplate",
            CodegenLang::JavaHttpClient => "java-httpclient",
            CodegenLang::PythonRequests => "python-requests",
            CodegenLang::PythonUrllib => "python-urllib",
            CodegenLang::RustReqwest => "rust-reqwest",
            CodegenLang::RustUreq => "rust-ureq",
            CodegenLang::GoNetHttp => "go-nethttp",
            CodegenLang::JsFetch => "js-fetch",
            CodegenLang::JsAxios => "js-axios",
            CodegenLang::ShellCurl => "shell-curl",
            CodegenLang::ShellWget => "shell-wget",
            CodegenLang::PowerShell => "powershell",
            CodegenLang::CSharpHttpClient => "csharp-httpclient",
            CodegenLang::PhpCurl => "php-curl",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|l| l.id() == id)
    }
}

// ===================== 共用：请求体计划 =====================

/// 请求体的统一描述，各语言生成器按它展开。
enum BodyPlan {
    None,
    /// 字符串体（JSON / 文本 / 原文 / 表单 urlencoded）
    Text {
        text: String,
        media_type: Option<String>,
    },
    File {
        path: String,
        media_type: String,
    },
    Multipart(Vec<FormField>),
}

fn body_plan(req: &RequestItem) -> BodyPlan {
    match req.body.kind {
        BodyKind::None => BodyPlan::None,
        BodyKind::Json => BodyPlan::Text {
            text: req.body.content.clone(),
            media_type: Some("application/json".into()),
        },
        BodyKind::Text => BodyPlan::Text {
            text: req.body.content.clone(),
            media_type: Some("text/plain; charset=utf-8".into()),
        },
        BodyKind::Form => BodyPlan::Text {
            text: req.body.content.clone(),
            media_type: Some("application/x-www-form-urlencoded".into()),
        },
        BodyKind::Raw => BodyPlan::Text {
            text: req.body.content.clone(),
            media_type: None,
        },
        BodyKind::File => {
            let path = req.body.content.trim().to_string();
            let media_type = crate::http::guess_content_type(&path).to_string();
            BodyPlan::File { path, media_type }
        }
        BodyKind::Multipart => {
            let fields = req
                .body
                .multipart_fields()
                .into_iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .collect();
            BodyPlan::Multipart(fields)
        }
    }
}

/// 用户显式设置的 Content-Type（有的话生成器就不再自己加一行）。
fn has_user_content_type(req: &RequestItem) -> bool {
    req.headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type") && !h.value.is_empty())
}

/// 生成器要自己补的 Content-Type（用户没写且体有默认类型时）。
fn auto_media_type(req: &RequestItem, plan: &BodyPlan) -> Option<String> {
    if has_user_content_type(req) {
        return None;
    }
    match plan {
        BodyPlan::Text { media_type, .. } => media_type.clone(),
        BodyPlan::File { media_type, .. } => Some(media_type.clone()),
        _ => None,
    }
}

fn enabled_headers(req: &RequestItem) -> Vec<&Kv> {
    req.headers
        .iter()
        .filter(|h| h.enabled && !h.key.is_empty())
        .collect()
}

fn method_upper(req: &RequestItem) -> String {
    req.method.to_uppercase()
}

/// 该库不便内联生成 multipart 时的说明注释（列出等价的部件清单）。
fn multipart_note(fields: &[FormField], comment: &str) -> String {
    let mut out = String::new();
    for f in fields {
        if f.is_file {
            out.push_str(&format!(
                "{} multipart 文件部件: {} = @{}\n",
                comment, f.key, f.value
            ));
        } else {
            out.push_str(&format!(
                "{} multipart 文本部件: {} = {}\n",
                comment, f.key, f.value
            ));
        }
    }
    out
}

// ===================== 入口 =====================

pub fn generate(lang: CodegenLang, req: &RequestItem) -> Result<String> {
    let url = crate::vars::apply_params(&req.url, &req.params);
    let plan = body_plan(req);
    let m = method_upper(req);
    Ok(match lang {
        CodegenLang::JavaOkHttp => java_okhttp(req, &url, &plan, &m),
        CodegenLang::JavaRestTemplate => java_rest_template(req, &url, &plan, &m),
        CodegenLang::JavaHttpClient => java_http_client(req, &url, &plan, &m),
        CodegenLang::PythonRequests => python_requests(req, &url, &plan, &m),
        CodegenLang::PythonUrllib => python_urllib(req, &url, &plan, &m),
        CodegenLang::RustReqwest => rust_reqwest(req, &url, &plan, &m),
        CodegenLang::RustUreq => rust_ureq(req, &url, &plan, &m),
        CodegenLang::GoNetHttp => go_net_http(req, &url, &plan, &m),
        CodegenLang::JsFetch => js_fetch(req, &url, &plan, &m),
        CodegenLang::JsAxios => js_axios(req, &url, &plan, &m),
        CodegenLang::ShellCurl => shell_curl(req, &url, &plan),
        CodegenLang::ShellWget => shell_wget(req, &url, &plan, &m),
        CodegenLang::PowerShell => powershell(req, &url, &plan, &m),
        CodegenLang::CSharpHttpClient => csharp_http_client(req, &url, &plan, &m),
        CodegenLang::PhpCurl => php_curl(req, &url, &plan),
    })
}

// ===================== 转义工具 =====================

/// 反斜杠风格（Java/C/Python/JS/Go/PHP/Rust 的双引号字符串）。
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// shell/PowerShell 单引号字符串（shell: '\'' ；PS: ''）
fn esc_shell(s: &str) -> String {
    s.replace('\'', "'\\''")
}
fn esc_ps(s: &str) -> String {
    s.replace('\'', "''")
}

fn esc_java(s: &str) -> String {
    esc(s)
}

// ===================== Java =====================

/// Java OkHttp（okhttp3）。
fn java_okhttp(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("import okhttp3.*;\nimport java.io.IOException;\n\n");
    code.push_str("public class ApiCall {\n");
    code.push_str("  public static void main(String[] args) throws IOException {\n");
    code.push_str("    OkHttpClient client = new OkHttpClient();\n");
    code.push_str("    Request.Builder builder = new Request.Builder()\n");
    code.push_str(&format!("        .url(\"{}\");\n", esc_java(url)));

    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    builder.header(\"{}\", \"{}\");\n",
            esc_java(&h.key),
            esc_java(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            "    builder.header(\"Content-Type\", \"{}\");\n",
            esc_java(&ct)
        ));
    }

    let has_body = !matches!(plan, BodyPlan::None);
    match plan {
        BodyPlan::Text { text, media_type } => {
            let mt = media_type
                .as_ref()
                .map(|m| format!("MediaType.parse(\"{}\")", esc_java(m)))
                .unwrap_or_else(|| "null".into());
            code.push_str("    RequestBody body = RequestBody.create(\n");
            code.push_str(&format!("        {},\n", mt));
            code.push_str(&format!("        \"{}\"\n", esc_java(text)));
            code.push_str("    );\n");
        }
        BodyPlan::File { path, media_type } => {
            code.push_str(&format!(
                "    RequestBody body = RequestBody.create(new java.io.File(\"{}\"), MediaType.parse(\"{}\"));\n",
                esc_java(path),
                esc_java(media_type)
            ));
        }
        BodyPlan::Multipart(fields) => {
            code.push_str(
                "    MultipartBody.Builder mb = new MultipartBody.Builder().setType(MultipartBody.FORM);\n",
            );
            for f in fields {
                if f.is_file {
                    let name = file_name(&f.value);
                    code.push_str(&format!(
                        "    mb.addFormDataPart(\"{}\", \"{}\", RequestBody.create(new java.io.File(\"{}\"), MediaType.parse(\"{}\")));\n",
                        esc_java(&f.key),
                        esc_java(&name),
                        esc_java(f.value.trim()),
                        crate::http::guess_content_type(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        "    mb.addFormDataPart(\"{}\", \"{}\");\n",
                        esc_java(&f.key),
                        esc_java(&f.value)
                    ));
                }
            }
            code.push_str("    RequestBody body = mb.build();\n");
        }
        BodyPlan::None => {}
    }

    match (method, has_body) {
        ("GET", false) => code.push_str("    Request request = builder.build();\n"),
        ("HEAD", false) => code.push_str("    Request request = builder.head().build();\n"),
        ("DELETE", false) => code.push_str("    Request request = builder.delete().build();\n"),
        ("POST", true) => code.push_str("    Request request = builder.post(body).build();\n"),
        ("PUT", true) => code.push_str("    Request request = builder.put(body).build();\n"),
        ("PATCH", true) => code.push_str("    Request request = builder.patch(body).build();\n"),
        ("DELETE", true) => code.push_str("    Request request = builder.delete(body).build();\n"),
        (m, true) => code.push_str(&format!(
            "    Request request = builder.method(\"{}\", body).build();\n",
            m
        )),
        (m, false) => code.push_str(&format!(
            "    Request request = builder.method(\"{}\", null).build();\n",
            m
        )),
    }
    code.push('\n');
    code.push_str("    try (Response response = client.newCall(request).execute()) {\n");
    code.push_str("      System.out.println(response.code());\n");
    code.push_str("      if (response.body() != null) {\n");
    code.push_str("        System.out.println(response.body().string());\n");
    code.push_str("      }\n    }\n  }\n}\n");
    code
}

/// Java Spring RestTemplate。
fn java_rest_template(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str(
        "import org.springframework.http.*;\nimport org.springframework.web.client.RestTemplate;\n",
    );
    code.push_str("public class ApiCall {\n");
    code.push_str("  public static void main(String[] args) {\n");
    code.push_str("    RestTemplate rest = new RestTemplate();\n");
    code.push_str("    HttpHeaders headers = new HttpHeaders();\n");
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    headers.set(\"{}\", \"{}\");\n",
            esc_java(&h.key),
            esc_java(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            "    headers.setContentType(MediaType.parseMediaType(\"{}\"));\n",
            esc_java(&ct)
        ));
    }

    let body_expr = match plan {
        BodyPlan::None => "null".to_string(),
        BodyPlan::Text { text, .. } => format!("\"{}\"", esc_java(text)),
        BodyPlan::File { path, .. } => format!(
            "new org.springframework.core.io.FileSystemResource(\"{}\")",
            esc_java(path)
        ),
        BodyPlan::Multipart(fields) => {
            code.push_str(
                "    org.springframework.util.LinkedMultiValueMap<String, Object> form = new org.springframework.util.LinkedMultiValueMap<>();\n",
            );
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "    form.add(\"{}\", new org.springframework.core.io.FileSystemResource(\"{}\"));\n",
                        esc_java(&f.key),
                        esc_java(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        "    form.add(\"{}\", \"{}\");\n",
                        esc_java(&f.key),
                        esc_java(&f.value)
                    ));
                }
            }
            code.push_str("    headers.setContentType(MediaType.MULTIPART_FORM_DATA);\n");
            "form".to_string()
        }
    };
    code.push_str(&format!(
        "    HttpEntity<Object> entity = new HttpEntity<>({}, headers);\n",
        body_expr
    ));
    code.push_str(&format!(
        "    ResponseEntity<String> response = rest.exchange(\n        \"{}\", HttpMethod.{}, entity, String.class);\n",
        esc_java(url),
        method
    ));
    code.push_str("    System.out.println(response.getStatusCode());\n");
    code.push_str("    System.out.println(response.getBody());\n  }\n}\n");
    code
}

/// Java 11+ 标准库 java.net.http。
fn java_http_client(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("import java.net.URI;\nimport java.net.http.*;\nimport java.nio.file.Path;\nimport java.time.Duration;\n\n");
    code.push_str("public class ApiCall {\n");
    code.push_str("  public static void main(String[] args) throws Exception {\n");
    code.push_str("    HttpClient client = HttpClient.newBuilder()\n");
    code.push_str("        .connectTimeout(Duration.ofSeconds(30))\n        .build();\n\n");
    code.push_str("    HttpRequest.Builder builder = HttpRequest.newBuilder()\n");
    code.push_str(&format!(
        "        .uri(URI.create(\"{}\"));\n",
        esc_java(url)
    ));
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    builder.header(\"{}\", \"{}\");\n",
            esc_java(&h.key),
            esc_java(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            "    builder.header(\"Content-Type\", \"{}\");\n",
            esc_java(&ct)
        ));
    }
    match plan {
        BodyPlan::None => code.push_str(&format!(
            "    builder.method(\"{}\", HttpRequest.BodyPublishers.noBody());\n",
            method
        )),
        BodyPlan::Text { text, .. } => code.push_str(&format!(
            "    builder.method(\"{}\", HttpRequest.BodyPublishers.ofString(\"{}\"));\n",
            method,
            esc_java(text)
        )),
        BodyPlan::File { path, .. } => code.push_str(&format!(
            "    builder.method(\"{}\", HttpRequest.BodyPublishers.ofFile(Path.of(\"{}\")));\n",
            method,
            esc_java(path)
        )),
        BodyPlan::Multipart(fields) => {
            code.push_str("    // java.net.http 没有内置 multipart，手写边界或用 okhttp/RestTemplate 版本：\n");
            code.push_str(&multipart_note(fields, "    //"));
            code.push_str(&format!(
                "    builder.method(\"{}\", HttpRequest.BodyPublishers.noBody());\n",
                method
            ));
        }
    }
    code.push_str("\n    HttpResponse<String> response = client.send(\n");
    code.push_str("        builder.build(), HttpResponse.BodyHandlers.ofString());\n");
    code.push_str("    System.out.println(response.statusCode());\n");
    code.push_str("    System.out.println(response.body());\n  }\n}\n");
    code
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string())
}

// ===================== Python =====================

/// Python requests。
fn python_requests(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("import requests\n\n");
    code.push_str(&format!("url = \"{}\"\n", esc(url)));
    code.push_str("headers = {\n");
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    \"{}\": \"{}\",\n",
            esc(&h.key),
            esc(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!("    \"Content-Type\": \"{}\",\n", esc(&ct)));
    }
    code.push_str("}\n\n");
    match plan {
        BodyPlan::None => {
            code.push_str(&format!(
                "response = requests.request(\"{}\", url, headers=headers, timeout=30)\n",
                method
            ));
        }
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!("body = \"\"\"{}\"\"\"\n\n", text));
            code.push_str(&format!(
                "response = requests.request(\"{}\", url, headers=headers, data=body.encode(\"utf-8\"), timeout=30)\n",
                method
            ));
        }
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(
                "with open(\"{}\", \"rb\") as f:\n    body = f.read()\n\n",
                esc(path)
            ));
            code.push_str(&format!(
                "response = requests.request(\"{}\", url, headers=headers, data=body, timeout=30)\n",
                method
            ));
        }
        BodyPlan::Multipart(fields) => {
            code.push_str("files = {\n");
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "    \"{}\": open(\"{}\", \"rb\"),\n",
                        esc(&f.key),
                        esc(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        "    \"{}\": (None, \"{}\"),\n",
                        esc(&f.key),
                        esc(&f.value)
                    ));
                }
            }
            code.push_str("}\n");
            code.push_str(&format!(
                "\nresponse = requests.request(\"{}\", url, headers=headers, files=files, timeout=30)\n",
                method
            ));
        }
    }
    code.push_str("\nprint(response.status_code)\nprint(response.text)\n");
    code
}

/// Python 标准库 urllib。
fn python_urllib(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("import urllib.request\n\n");
    code.push_str(&format!("url = \"{}\"\n", esc(url)));
    code.push_str("headers = {\n");
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    \"{}\": \"{}\",\n",
            esc(&h.key),
            esc(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!("    \"Content-Type\": \"{}\",\n", esc(&ct)));
    }
    code.push_str("}\n\n");
    match plan {
        BodyPlan::None => {
            code.push_str(&format!(
                "request = urllib.request.Request(url, headers=headers, method=\"{}\")\n",
                method
            ));
        }
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!(
                "body = \"\"\"{}\"\"\".encode(\"utf-8\")\n\n",
                text
            ));
            code.push_str(&format!(
                "request = urllib.request.Request(url, data=body, headers=headers, method=\"{}\")\n",
                method
            ));
        }
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(
                "body = open(\"{}\", \"rb\").read()\n\n",
                esc(path)
            ));
            code.push_str(&format!(
                "request = urllib.request.Request(url, data=body, headers=headers, method=\"{}\")\n",
                method
            ));
        }
        BodyPlan::Multipart(fields) => {
            // urllib 没有 multipart 封装，给出等价说明（生成可用但不简洁的边界逻辑没意义）
            code.push_str("# urllib 没有 multipart 封装，建议改用 requests 版本；部件如下：\n");
            code.push_str(&multipart_note(fields, "#"));
            code.push_str(&format!(
                "request = urllib.request.Request(url, headers=headers, method=\"{}\")\n",
                method
            ));
        }
    }
    code.push_str("\nwith urllib.request.urlopen(request, timeout=30) as response:\n");
    code.push_str("    print(response.status)\n    print(response.read().decode(\"utf-8\"))\n");
    code
}

// ===================== Rust =====================

/// Rust reqwest（阻塞版）。
fn rust_reqwest(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str(
        "use reqwest::blocking::Client;\nuse reqwest::Method;\nuse std::time::Duration;\n\n",
    );
    code.push_str("fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
    code.push_str(
        "    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;\n\n",
    );
    code.push_str("    let mut request = client\n");
    code.push_str(&format!(
        "        .request(Method::from_bytes(b\"{}\")?, \"{}\")\n",
        method,
        esc(url)
    ));
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            "        .header(\"Content-Type\", \"{}\")\n",
            esc(&ct)
        ));
    }
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "        .header(\"{}\", \"{}\")\n",
            esc(&h.key),
            esc(&h.value)
        ));
    }
    match plan {
        BodyPlan::None => code.push_str("        ;\n"),
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!("        .body(r#\"{}\"#.to_string());\n", text))
        }
        BodyPlan::File { path, .. } => code.push_str(&format!(
            "        .body(std::fs::File::open(\"{}\")?);\n",
            esc(path)
        )),
        BodyPlan::Multipart(fields) => {
            code.push_str("        .multipart({\n");
            code.push_str(
                "            let mut form = reqwest::blocking::multipart::Form::new();\n",
            );
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "            form = form.file(\"{}\", \"{}\")?;\n",
                        esc(&f.key),
                        esc(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        "            form = form.text(\"{}\", \"{}\");\n",
                        esc(&f.key),
                        esc(&f.value)
                    ));
                }
            }
            code.push_str("            form\n        });\n");
        }
    }
    code.push_str("\n    let response = request.send()?;\n");
    code.push_str("    println!(\"{}\", response.status());\n");
    code.push_str("    println!(\"{}\", response.text()?);\n    Ok(())\n}\n");
    code
}

/// Rust ureq（轻量同步）。
fn rust_ureq(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("use std::time::Duration;\n\n");
    code.push_str("fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
    code.push_str(
        "    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build();\n",
    );
    code.push_str(&format!("    let url = \"{}\";\n", esc(url)));
    code.push_str(&format!(
        "    let mut request = agent.request(\"{}\", url);\n",
        method
    ));
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    request = request.set(\"{}\", \"{}\");\n",
            esc(&h.key),
            esc(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            "    request = request.set(\"Content-Type\", \"{}\");\n",
            esc(&ct)
        ));
    }
    match plan {
        BodyPlan::None => code.push_str("    let response = request.call()?;\n"),
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!(
                "    let response = request.send_string(r#\"{}\"#)?;\n",
                text
            ));
        }
        BodyPlan::File { path, .. } => code.push_str(&format!(
            "    let response = request.send(std::fs::File::open(\"{}\")?)?;\n",
            esc(path)
        )),
        BodyPlan::Multipart(fields) => {
            code.push_str("    // ureq 的 multipart 支持有限（send_form 只支持文本部件）：\n");
            code.push_str(&multipart_note(fields, "    //"));
            code.push_str("    let response = request.call()?;\n");
        }
    }
    code.push_str("    println!(\"{}\", response.status());\n");
    code.push_str("    println!(\"{}\", response.into_string()?);\n    Ok(())\n}\n");
    code
}

// ===================== Go =====================

/// Go 标准库 net/http。
fn go_net_http(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    // import 按 gofmt 的字母序输出
    let mut imports = vec!["fmt", "io", "net/http", "time"];
    match plan {
        BodyPlan::File { .. } => imports.push("os"),
        BodyPlan::Multipart(_) => {
            imports.extend(["bytes", "mime/multipart", "os"]);
        }
        _ => imports.push("strings"),
    }
    imports.sort_unstable();
    code.push_str("package main\n\nimport (\n");
    for i in imports {
        code.push_str(&format!("\t\"{}\"\n", i));
    }
    code.push_str(")\n\n");
    code.push_str("func main() {\n");
    match plan {
        BodyPlan::Multipart(fields) => {
            code.push_str("\tvar body bytes.Buffer\n\twriter := multipart.NewWriter(&body)\n");
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "\tfile, _ := os.Open(\"{}\")\n\tdefer file.Close()\n\tpart, _ := writer.CreateFormFile(\"{}\", \"{}\")\n\tio.Copy(part, file)\n",
                        esc(f.value.trim()),
                        esc(&f.key),
                        esc(&file_name(&f.value))
                    ));
                } else {
                    code.push_str(&format!(
                        "\twriter.WriteField(\"{}\", \"{}\")\n",
                        esc(&f.key),
                        esc(&f.value)
                    ));
                }
            }
            code.push_str("\twriter.Close()\n");
        }
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(
                "\tbody, err := os.Open(\"{}\")\n\tif err != nil {{\n\t\tpanic(err)\n\t}}\n\tdefer body.Close()\n",
                esc(path)
            ));
        }
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!("\tbody := strings.NewReader(`{}`)\n", text));
        }
        BodyPlan::None => {}
    }
    let body_arg = match plan {
        BodyPlan::None => "nil".to_string(),
        BodyPlan::Multipart(_) => "&body".to_string(),
        _ => "body".to_string(),
    };
    code.push_str(&format!(
        "\treq, err := http.NewRequest(\"{}\", \"{}\", {})\n\tif err != nil {{\n\t\tpanic(err)\n\t}}\n",
        method,
        esc(url),
        body_arg
    ));
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "\treq.Header.Set(\"{}\", \"{}\")\n",
            esc(&h.key),
            esc(&h.value)
        ));
    }
    if let BodyPlan::Multipart(_) = plan {
        code.push_str("\treq.Header.Set(\"Content-Type\", writer.FormDataContentType())\n");
    } else if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            "\treq.Header.Set(\"Content-Type\", \"{}\")\n",
            esc(&ct)
        ));
    }
    code.push_str(
        "\n\tclient := &http.Client{Timeout: 30 * time.Second}\n\tresp, err := client.Do(req)\n\tif err != nil {\n\t\tpanic(err)\n\t}\n\tdefer resp.Body.Close()\n",
    );
    code.push_str("\n\tdata, _ := io.ReadAll(resp.Body)\n\tfmt.Println(resp.Status)\n\tfmt.Println(string(data))\n}\n");
    code
}

// ===================== JavaScript =====================

/// 浏览器 / Node 18+ 的 fetch。
fn js_fetch(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    if matches!(plan, BodyPlan::File { .. } | BodyPlan::Multipart(_)) {
        code.push_str("import fs from \"node:fs\";\n\n");
    }
    code.push_str(&format!("const url = \"{}\";\n", esc(url)));
    code.push_str("const headers = {\n");
    for h in enabled_headers(req) {
        code.push_str(&format!("  \"{}\": \"{}\",\n", esc(&h.key), esc(&h.value)));
    }
    code.push_str("};\n\n");
    match plan {
        BodyPlan::None => code.push_str(&format!(
            "const response = await fetch(url, {{ method: \"{}\", headers }});\n",
            method
        )),
        BodyPlan::Text { text, .. } => {
            if let Some(ct) = auto_media_type(req, plan) {
                code.push_str(&format!(
                    "headers[\"Content-Type\"] = \"{}\";\n\n",
                    esc(&ct)
                ));
            }
            code.push_str(&format!("const body = `{}`;\n\n", text));
            code.push_str(&format!(
                "const response = await fetch(url, {{ method: \"{}\", headers, body }});\n",
                method
            ));
        }
        BodyPlan::File { path, .. } => {
            if let Some(ct) = auto_media_type(req, plan) {
                code.push_str(&format!("headers[\"Content-Type\"] = \"{}\";\n", esc(&ct)));
            }
            code.push_str(&format!(
                "const body = fs.readFileSync(\"{}\");\n\n",
                esc(path)
            ));
            code.push_str(&format!(
                "const response = await fetch(url, {{ method: \"{}\", headers, body }});\n",
                method
            ));
        }
        BodyPlan::Multipart(fields) => {
            code.push_str("const form = new FormData();\n");
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "form.append(\"{}\", new Blob([fs.readFileSync(\"{}\")]), \"{}\");\n",
                        esc(&f.key),
                        esc(f.value.trim()),
                        esc(&file_name(&f.value))
                    ));
                } else {
                    code.push_str(&format!(
                        "form.append(\"{}\", \"{}\");\n",
                        esc(&f.key),
                        esc(&f.value)
                    ));
                }
            }
            code.push_str(&format!(
                "\nconst response = await fetch(url, {{ method: \"{}\", headers, body: form }});\n",
                method
            ));
        }
    }
    code.push_str("\nconsole.log(response.status);\nconsole.log(await response.text());\n");
    code
}

/// Node axios。
fn js_axios(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("import axios from \"axios\";\n");
    if matches!(plan, BodyPlan::File { .. }) {
        code.push_str("import fs from \"node:fs\";\n");
    }
    code.push_str("\nconst headers = {\n");
    for h in enabled_headers(req) {
        code.push_str(&format!("  \"{}\": \"{}\",\n", esc(&h.key), esc(&h.value)));
    }
    code.push_str("};\n\n");
    let body = match plan {
        BodyPlan::None => "undefined".to_string(),
        BodyPlan::Text { text, .. } => format!("`{}`", text),
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(
                "const body = fs.readFileSync(\"{}\");\n\n",
                esc(path)
            ));
            "body".to_string()
        }
        BodyPlan::Multipart(fields) => {
            code.push_str("const form = new FormData();\n");
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "form.append(\"{}\", fs.createReadStream(\"{}\"));\n",
                        esc(&f.key),
                        esc(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        "form.append(\"{}\", \"{}\");\n",
                        esc(&f.key),
                        esc(&f.value)
                    ));
                }
            }
            code.push('\n');
            "form".to_string()
        }
    };
    if let Some(ct) = auto_media_type(req, plan)
        && !matches!(plan, BodyPlan::Multipart(_))
    {
        code.push_str(&format!(
            "headers[\"Content-Type\"] = \"{}\";\n\n",
            esc(&ct)
        ));
    }
    code.push_str(&format!(
        "const response = await axios.request({{\n  method: \"{}\",\n  url: \"{}\",\n  headers,\n  data: {},\n  timeout: 30000,\n}});\n",
        method,
        esc(url),
        body
    ));
    code.push_str("\nconsole.log(response.status);\nconsole.log(response.data);\n");
    code
}

// ===================== Shell =====================

/// curl。
fn shell_curl(req: &RequestItem, url: &str, plan: &BodyPlan) -> String {
    let mut code = String::new();
    code.push_str(&format!(
        "curl -X {} '{}'",
        method_upper(req),
        esc_shell(url)
    ));
    code.push_str(" \\\n  --max-time 30");
    for h in enabled_headers(req) {
        code.push_str(&format!(
            " \\\n  -H '{}: {}'",
            esc_shell(&h.key),
            esc_shell(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(" \\\n  -H 'Content-Type: {}'", esc_shell(&ct)));
    }
    match plan {
        BodyPlan::None => {}
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!(" \\\n  --data-raw '{}'", esc_shell(text)));
        }
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(" \\\n  --data-binary @'{}'", esc_shell(path)));
        }
        BodyPlan::Multipart(fields) => {
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        " \\\n  -F '{}=@{}'",
                        esc_shell(&f.key),
                        esc_shell(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        " \\\n  -F '{}={}'",
                        esc_shell(&f.key),
                        esc_shell(&f.value)
                    ));
                }
            }
        }
    }
    code.push('\n');
    code
}

/// wget。
fn shell_wget(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str(&format!("wget -O - --method={} --timeout=30", method));
    for h in enabled_headers(req) {
        code.push_str(&format!(
            " \\\n  --header='{}: {}'",
            esc_shell(&h.key),
            esc_shell(&h.value)
        ));
    }
    if let Some(ct) = auto_media_type(req, plan) {
        code.push_str(&format!(
            " \\\n  --header='Content-Type: {}'",
            esc_shell(&ct)
        ));
    }
    match plan {
        BodyPlan::None => {}
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!(" \\\n  --body-data='{}'", esc_shell(text)));
        }
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(" \\\n  --body-file='{}'", esc_shell(path)));
        }
        BodyPlan::Multipart(fields) => {
            code.push_str(" \\\n  # wget 不支持 multipart，建议用 curl 版本；部件如下：\n");
            code.push_str(&multipart_note(fields, "  #"));
        }
    }
    code.push_str(&format!(" \\\n  '{}'\n", esc_shell(url)));
    code
}

// ===================== PowerShell =====================

/// PowerShell Invoke-RestMethod。
fn powershell(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str(&format!("$uri = '{}'\n", esc_ps(url)));
    code.push_str("$headers = @{\n");
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "    '{}' = '{}'\n",
            esc_ps(&h.key),
            esc_ps(&h.value)
        ));
    }
    code.push_str("}\n\n");
    let mut params = vec![
        format!("    -Method {}", title_case(method)),
        "    -Uri $uri".to_string(),
        "    -Headers $headers".to_string(),
    ];
    match plan {
        BodyPlan::None => {}
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!("$body = @'\n{}\n'@\n\n", text));
            params.push("    -Body $body".to_string());
        }
        BodyPlan::File { path, .. } => {
            params.push(format!("    -InFile '{}'", esc_ps(path)));
        }
        BodyPlan::Multipart(fields) => {
            let mut pairs = Vec::new();
            for f in fields {
                if f.is_file {
                    pairs.push(format!(
                        "        '{}' = Get-Item -Path '{}'",
                        esc_ps(&f.key),
                        esc_ps(f.value.trim())
                    ));
                } else {
                    pairs.push(format!(
                        "        '{}' = '{}'",
                        esc_ps(&f.key),
                        esc_ps(&f.value)
                    ));
                }
            }
            code.push_str(&format!("$form = @{{\n{}\n}}\n\n", pairs.join("\n")));
            params.push("    -Form $form".to_string());
        }
    }
    if let Some(ct) = auto_media_type(req, plan)
        && !matches!(plan, BodyPlan::Multipart(_))
    {
        params.push(format!("    -ContentType '{}'", esc_ps(&ct)));
    }
    code.push_str(&format!(
        "$response = Invoke-RestMethod \\\n{} \\\n    -TimeoutSec 30\n\n$response | ConvertTo-Json -Depth 10\n",
        params.join(" \\\n")
    ));
    code
}

fn title_case(m: &str) -> String {
    let mut c = m.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

// ===================== C# =====================

/// C# HttpClient。
fn csharp_http_client(req: &RequestItem, url: &str, plan: &BodyPlan, method: &str) -> String {
    let mut code = String::new();
    code.push_str("using System;\nusing System.IO;\nusing System.Net.Http;\nusing System.Text;\nusing System.Threading.Tasks;\n\n");
    code.push_str("class ApiCall\n{\n    static async Task Main()\n    {\n");
    code.push_str(
        "        using var client = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };\n",
    );
    code.push_str("        var request = new HttpRequestMessage(new HttpMethod(\"");
    code.push_str(method);
    code.push_str("\"), \"");
    code.push_str(&esc(url));
    code.push_str("\");\n");
    for h in enabled_headers(req) {
        code.push_str(&format!(
            "        request.Headers.TryAddWithoutValidation(\"{}\", \"{}\");\n",
            esc(&h.key),
            esc(&h.value)
        ));
    }
    match plan {
        BodyPlan::None => {}
        BodyPlan::Text { text, .. } => {
            let ct = auto_media_type(req, plan).unwrap_or_else(|| "text/plain".into());
            code.push_str(&format!(
                "        request.Content = new StringContent(\"{}\", Encoding.UTF8, \"{}\");\n",
                esc(text),
                esc(&ct)
            ));
        }
        BodyPlan::File { path, media_type } => {
            code.push_str(&format!(
                "        var stream = File.OpenRead(\"{}\");\n        request.Content = new StreamContent(stream);\n        request.Content.Headers.ContentType = new System.Net.Http.Headers.MediaTypeHeaderValue(\"{}\");\n",
                esc(path),
                esc(media_type)
            ));
        }
        BodyPlan::Multipart(fields) => {
            code.push_str("        var form = new MultipartFormDataContent();\n");
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "        form.Add(new StreamContent(File.OpenRead(\"{}\")), \"{}\", \"{}\");\n",
                        esc(f.value.trim()),
                        esc(&f.key),
                        esc(&file_name(&f.value))
                    ));
                } else {
                    code.push_str(&format!(
                        "        form.Add(new StringContent(\"{}\"), \"{}\");\n",
                        esc(&f.value),
                        esc(&f.key)
                    ));
                }
            }
            code.push_str("        request.Content = form;\n");
        }
    }
    code.push_str("\n        var response = await client.SendAsync(request);\n");
    code.push_str("        Console.WriteLine((int)response.StatusCode);\n");
    code.push_str(
        "        Console.WriteLine(await response.Content.ReadAsStringAsync());\n    }\n}\n",
    );
    code
}

// ===================== PHP =====================

/// PHP cURL。
fn php_curl(req: &RequestItem, url: &str, plan: &BodyPlan) -> String {
    let mut code = String::new();
    code.push_str("<?php\n\n$ch = curl_init();\n\n");
    code.push_str(&format!(
        "curl_setopt($ch, CURLOPT_URL, '{}');\n",
        esc_shell(url)
    ));
    code.push_str("curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);\n");
    code.push_str("curl_setopt($ch, CURLOPT_TIMEOUT, 30);\n");
    code.push_str(&format!(
        "curl_setopt($ch, CURLOPT_CUSTOMREQUEST, '{}');\n",
        method_upper(req)
    ));
    let mut headers: Vec<String> = enabled_headers(req)
        .iter()
        .map(|h| format!("'{}: {}'", esc_shell(&h.key), esc_shell(&h.value)))
        .collect();
    if let Some(ct) = auto_media_type(req, plan) {
        headers.push(format!("'Content-Type: {}'", esc_shell(&ct)));
    }
    if !headers.is_empty() {
        code.push_str(&format!(
            "curl_setopt($ch, CURLOPT_HTTPHEADER, [\n    {},\n]);\n",
            headers.join(",\n    ")
        ));
    }
    match plan {
        BodyPlan::None => {}
        BodyPlan::Text { text, .. } => {
            code.push_str(&format!(
                "curl_setopt($ch, CURLOPT_POSTFIELDS, '{}');\n",
                esc_shell(text)
            ));
        }
        BodyPlan::File { path, .. } => {
            code.push_str(&format!(
                "curl_setopt($ch, CURLOPT_POSTFIELDS, file_get_contents('{}'));\n",
                esc_shell(path)
            ));
        }
        BodyPlan::Multipart(fields) => {
            code.push_str("$fields = [\n");
            for f in fields {
                if f.is_file {
                    code.push_str(&format!(
                        "    '{}' => new CURLFile('{}'),\n",
                        esc_shell(&f.key),
                        esc_shell(f.value.trim())
                    ));
                } else {
                    code.push_str(&format!(
                        "    '{}' => '{}',\n",
                        esc_shell(&f.key),
                        esc_shell(&f.value)
                    ));
                }
            }
            code.push_str("];\ncurl_setopt($ch, CURLOPT_POSTFIELDS, $fields);\n");
        }
    }
    code.push_str("\n$response = curl_exec($ch);\nif ($response === false) {\n    echo curl_error($ch);\n} else {\n    echo curl_getinfo($ch, CURLINFO_HTTP_CODE), \"\\n\";\n    echo $response, \"\\n\";\n}\ncurl_close($ch);\n");
    code
}
