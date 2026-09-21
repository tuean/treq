use treq_core::BodyKind;
use treq_core::curl::{parse_curl, tokenize};

#[test]
fn tokenize_quotes_and_escapes() {
    assert_eq!(
        tokenize(r#"curl -H 'Content-Type: application/json' -d '{"a":1}' "http://x.com/a\ b""#),
        vec![
            "curl",
            "-H",
            "Content-Type: application/json",
            "-d",
            "{\"a\":1}",
            "http://x.com/a\\ b",
        ]
    );
    // bash 语义：双引号内 \ 只转义 $ ` " \ 换行；\ 空格保留反斜杠
    assert_eq!(
        tokenize(r#"echo "a\"b" 'c\ d'"#),
        vec!["echo", "a\"b", "c\\ d"]
    );
}

#[test]
fn parse_basic_get() {
    let p = parse_curl("curl -sSL 'https://api.example.com/v1/users/1?x=1'").unwrap();
    assert_eq!(p.method, "GET");
    assert_eq!(p.url, "https://api.example.com/v1/users/1?x=1");
    assert_eq!(p.body_kind, BodyKind::None);
}

#[test]
fn parse_post_json_with_header() {
    let p = parse_curl(
        "curl -X POST -H 'Content-Type: application/json' -d '{\"a\":1}' https://api.example.com/x",
    )
    .unwrap();
    assert_eq!(p.method, "POST");
    assert_eq!(p.body_content, "{\"a\":1}");
    assert_eq!(p.body_kind, BodyKind::Json);
    assert_eq!(p.headers.len(), 1);
    assert_eq!(p.headers[0].key, "Content-Type");
    assert_eq!(p.headers[0].value, "application/json");
}

#[test]
fn parse_data_implies_post_and_form_default() {
    let p = parse_curl("curl -d 'a=1' -d 'b=2' http://x.com/form").unwrap();
    assert_eq!(p.method, "POST");
    assert_eq!(p.body_content, "a=1&b=2");
    assert_eq!(p.body_kind, BodyKind::Form);
}

#[test]
fn parse_json_flag() {
    let p = parse_curl("curl --json '{\"k\":\"v\"}' http://x.com/j").unwrap();
    assert_eq!(p.method, "POST");
    assert_eq!(p.body_kind, BodyKind::Json);
    assert_eq!(p.body_content, "{\"k\":\"v\"}");
}

#[test]
fn parse_form_flags_multipart() {
    let p =
        parse_curl("curl -X POST https://x.com/upload -F name=treq -F file=@/tmp/a.png").unwrap();
    assert_eq!(p.method, "POST");
    assert_eq!(p.body_kind, BodyKind::Multipart);
    assert_eq!(p.body_content, "name=treq\nfile=@/tmp/a.png");
}

#[test]
fn parse_user_basic_auth() {
    let p = parse_curl("curl -u alice:secret http://x.com/").unwrap();
    let auth = p.headers.iter().find(|h| h.key == "Authorization").unwrap();
    assert_eq!(auth.value, "Basic YWxpY2U6c2VjcmV0");
}

#[test]
fn parse_get_mode_appends_query() {
    let p = parse_curl("curl -G -d 'q=hello' 'http://x.com/search?lang=zh'").unwrap();
    assert_eq!(p.method, "GET");
    assert_eq!(p.url, "http://x.com/search?lang=zh&q=hello");
    assert_eq!(p.body_kind, BodyKind::None);
}

#[test]
fn parse_head_flag() {
    let p = parse_curl("curl -I http://x.com/").unwrap();
    assert_eq!(p.method, "HEAD");
}

#[test]
fn parse_curl_omitted_url_form() {
    let p = parse_curl("http://x.com/a").unwrap();
    assert_eq!(p.method, "GET");
    assert_eq!(p.url, "http://x.com/a");
}

#[test]
fn parse_errors() {
    assert!(parse_curl("").is_err());
    assert!(parse_curl("wc -l file").is_err());
    assert!(parse_curl("curl -H 'X: 1'").is_err()); // 无 URL
}

#[test]
fn parse_unknown_flags_ignored() {
    let p = parse_curl("curl -sS --compressed -H 'X-T: 1' --max-time 5 https://x.com/").unwrap();
    assert_eq!(p.url, "https://x.com/");
    assert_eq!(p.headers.len(), 1);
}
