//! 请求认证：Bearer / Basic / API Key。
//!
//! 存的是**模板**（可以写 `{{ token }}`），发送前用当前环境变量解析成真值再拼进请求。
//! 规则：用户自己写了同名头/参数就不覆盖（手改优先），所以迁移进来的老请求行为不变。

use crate::models::{Kv, RequestItem};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// API Key 放哪儿。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyIn {
    Header,
    Query,
}

/// 一条请求的认证方式；`None` 变体 = 明确不用（老请求缺字段也是不用）。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Auth {
    #[default]
    None,
    Bearer {
        #[serde(default)]
        token: String,
        /// 前缀，默认 `Bearer`（有些服务要 `Token` 或 `Token ` 这类自定义）
        #[serde(default)]
        prefix: String,
    },
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    #[serde(rename = "apikey")]
    ApiKey {
        #[serde(default)]
        key: String,
        #[serde(default)]
        value: String,
        #[serde(default = "default_key_in")]
        location: KeyIn,
    },
}

fn default_key_in() -> KeyIn {
    KeyIn::Header
}

impl Auth {
    /// 界面上的类型名（i18n key 后缀）。
    pub fn kind_id(&self) -> &'static str {
        match self {
            Auth::None => "none",
            Auth::Bearer { .. } => "bearer",
            Auth::Basic { .. } => "basic",
            Auth::ApiKey { .. } => "apikey",
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Auth::None)
    }

    /// 概要（列表/标签用）：`Bearer {{ token }}`、`Basic admin`、`X-Api-Key`。
    pub fn summary(&self) -> String {
        match self {
            Auth::None => String::new(),
            Auth::Bearer { token, prefix } => {
                let p = if prefix.trim().is_empty() {
                    "Bearer"
                } else {
                    prefix.trim()
                };
                if token.is_empty() {
                    format!("{p} （没填 token）")
                } else {
                    format!("{p} {token}")
                }
            }
            Auth::Basic { username, .. } => {
                if username.is_empty() {
                    "Basic （没填用户名）".to_string()
                } else {
                    format!("Basic {username}")
                }
            }
            Auth::ApiKey { key, location, .. } => {
                let where_ = match location {
                    KeyIn::Header => "头",
                    KeyIn::Query => "query",
                };
                if key.is_empty() {
                    format!("API Key（没填名称，放{where_}）")
                } else {
                    format!("{key}（放{where_}）")
                }
            }
        }
    }

    /// 用环境变量把模板里的 `{{ var }}` 换成真值。
    pub fn resolve(&self, vars: &BTreeMap<String, String>) -> Auth {
        let r = |s: &str| crate::vars::resolve(s, vars);
        match self {
            Auth::None => Auth::None,
            Auth::Bearer { token, prefix } => Auth::Bearer {
                token: r(token),
                prefix: r(prefix),
            },
            Auth::Basic { username, password } => Auth::Basic {
                username: r(username),
                password: r(password),
            },
            Auth::ApiKey {
                key,
                value,
                location,
            } => Auth::ApiKey {
                key: r(key),
                value: r(value),
                location: *location,
            },
        }
    }
}

/// 认证表单里的两个输入槽（不同认证方式下的含义不同，见 `field`/`set_field`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthField {
    /// Bearer=token，Basic=用户名，ApiKey=参数名
    A,
    /// Bearer=前缀，Basic=密码，ApiKey=参数值
    B,
}

impl Auth {
    /// 取槽位当前值（没有的返回空串）。
    pub fn field(&self, f: AuthField) -> &str {
        match (self, f) {
            (Auth::Bearer { token, .. }, AuthField::A) => token,
            (Auth::Bearer { prefix, .. }, AuthField::B) => prefix,
            (Auth::Basic { username, .. }, AuthField::A) => username,
            (Auth::Basic { password, .. }, AuthField::B) => password,
            (Auth::ApiKey { key, .. }, AuthField::A) => key,
            (Auth::ApiKey { value, .. }, AuthField::B) => value,
            _ => "",
        }
    }

    /// 写槽位（类型不匹配就忽略）。
    pub fn set_field(&mut self, f: AuthField, v: String) {
        match (self, f) {
            (Auth::Bearer { token, .. }, AuthField::A) => *token = v,
            (Auth::Bearer { prefix, .. }, AuthField::B) => *prefix = v,
            (Auth::Basic { username, .. }, AuthField::A) => *username = v,
            (Auth::Basic { password, .. }, AuthField::B) => *password = v,
            (Auth::ApiKey { key, .. }, AuthField::A) => *key = v,
            (Auth::ApiKey { value, .. }, AuthField::B) => *value = v,
            _ => {}
        }
    }

    /// 换认证方式：把旧类型的主值（token / 用户名 / 参数名）带过去，别让用户重敲。
    pub fn with_kind(&self, kind: &str) -> Auth {
        let primary = self.field(AuthField::A).to_string();
        match kind {
            "bearer" => Auth::Bearer {
                token: primary,
                prefix: String::new(),
            },
            "basic" => Auth::Basic {
                username: primary,
                password: String::new(),
            },
            "apikey" => Auth::ApiKey {
                key: primary,
                value: String::new(),
                location: KeyIn::Header,
            },
            _ => Auth::None,
        }
    }
}

/// Basic 认证的头值（`Basic base64(user:pass)`）。
pub fn basic_header(username: &str, password: &str) -> String {
    use base64::Engine;
    let raw = format!("{}:{}", username, password);
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
    )
}

fn has_header(req: &RequestItem, name: &str) -> bool {
    req.headers
        .iter()
        .any(|h| h.enabled && h.key.eq_ignore_ascii_case(name))
}

fn upsert_header(req: &mut RequestItem, name: &str, value: &str) {
    req.headers.push(Kv {
        key: name.to_string(),
        value: value.to_string(),
        enabled: true,
        description: String::new(),
    });
}

fn upsert_param(req: &mut RequestItem, name: &str, value: &str) {
    req.params.push(Kv {
        key: name.to_string(),
        value: value.to_string(),
        enabled: true,
        description: String::new(),
    });
}

/// 把认证写进请求（头或 query 参数）。
///
/// - Bearer：`Authorization: <prefix> <token>`
/// - Basic：`Authorization: Basic base64(user:pass)`
/// - API Key：按 `location` 放头或 query
///
/// 已经有同名头/参数（用户手写的）时**不覆盖**。返回是否真的写进去了。
pub fn apply(req: &mut RequestItem, auth: &Auth) -> bool {
    match auth {
        Auth::None => false,
        Auth::Bearer { token, prefix } => {
            if token.trim().is_empty() || has_header(req, "Authorization") {
                return false;
            }
            let p = if prefix.trim().is_empty() {
                "Bearer"
            } else {
                prefix.trim()
            };
            upsert_header(req, "Authorization", &format!("{} {}", p, token.trim()));
            true
        }
        Auth::Basic { username, password } => {
            if has_header(req, "Authorization") {
                return false;
            }
            if username.is_empty() && password.is_empty() {
                return false;
            }
            let h = basic_header(username, password);
            upsert_header(req, "Authorization", &h);
            true
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            if key.trim().is_empty() {
                return false;
            }
            let key = key.trim();
            match location {
                KeyIn::Header => {
                    if has_header(req, key) {
                        return false;
                    }
                    upsert_header(req, key, value);
                }
                KeyIn::Query => {
                    let exists = req
                        .params
                        .iter()
                        .any(|p| p.enabled && p.key.eq_ignore_ascii_case(key));
                    if exists {
                        return false;
                    }
                    upsert_param(req, key, value);
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Body, BodyKind};

    fn req() -> RequestItem {
        RequestItem {
            id: "i".into(),
            name: "n".into(),
            method: "GET".into(),
            url: "http://h/".into(),
            params: vec![],
            headers: vec![],
            body: Body {
                kind: BodyKind::None,
                content: String::new(),
                form_data: vec![],
            },
            description: String::new(),
            docs_open: true,
            auth: None,
        }
    }

    #[test]
    fn bearer_sets_authorization() {
        let mut r = req();
        let a = Auth::Bearer {
            token: "abc123".into(),
            prefix: String::new(),
        };
        assert!(apply(&mut r, &a));
        assert_eq!(r.headers[0].key, "Authorization");
        assert_eq!(r.headers[0].value, "Bearer abc123");
        // 自定义前缀
        let mut r2 = req();
        apply(
            &mut r2,
            &Auth::Bearer {
                token: "t".into(),
                prefix: "Token".into(),
            },
        );
        assert_eq!(r2.headers[0].value, "Token t");
        // 没填 token = 什么都不加（别发个空 Authorization）
        let mut r3 = req();
        assert!(!apply(
            &mut r3,
            &Auth::Bearer {
                token: "  ".into(),
                prefix: String::new()
            }
        ));
        assert!(r3.headers.is_empty());
    }

    #[test]
    fn basic_base64() {
        let mut r = req();
        apply(
            &mut r,
            &Auth::Basic {
                username: "admin".into(),
                password: "s3cret".into(),
            },
        );
        assert_eq!(r.headers[0].value, "Basic YWRtaW46czNjcmV0");
        assert_eq!(basic_header("a", ""), "Basic YTo=");
    }

    #[test]
    fn manual_header_wins() {
        let mut r = req();
        r.headers.push(Kv {
            key: "authorization".into(), // 大小写不敏感
            value: "Bearer 手写的".into(),
            enabled: true,
            description: String::new(),
        });
        assert!(!apply(
            &mut r,
            &Auth::Bearer {
                token: "别的".into(),
                prefix: String::new()
            }
        ));
        assert_eq!(r.headers.len(), 1);
        assert_eq!(r.headers[0].value, "Bearer 手写的");
        // 禁用状态的头不算「手写」
        r.headers[0].enabled = false;
        assert!(apply(
            &mut r,
            &Auth::Bearer {
                token: "新的".into(),
                prefix: String::new()
            }
        ));
        assert_eq!(r.headers.len(), 2);
    }

    #[test]
    fn apikey_header_or_query() {
        let mut h = req();
        assert!(apply(
            &mut h,
            &Auth::ApiKey {
                key: "X-Api-Key".into(),
                value: "k1".into(),
                location: KeyIn::Header,
            }
        ));
        assert_eq!(h.headers[0].key, "X-Api-Key");
        assert!(h.params.is_empty());

        let mut q = req();
        assert!(apply(
            &mut q,
            &Auth::ApiKey {
                key: "api_key".into(),
                value: "k2".into(),
                location: KeyIn::Query,
            }
        ));
        assert_eq!(q.params[0].key, "api_key");
        assert_eq!(q.params[0].value, "k2");
        assert!(q.headers.is_empty());
        // 已有同名 query 参数就不动
        assert!(!apply(
            &mut q,
            &Auth::ApiKey {
                key: "api_key".into(),
                value: "覆盖".into(),
                location: KeyIn::Query,
            }
        ));
        assert_eq!(q.params.len(), 1);
    }

    #[test]
    fn resolve_uses_env_vars() {
        let mut vars = BTreeMap::new();
        vars.insert("token".to_string(), "real-token".to_string());
        vars.insert("host".to_string(), "api.example.com".to_string());
        let a = Auth::Bearer {
            token: "{{ token }}".into(),
            prefix: "Token".into(),
        };
        let ra = a.resolve(&vars);
        match &ra {
            Auth::Bearer { token, prefix } => {
                assert_eq!(token, "real-token");
                assert_eq!(prefix, "Token");
            }
            _ => panic!("类型不该变"),
        }
        // 模板没定义 → 原样保留（界面上看得见，方便排错）
        let missing = Auth::ApiKey {
            key: "X-Key".into(),
            value: "{{ nope }}".into(),
            location: KeyIn::Header,
        };
        match missing.resolve(&vars) {
            Auth::ApiKey { value, .. } => assert_eq!(value, "{{ nope }}"),
            _ => panic!("类型不该变"),
        }
    }

    #[test]
    fn summary_and_kind() {
        assert!(Auth::None.is_none());
        assert_eq!(
            Auth::Bearer {
                token: "t".into(),
                prefix: String::new()
            }
            .summary(),
            "Bearer t"
        );
        assert!(
            Auth::ApiKey {
                key: String::new(),
                value: String::new(),
                location: KeyIn::Query
            }
            .summary()
            .contains("query")
        );
        assert_eq!(
            Auth::Basic {
                username: "u".into(),
                password: "p".into()
            }
            .kind_id(),
            "basic"
        );
    }

    #[test]
    fn slots_and_kind_switch() {
        let a = Auth::Bearer {
            token: "tok".into(),
            prefix: "Token".into(),
        };
        assert_eq!(a.field(AuthField::A), "tok");
        assert_eq!(a.field(AuthField::B), "Token");
        let mut b = a.clone();
        b.set_field(AuthField::A, "tok2".into());
        assert_eq!(b.field(AuthField::A), "tok2");
        // 换类型带上主值
        match a.with_kind("basic") {
            Auth::Basic { username, password } => {
                assert_eq!(username, "tok");
                assert!(password.is_empty());
            }
            _ => panic!("应变成 basic"),
        }
        match a.with_kind("apikey") {
            Auth::ApiKey { key, location, .. } => {
                assert_eq!(key, "tok");
                assert_eq!(location, KeyIn::Header);
            }
            _ => panic!("应变成 apikey"),
        }
        assert!(a.with_kind("none").is_none());
        // NonE 上位没有值可带
        assert_eq!(Auth::None.field(AuthField::A), "");
    }

    #[test]
    fn serde_shape_is_stable() {
        // yml 里长这样，老文件没这字段就是 None（不用认证）
        let a: Auth =
            serde_yaml::from_str("kind: bearer\ntoken: '{{ tok }}'\nprefix: Token").unwrap();
        assert_eq!(
            a,
            Auth::Bearer {
                token: "{{ tok }}".into(),
                prefix: "Token".into()
            }
        );
        let b: Auth = serde_yaml::from_str("kind: apikey\nkey: X-Key\nvalue: v").unwrap();
        match b {
            Auth::ApiKey { location, .. } => assert_eq!(location, KeyIn::Header), // 默认放头
            _ => panic!("应是 apikey"),
        }
        let c: Auth = serde_yaml::from_str("kind: none").unwrap();
        assert!(c.is_none());
        let out = serde_yaml::to_string(&Auth::Basic {
            username: "u".into(),
            password: "p".into(),
        })
        .unwrap();
        assert!(out.contains("kind: basic"), "{out}");
    }
}
