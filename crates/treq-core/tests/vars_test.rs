use std::collections::BTreeMap;
use treq_core::vars::{merge_env, resolve, resolve_request};
use treq_core::*;

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn resolve_simple_and_nested() {
    let v = vars(&[("baseUrl", "https://api.example.com"), ("userId", "42")]);
    assert_eq!(
        resolve("{{ baseUrl }}/users/{{ userId }}", &v),
        "https://api.example.com/users/42"
    );
}

#[test]
fn resolve_keeps_unknown_literal() {
    let v = vars(&[]);
    assert_eq!(resolve("a={{ missing }}", &v), "a={{ missing }}");
}

#[test]
fn resolve_handles_spacing_and_multiple_on_line() {
    let v = vars(&[("x", "1"), ("y", "2")]);
    assert_eq!(resolve("{{x}},{{  y  }}", &v), "1,2");
}

#[test]
fn merge_env_overrides_base() {
    let base = Environment {
        id: "b".into(),
        name: "Base".into(),
        variables: vars(&[("baseUrl", "https://prod.example.com"), ("key", "k1")]),
    };
    let dev = Environment {
        id: "d".into(),
        name: "dev".into(),
        variables: vars(&[("baseUrl", "http://localhost:8080")]),
    };
    let merged = merge_env(&base, Some(&dev));
    assert_eq!(merged.get("baseUrl").unwrap(), "http://localhost:8080");
    assert_eq!(merged.get("key").unwrap(), "k1");
    assert_eq!(merged.get("nope"), None);
}

#[test]
fn resolve_request_fields() {
    let req = RequestItem {
        id: "i".into(),
        name: "n".into(),
        method: "POST".into(),
        url: "{{ baseUrl }}/x".into(),
        params: vec![Kv {
            key: "q".into(),
            value: "{{ userId }}".into(),
            enabled: true,
            description: String::new(),
        }],
        headers: vec![Kv {
            key: "X-K".into(),
            value: "{{ key }}".into(),
            enabled: true,
            description: String::new(),
        }],
        body: Body {
            kind: BodyKind::Json,
            content: "{\"u\":\"{{ userId }}\"}".into(),
            form_data: Vec::new(),
        },
        description: String::new(),
        docs_open: true,
        auth: None,
    };
    let v = vars(&[("baseUrl", "http://h"), ("userId", "7"), ("key", "v")]);
    let r = resolve_request(&req, &v);
    assert_eq!(r.url, "http://h/x");
    assert_eq!(r.params[0].value, "7");
    assert_eq!(r.headers[0].value, "v");
    assert_eq!(r.body.content, "{\"u\":\"7\"}");
    assert_eq!(req.url, "{{ baseUrl }}/x");
}
