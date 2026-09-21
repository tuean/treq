use serde_json::json;
use treq_core::json::{find_path_by_value, query_path};

#[test]
fn dotted_path_and_index() {
    let v = json!({"a": {"b": [1, 2, 3]}});
    let hits = query_path(&v, "$.a.b[1]").unwrap();
    assert_eq!(hits, vec![&json!(2)]);
}

#[test]
fn wildcard_expands_objects_and_arrays() {
    let v = json!({"store": {"books": [{"author": "A"}, {"author": "B"}]}});
    let hits = query_path(&v, "$.store.books[*].author").unwrap();
    assert_eq!(hits, vec![&json!("A"), &json!("B")]);

    let o = json!({"m": {"x": 1, "y": 2}});
    assert_eq!(query_path(&o, "$.m.*").unwrap().len(), 2);
}

#[test]
fn missing_key_yields_empty_not_error() {
    let v = json!({"a": 1});
    assert!(query_path(&v, "$.nope.deep").unwrap().is_empty());
    assert_eq!(query_path(&v, "$").unwrap(), vec![&v]);
}

#[test]
fn bad_paths_error() {
    let v = json!({"a": 1});
    assert!(query_path(&v, "").is_err());
    assert!(query_path(&v, "$.a[").is_err());
    assert!(query_path(&v, "$.").is_err());
}

#[test]
fn bracket_quoted_key() {
    let v = json!({"a b": {"c": true}});
    assert_eq!(query_path(&v, "$[\"a b\"].c").unwrap(), vec![&json!(true)]);
}

#[test]
fn find_path_by_value_walks_in_document_order() {
    let v = json!({"data": {"token": "abc", "items": [{"id": 7}, {"id": 8}]}, "n": 42});
    assert_eq!(
        find_path_by_value(&v, "abc").as_deref(),
        Some("$.data.token")
    );
    assert_eq!(
        find_path_by_value(&v, "8").as_deref(),
        Some("$.data.items[1].id")
    );
    assert_eq!(find_path_by_value(&v, "42").as_deref(), Some("$.n"));
    assert_eq!(find_path_by_value(&v, "nope"), None);
    // 拿对象/数组去反查没有意义：不返回（这里用它的字面量也查不到）
    assert_eq!(find_path_by_value(&v, "{\"id\":7}"), None);
}

#[test]
fn find_path_quotes_awkward_keys() {
    let v = json!({"a b": {"c.d": {"ok": true}}});
    assert_eq!(
        find_path_by_value(&v, "true").as_deref(),
        Some("$[\"a b\"][\"c.d\"].ok")
    );
}

#[test]
fn find_path_result_is_queryable() {
    let v = json!({"deep": [{"名单": [1, 2, 3]}]});
    let path = find_path_by_value(&v, "3").unwrap();
    assert_eq!(path, "$.deep[0].名单[2]");
    assert_eq!(query_path(&v, &path).unwrap(), vec![&json!(3)]);
}
