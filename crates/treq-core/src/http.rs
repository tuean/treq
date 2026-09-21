use crate::models::*;
use std::error::Error as _;
use std::fmt;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
/// 流式通道类型：调用方（app）不用直接依赖 tokio
pub use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender as StreamTx, unbounded_channel};

/// reqwest 0.13 需要 tokio 运行时；gpui 的 executor 不是 tokio，
/// 所以请求统一提交到共享 tokio runtime 上执行。
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("failed to build tokio runtime")
    })
}

#[derive(Debug, Clone)]
pub enum HttpError {
    Timeout,
    Connect(String),
    Other(String),
}

/// 用户常把 URL 写成 `example.com/api`（浏览器地址栏习惯，没协议头），
/// reqwest 会直接报 `builder error ... URL scheme is not allowed`，很费解。
/// 这里在发送前补上 `http://`：已经有 `scheme://` 的、以 `{`（变量模板）开头的、
/// 协议相对 `//host` 的都不动。
pub fn normalize_url(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() || u.starts_with('{') || u.starts_with("//") {
        return u.to_string();
    }
    // 判定「已经有协议」只看 `://`：`127.0.0.1:8321/x`、`localhost:3000` 里的冒号是端口，
    // 不是协议（只按冒号判断会把它们误当成有协议而不补）。
    let head = u.split(['?', '#']).next().unwrap_or(u);
    if head.contains("://") {
        return u.to_string();
    }
    format!("http://{u}")
}

/// 连接类错误的原因分类：reqwest 的报错是一长串英文链路，用户看不懂，
/// 这里按链路里的关键字归因，给一句人话 + 一条排查建议。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectKind {
    Refused,
    Dns,
    TimedOut,
    Tls,
    Proxy,
    Other,
}

impl ConnectKind {
    /// 从 reqwest 的英文错误链里认原因（关键字全部小写匹配）。
    pub fn of(raw: &str) -> Self {
        let s = raw.to_lowercase();
        // 代理连不上也算连接问题，但要单独提示（用户多半是 Clash 没开）
        if s.contains("proxy") && (s.contains("connect") || s.contains("refused")) {
            return ConnectKind::Proxy;
        }
        if s.contains("connection refused") || s.contains("os error 61") {
            return ConnectKind::Refused;
        }
        if s.contains("dns")
            || s.contains("lookup address")
            || s.contains("name or service not known")
            || s.contains("failed to resolve")
        {
            return ConnectKind::Dns;
        }
        if s.contains("timed out") || s.contains("timeout") || s.contains("os error 60") {
            return ConnectKind::TimedOut;
        }
        if s.contains("certificate")
            || s.contains("tls")
            || s.contains("ssl")
            || s.contains("handshake")
        {
            return ConnectKind::Tls;
        }
        ConnectKind::Other
    }

    /// 一句人话。
    pub fn label(self) -> &'static str {
        match self {
            ConnectKind::Refused => "目标端口拒绝连接",
            ConnectKind::Dns => "域名解析失败",
            ConnectKind::TimedOut => "连接超时（网络不通或目标不可达）",
            ConnectKind::Tls => "TLS 握手/证书校验失败",
            ConnectKind::Proxy => "连不上代理",
            ConnectKind::Other => "连接出错",
        }
    }

    /// 排查建议。
    pub fn hint(self) -> &'static str {
        match self {
            ConnectKind::Refused => "服务没起或地址/端口写错了",
            ConnectKind::Dns => "检查域名拼写，或 hosts / DNS / 是否需要走代理",
            ConnectKind::TimedOut => "检查网络；内网地址可能需要连对 VPN",
            ConnectKind::Tls => "证书自签名或过期；确认站点协议是 http 还是 https",
            ConnectKind::Proxy => "代理没开或端口不对，可在「设置 → 网络」里改",
            ConnectKind::Other => "展开详情看原始错误",
        }
    }
}

/// 从英文链路里抠出最有用的一小段（最后两跳），给「详情」用。
pub fn conn_detail(raw: &str) -> String {
    let parts: Vec<&str> = raw
        .split(" -> ")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() > 2 {
        parts[parts.len() - 2..].join(" → ")
    } else {
        parts.join(" → ")
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Timeout => write!(f, "等待响应超时（可在「设置 → 网络」里调大超时）"),
            HttpError::Connect(m) => {
                let kind = ConnectKind::of(m);
                write!(f, "连接失败：{} —— {}", kind.label(), kind.hint())
            }
            HttpError::Other(m) => write!(f, "请求失败：{}", m),
        }
    }
}

impl HttpError {
    /// 原始英文错误链：界面上折进「详情」，排查时给机器/搜索引擎看。
    pub fn raw(&self) -> Option<&str> {
        match self {
            HttpError::Timeout => None,
            HttpError::Connect(m) | HttpError::Other(m) => Some(m),
        }
    }
}
impl std::error::Error for HttpError {}

#[derive(Debug, Clone)]
pub struct ResponseData {
    pub status: Option<u16>,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub duration: Duration,
    pub error: Option<HttpError>,
}

impl ResponseData {
    pub fn empty() -> Self {
        ResponseData {
            status: None,
            status_text: String::new(),
            headers: vec![],
            body: vec![],
            duration: Duration::from_millis(0),
            error: None,
        }
    }
}

/// 流式响应事件：SSE（text/event-stream）与普通请求共用同一条通道，
/// 普通请求就是 Head + 一个 Chunk + Done，SSE 会持续发 Chunk。
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Head {
        status: Option<u16>,
        status_text: String,
        headers: Vec<(String, String)>,
    },
    Chunk(Vec<u8>),
    Done {
        error: Option<HttpError>,
        duration_ms: u64,
    },
}

/// 响应是不是事件流（SSE）：服务端声明 `text/event-stream` 就走流式读取。
pub fn is_event_stream(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("content-type")
            && v.trim_start()
                .to_ascii_lowercase()
                .starts_with("text/event-stream")
    })
}

/// reqwest 的错误信息一层套一层，把整条链拼起来（连接类错误的原因只在下层，
/// 只看顶层会是「error sending request for url (...)",归因不出来）。
fn chain_of(e: &reqwest::Error) -> String {
    let mut chain = e.to_string();
    let mut next = e.source();
    while let Some(s) = next {
        chain.push_str(&format!(" -> {}", s));
        next = s.source();
    }
    chain
}

fn err_of(e: reqwest::Error) -> HttpError {
    if e.is_timeout() {
        HttpError::Timeout
    } else if e.is_builder() {
        // 组装阶段就失败了：多半是 URL 写错（协议不支持、字符非法）
        let chain = chain_of(&e);
        let hint = if chain.contains("scheme") {
            "URL 的协议不支持，只支持 http / https（可省掉 https:// 之外的其他前缀）"
        } else if chain.contains("relative URL") || chain.contains("relative url") {
            "URL 不完整，需要带域名或 IP（例如 http://example.com/api）"
        } else {
            "URL 不合法，检查是否少了 http:// 或 https://、域名里有没有非法字符"
        };
        HttpError::Other(format!("{hint}。原始错误：{chain}"))
    } else if e.is_connect() {
        let chain = chain_of(&e);
        HttpError::Connect(chain)
    } else {
        HttpError::Other(chain_of(&e))
    }
}

/// 组装请求：头 / 默认 Content-Type / body（一次性发送与流式发送共用，避免两套逻辑漂移）。
/// 按超时/代理建 client。代理为空 = 直连；支持 http(s):// 与 socks5://（Clash 之类）。
/// `stream` 为 true 时不设总超时（SSE 可以开很久），只留连接超时。
fn build_client(
    timeout: Duration,
    stream: bool,
    proxy: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    let mut b = if stream {
        reqwest::Client::builder().connect_timeout(timeout)
    } else {
        reqwest::Client::builder().timeout(timeout)
    };
    if let Some(p) = proxy.map(str::trim).filter(|p| !p.is_empty()) {
        b = b.proxy(parse_proxy(p)?);
    }
    b.build()
        .map_err(|e| HttpError::Other(format!("failed to build http client: {e}")))
}

/// 请求超时上限（秒）：够长的内部慢接口，但别让人输 10 个小时。
pub const MAX_TIMEOUT_SEC: u64 = 600;

/// 解析设置里的超时输入：空 = 用默认值；1..=600 之外或不是数字都报错。
pub fn parse_timeout(raw: &str, default_sec: u64) -> Result<u64, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(default_sec);
    }
    match t.parse::<u64>() {
        Ok(n) if (1..=MAX_TIMEOUT_SEC).contains(&n) => Ok(n),
        Ok(n) => Err(format!("超时只能是 1–{} 秒（收到 {}）", MAX_TIMEOUT_SEC, n)),
        Err(_) => Err(format!("超时得是整数秒数（收到「{}」）", t)),
    }
}

/// 只校验不建连接（对话框里保存前先自查，错值别等到发请求才报）。
pub fn check_proxy(p: &str) -> Result<(), String> {
    parse_proxy(p).map(|_| ()).map_err(|e| match e {
        HttpError::Other(m) => m, // 去掉 "request failed: " 前缀，对话框里只显示原因
        other => other.to_string(),
    })
}

/// 解析代理串：没写协议就默认 http://（用户常只填 `127.0.0.1:7897`），
/// 明显的错值（空 host、空格、非 ASCII）直接报错——比静默直连好排查。
fn parse_proxy(p: &str) -> Result<reqwest::Proxy, HttpError> {
    let p = p.trim();
    let full = if p.contains("://") {
        p.to_string()
    } else {
        format!("http://{p}")
    };
    let bad = |msg: &str| HttpError::Other(format!("代理地址无效（{}）{}", p, msg));
    let Some((scheme, rest)) = full.split_once("://") else {
        return Err(bad("：需要形如 http://127.0.0.1:7897"));
    };
    if scheme.is_empty() {
        return Err(bad("：缺少协议，例如 http://127.0.0.1:7897"));
    }
    let host = rest.trim_end_matches('/');
    if host.is_empty() {
        return Err(bad("：缺少主机名或端口"));
    }
    if !host.is_ascii() || host.contains(char::is_whitespace) {
        return Err(bad("：主机名只能是 ASCII（英文/数字/点号/冒号）"));
    }
    reqwest::Proxy::all(&full).map_err(|e| bad(&format!("：{e}")))
}

fn build_request(
    client: &reqwest::Client,
    req: &RequestItem,
) -> Result<reqwest::RequestBuilder, HttpError> {
    let url = normalize_url(&crate::vars::apply_params(&req.url, &req.params));
    let method = reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, &url);
    for h in &req.headers {
        if h.enabled
            && !h.key.is_empty()
            && let (Ok(k), Ok(v)) = (
                reqwest::header::HeaderName::from_bytes(h.key.as_bytes()),
                reqwest::header::HeaderValue::from_str(&h.value),
            )
        {
            builder = builder.header(k, v);
        }
    }
    // 默认 Content-Type 仅在用户未显式设置时添加（reqwest 的 header() 是 append 语义，
    // 无条件加会出现两个 Content-Type 头）
    let user_ct = req
        .headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type"));
    let mut default_ct: Option<String> = match req.body.kind {
        BodyKind::None => None,
        BodyKind::Json => Some("application/json".to_string()),
        BodyKind::Text => Some("text/plain; charset=utf-8".to_string()),
        BodyKind::Form => Some("application/x-www-form-urlencoded".to_string()),
        // multipart 的 Content-Type 带 boundary，在下面按实际 boundary 生成
        BodyKind::Multipart => None,
        BodyKind::File => Some(guess_content_type(&req.body.content).to_string()),
        BodyKind::Raw => None,
    };
    let mut body_bytes: Option<Vec<u8>> = None;
    match req.body.kind {
        BodyKind::None => {}
        BodyKind::Json | BodyKind::Text | BodyKind::Form | BodyKind::Raw => {
            body_bytes = Some(req.body.content.as_bytes().to_vec());
        }
        BodyKind::File => {
            let path = req.body.content.trim();
            if path.is_empty() {
                return Err(HttpError::Other("file body: 文件路径为空".to_string()));
            }
            match std::fs::read(path) {
                Ok(bytes) => body_bytes = Some(bytes),
                Err(e) => {
                    return Err(HttpError::Other(format!(
                        "file body: 读取 {} 失败: {}",
                        path, e
                    )));
                }
            }
        }
        BodyKind::Multipart => {
            let boundary = format!("----treq{}", uuid::Uuid::new_v4().simple());
            let fields = req.body.multipart_fields();
            match build_multipart(&fields, &boundary) {
                Ok(bytes) => {
                    body_bytes = Some(bytes);
                    default_ct = Some(format!("multipart/form-data; boundary={}", boundary));
                }
                Err(e) => return Err(HttpError::Other(e)),
            }
        }
    }
    if !user_ct && let Some(ct) = default_ct {
        builder = builder.header("Content-Type", ct);
    }
    if let Some(bytes) = body_bytes {
        builder = builder.body(bytes);
    }
    Ok(builder)
}

/// 流式发送。响应头一到就发 `Head`，正文边收边发 `Chunk`（SSE 不用等整个响应结束）；
/// 返回值是累计后的完整结果，调用方最终以它为准。
/// `cancel` 置位后 1 秒内中断连接；接收端丢弃 channel 也会中断。
pub async fn send_stream(
    req: &RequestItem,
    timeout: Duration,
    proxy: Option<&str>,
    tx: StreamTx<StreamEvent>,
    cancel: Arc<AtomicBool>,
) -> ResponseData {
    let req = req.clone();
    let proxy = proxy.map(|p| p.to_string());
    let handle = tokio_runtime()
        .handle()
        .spawn(async move { stream_inner(&req, timeout, proxy.as_deref(), tx, cancel).await });
    handle.await.unwrap_or_else(|_| ResponseData {
        error: Some(HttpError::Other("request task panicked".into())),
        ..ResponseData::empty()
    })
}

async fn stream_inner(
    req: &RequestItem,
    timeout: Duration,
    proxy: Option<&str>,
    tx: StreamTx<StreamEvent>,
    cancel: Arc<AtomicBool>,
) -> ResponseData {
    let started = std::time::Instant::now();
    let fail = |error: HttpError| ResponseData {
        error: Some(error),
        ..ResponseData::empty()
    };
    // 流式请求不能套总超时（SSE 可以开很久），只保留连接超时
    let client = match build_client(timeout, true, proxy) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let builder = match build_request(&client, req) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    // 等响应头也受 `timeout` 约束（服务端卡住不回包不能无限等）；
    // 收到头之后：事件流不设限，普通响应由下面的 body 超时管。
    let mut resp = match tokio::time::timeout(timeout, builder.send()).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return fail(err_of(e)),
        Err(_) => return fail(HttpError::Timeout),
    };
    let status = resp.status().as_u16();
    let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
    let headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect();
    let _ = tx.send(StreamEvent::Head {
        status: Some(status),
        status_text: status_text.clone(),
        headers: headers.clone(),
    });

    let mut body: Vec<u8> = Vec::new();
    let mut error: Option<HttpError> = None;
    if is_event_stream(&headers) {
        loop {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            // 1 秒的空转超时：让「停止」最多 1 秒内生效
            match tokio::time::timeout(Duration::from_secs(1), resp.chunk()).await {
                Err(_) => continue,
                Ok(Ok(Some(chunk))) => {
                    if tx.send(StreamEvent::Chunk(chunk.to_vec())).is_err() {
                        break;
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(Ok(None)) => break,
                Ok(Err(e)) => {
                    error = Some(err_of(e));
                    break;
                }
            }
        }
    } else {
        match tokio::time::timeout(timeout, resp.bytes()).await {
            Ok(Ok(b)) => {
                let _ = tx.send(StreamEvent::Chunk(b.to_vec()));
                body = b.to_vec();
            }
            Ok(Err(e)) => error = Some(HttpError::Other(format!("read body: {}", e))),
            Err(_) => error = Some(HttpError::Timeout),
        }
    }
    let duration = started.elapsed();
    let _ = tx.send(StreamEvent::Done {
        error: error.clone(),
        duration_ms: duration.as_millis() as u64,
    });
    ResponseData {
        status: Some(status),
        status_text,
        headers,
        body,
        duration,
        error,
    }
}

pub async fn send(req: &RequestItem, timeout: Duration, proxy: Option<&str>) -> ResponseData {
    // 在共享 tokio runtime 上执行真实请求，再回到调用方的 executor await
    let req = req.clone();
    let proxy = proxy.map(|p| p.to_string());
    let handle = tokio_runtime().handle().spawn(async move {
        // 此闭包内的 await 都在 tokio 线程池上
        crate::http::send_inner(&req, timeout, proxy.as_deref()).await
    });
    handle.await.unwrap_or_else(|_| ResponseData {
        status: None,
        status_text: String::new(),
        headers: vec![],
        body: vec![],
        duration: Duration::from_millis(0),
        error: Some(HttpError::Other("request task panicked".into())),
    })
}

async fn send_inner(req: &RequestItem, timeout: Duration, proxy: Option<&str>) -> ResponseData {
    let started = std::time::Instant::now();
    let fail = |error: HttpError| ResponseData {
        error: Some(error),
        ..ResponseData::empty()
    };
    let client = match build_client(timeout, false, proxy) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let builder = match build_request(&client, req) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    match builder.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_string(),
                        v.to_str().unwrap_or("<binary>").to_string(),
                    )
                })
                .collect();
            match resp.bytes().await {
                Ok(b) => ResponseData {
                    status: Some(status),
                    status_text,
                    headers,
                    body: b.to_vec(),
                    duration: started.elapsed(),
                    error: None,
                },
                Err(e) => ResponseData {
                    status: Some(status),
                    status_text,
                    headers,
                    body: vec![],
                    duration: started.elapsed(),
                    error: Some(HttpError::Other(format!("read body: {}", e))),
                },
            }
        }
        Err(e) => fail(err_of(e)),
    }
}

/// 从文件扩展名猜测 Content-Type（未知回退 application/octet-stream）。
pub fn guess_content_type(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("json") => "application/json",
        Some("txt") | Some("log") => "text/plain; charset=utf-8",
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("xml") => "application/xml",
        Some("csv") => "text/csv; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        Some("mp3") => "audio/mpeg",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// 根据结构化字段构造 multipart/form-data 请求体。
fn build_multipart(fields: &[FormField], boundary: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for field in fields {
        if !field.enabled || field.key.is_empty() {
            continue;
        }
        if field.is_file {
            let path = field.value.trim();
            if path.is_empty() {
                return Err(format!("multipart: 字段 {} 的文件路径为空", field.key));
            }
            let data =
                std::fs::read(path).map_err(|e| format!("multipart: 读取 {} 失败: {}", path, e))?;
            let filename = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            out.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            out.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    field.key, filename
                )
                .as_bytes(),
            );
            out.extend_from_slice(
                format!("Content-Type: {}\r\n\r\n", guess_content_type(path)).as_bytes(),
            );
            out.extend_from_slice(&data);
            out.extend_from_slice(b"\r\n");
        } else {
            out.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            out.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                    field.key
                )
                .as_bytes(),
            );
            out.extend_from_slice(field.value.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
    }
    out.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    Ok(out)
}
