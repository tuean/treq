//! 极简 cookie 罐：把响应里的 `Set-Cookie` 存下来，之后对同域请求自动带上。
//!
//! 不追求浏览器级完备（没有 SameSite、Public Suffix List、并发锁语义），
//! 够用就好：域名（host-only / Domain 后缀）、路径前缀、Secure、Max-Age/Expires 过期、
//! 同名同域同路径覆盖。时间一律用 unix 秒。

use serde::{Deserialize, Serialize};

/// 现在（unix 秒）。
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 一条 cookie。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// 域名（小写，不带前导点）
    pub domain: String,
    pub path: String,
    /// 过期时间（unix 秒）；None = 会话 cookie（只活到进程结束/清空）
    pub expires: Option<i64>,
    pub secure: bool,
    pub http_only: bool,
    /// true = 只对 domain 精确匹配（没写 Domain 属性的 Set-Cookie）
    pub host_only: bool,
}

impl Cookie {
    /// 展示用：`sid=abc`（属性串另给）
    pub fn pair(&self) -> String {
        format!("{}={}", self.name, self.value)
    }

    /// 展示用属性串：`Path=/ · 过期 +59 分 · Secure · HttpOnly`
    /// 剩余时间用相对值（时区无关），超过一天才给 UTC 日期。
    pub fn attrs(&self, now: i64) -> String {
        let mut parts = vec![format!("Path={}", self.path)];
        parts.push(match self.expires {
            Some(t) => {
                let left = t - now;
                if left <= 0 {
                    "已过期".to_string()
                } else if left < 3600 {
                    format!("过期 +{} 分", (left + 59) / 60)
                } else if left < 86_400 {
                    format!("过期 +{} 时", (left + 3599) / 3600)
                } else {
                    format!("过期 {}（UTC）", fmt_unix(t))
                }
            }
            None => "会话".to_string(),
        });
        if self.secure {
            parts.push("Secure".into());
        }
        if self.http_only {
            parts.push("HttpOnly".into());
        }
        if self.host_only {
            parts.push("host-only".into());
        }
        parts.join(" · ")
    }

    pub fn is_expired(&self, now: i64) -> bool {
        self.expires.is_some_and(|t| t <= now)
    }
}

/// unix 秒 → `MM-DD HH:MM`（本地时区偏差不管，展示够用）
fn fmt_unix(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let _ = y;
    format!(
        "{:02}-{:02} {:02}:{:02}",
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60
    )
}

/// Howard Hinnant 的 civil_from_days：天数 → (年, 月, 日)
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 前一个算法反过来用：在 cookie 里只会用它算 `Expires`。
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// `Wed, 21 Oct 2015 07:28:00 GMT` → unix 秒（认不出就 None）
pub fn parse_http_date(s: &str) -> Option<i64> {
    let s = s.trim();
    // 去掉星期前缀
    let rest = match s.split_once(", ") {
        Some((_, r)) => r,
        None => s,
    };
    let mut it = rest.split_whitespace();
    let day: u32 = it.next()?.parse().ok()?;
    let mon = match it.next()?.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    let year: i64 = it.next()?.parse().ok()?;
    let time = it.next()?;
    let mut hms = time.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let mi: i64 = hms.next()?.parse().ok()?;
    let sec: i64 = hms.next().unwrap_or("0").parse().ok()?;
    Some(days_from_civil(year, mon, day) * 86_400 + h * 3600 + mi * 60 + sec)
}

/// 从 URL 里抠出 host（小写，去端口、去 userinfo）——不引 URL 库。
pub fn host_of(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or(authority); // 去掉 user:pass@
    let host = if let Some(rest) = authority.strip_prefix('[') {
        // IPv6：保留中括号内的内容
        rest.split(']').next().unwrap_or(rest).to_string()
    } else {
        authority.split(':').next().unwrap_or_default().to_string()
    };
    host.to_ascii_lowercase()
}

/// URL 的路径部分（默认 "/"，去掉 query/fragment）。
pub fn path_of(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = match after_scheme.find('/') {
        Some(i) => &after_scheme[i..],
        None => return "/".to_string(),
    };
    let path = rest.split(['?', '#']).next().unwrap_or("/");
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

/// `https://…`（Secure cookie 只在这种请求上发）
pub fn is_https(url: &str) -> bool {
    url.split_once("://")
        .map(|(s, _)| s.eq_ignore_ascii_case("https") || s.eq_ignore_ascii_case("wss"))
        .unwrap_or(false)
}

/// 解析一条 `Set-Cookie`；解析不出名字或已过期（Max-Age=0）→ None。
/// `host` 是响应来源的 host（用于 host-only 判定）。
pub fn parse_set_cookie(raw: &str, host: &str, now: i64) -> Option<Cookie> {
    let mut parts = raw.split(';');
    let nv = parts.next()?.trim();
    let (name, value) = nv.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut c = Cookie {
        name: name.to_string(),
        value: value.trim().to_string(),
        domain: host.to_ascii_lowercase(),
        path: "/".to_string(),
        expires: None,
        secure: false,
        http_only: false,
        host_only: true,
    };
    let mut max_age: Option<i64> = None;
    for attr in parts {
        let attr = attr.trim();
        let (k, v) = match attr.split_once('=') {
            Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim().to_string()),
            None => (attr.to_ascii_lowercase(), String::new()),
        };
        match k.as_str() {
            "domain" => {
                let d = v.trim_start_matches('.').to_ascii_lowercase();
                if !d.is_empty() {
                    c.domain = d;
                    c.host_only = false;
                }
            }
            "path" if v.starts_with('/') => c.path = v,
            "max-age" => max_age = v.parse::<i64>().ok(),
            "expires" => c.expires = parse_http_date(&v),
            "secure" => c.secure = true,
            "httponly" => c.http_only = true,
            _ => {}
        }
    }
    if let Some(secs) = max_age {
        c.expires = if secs <= 0 { Some(0) } else { Some(now + secs) };
    }
    if c.is_expired(now) {
        return None; // Max-Age=0 / 已过期 = 服务器让你删掉它
    }
    Some(c)
}

/// 从响应头里挑出所有 `Set-Cookie`。
pub fn from_response_headers(url: &str, headers: &[(String, String)], now: i64) -> Vec<Cookie> {
    let host = host_of(url);
    headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, v)| parse_set_cookie(v, &host, now))
        .collect()
}

/// cookie 的域是否覆盖 `host`（`evilexample.com` 不该被 `domain=example.com` 命中）。
fn domain_covers(host: &str, c: &Cookie) -> bool {
    if c.host_only {
        return host == c.domain;
    }
    host == c.domain || host.ends_with(&format!(".{}", c.domain))
}

/// RFC 6265 路径匹配：cookie 路径是请求路径的前缀，且边界到 '/'。
pub fn path_covers(req_path: &str, cookie_path: &str) -> bool {
    if cookie_path == "/" {
        return true;
    }
    if !req_path.starts_with(cookie_path) {
        return false;
    }
    req_path.len() == cookie_path.len() || req_path.as_bytes().get(cookie_path.len()) == Some(&b'/')
}

/// cookie 罐：内存里一个 Vec，量级小（几十条）不值当搞索引。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CookieJar {
    #[serde(default)]
    pub cookies: Vec<Cookie>,
}

impl CookieJar {
    /// 存一条：同名 + 同域 + 同路径覆盖；否则追加。
    pub fn store(&mut self, c: Cookie) {
        if let Some(slot) = self
            .cookies
            .iter_mut()
            .find(|x| x.name == c.name && x.domain == c.domain && x.path == c.path)
        {
            *slot = c;
        } else {
            self.cookies.push(c);
        }
    }

    /// 从响应头收集（存进去之前先清一遍过期项）。
    pub fn store_from_headers(
        &mut self,
        url: &str,
        headers: &[(String, String)],
        now: i64,
    ) -> usize {
        self.prune(now);
        let found = from_response_headers(url, headers, now);
        let n = found.len();
        for c in found {
            self.store(c);
        }
        n
    }

    /// 该请求该带上的 `Cookie` 头（没有就 None）。顺带丢掉过期项。
    pub fn header_for(&mut self, url: &str, now: i64) -> Option<String> {
        self.prune(now);
        let host = host_of(url);
        let path = path_of(url);
        let https = is_https(url);
        let pairs: Vec<String> = self
            .cookies
            .iter()
            .filter(|c| domain_covers(&host, c))
            .filter(|c| path_covers(&path, &c.path))
            .filter(|c| !c.secure || https)
            .map(|c| c.pair())
            .collect();
        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
    }

    /// 展示/管理用：按域分组（新 → 旧不保证，按域名字典序）。
    pub fn grouped(&self, now: i64) -> Vec<(String, Vec<&Cookie>)> {
        let mut hosts: Vec<String> = self
            .cookies
            .iter()
            .filter(|c| !c.is_expired(now))
            .map(|c| c.domain.clone())
            .collect();
        hosts.sort();
        hosts.dedup();
        hosts
            .into_iter()
            .map(|h| {
                let list = self
                    .cookies
                    .iter()
                    .filter(|c| c.domain == h && !c.is_expired(now))
                    .collect();
                (h, list)
            })
            .collect()
    }

    /// 清空：给了域就只清那个域，否则全清。返回清掉条数。
    pub fn clear(&mut self, domain: Option<&str>) -> usize {
        let before = self.cookies.len();
        match domain {
            Some(d) => self.cookies.retain(|c| c.domain != d),
            None => self.cookies.clear(),
        }
        before - self.cookies.len()
    }

    /// 丢掉过期项，返回条数。
    pub fn prune(&mut self, now: i64) -> usize {
        let before = self.cookies.len();
        self.cookies.retain(|c| !c.is_expired(now));
        before - self.cookies.len()
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_date_roundtrip() {
        // 1970-01-01 = 0；这两个日期来自常见 Set-Cookie
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(1_445_412_480)
        );
        assert_eq!(
            parse_http_date("21 Oct 2015 07:28:00 GMT"),
            Some(1_445_412_480)
        );
        assert_eq!(parse_http_date("garbage"), None);
        assert_eq!(parse_http_date("Wed, 32 Foo 2015 07:28:00 GMT"), None);
    }

    #[test]
    fn host_and_path_of_url() {
        assert_eq!(host_of("http://a.b.c:8080/x/y?z=1"), "a.b.c");
        assert_eq!(host_of("https://user:pw@Host.COM/p"), "host.com");
        assert_eq!(host_of("http://[::1]:8321/x"), "::1");
        assert_eq!(host_of("not a url"), "not a url");
        assert_eq!(path_of("http://h/a/b?c=1#d"), "/a/b");
        assert_eq!(path_of("http://h"), "/");
        assert_eq!(path_of("http://h/"), "/");
        assert!(is_https("https://h/x"));
        assert!(!is_https("http://h/x"));
    }

    #[test]
    fn parse_attrs() {
        let c = parse_set_cookie(
            "sid=abc123; Path=/api; Domain=.Example.com; Max-Age=600; Secure; HttpOnly",
            "api.example.com",
            1000,
        )
        .unwrap();
        assert_eq!(c.name, "sid");
        assert_eq!(c.value, "abc123");
        assert_eq!(c.domain, "example.com");
        assert_eq!(c.path, "/api");
        assert_eq!(c.expires, Some(1600));
        assert!(c.secure && c.http_only);
        assert!(!c.host_only);
        // 没写 Domain = host-only
        let h = parse_set_cookie("a=1", "x.example.com", 0).unwrap();
        assert!(h.host_only && h.domain == "x.example.com" && h.path == "/");
        // Max-Age=0 = 立刻删
        assert!(parse_set_cookie("a=1; Max-Age=0", "h", 0).is_none());
        // 过期日期 = 不存
        assert!(parse_set_cookie("a=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT", "h", 100).is_none());
        // 没名字的忽略
        assert!(parse_set_cookie("=v", "h", 0).is_none());
        assert!(parse_set_cookie("not-a-cookie", "h", 0).is_none());
    }

    #[test]
    fn domain_matching_is_not_sloppy() {
        let jar_c = |domain: &str, host_only: bool| Cookie {
            name: "a".into(),
            value: "1".into(),
            domain: domain.into(),
            path: "/".into(),
            expires: None,
            secure: false,
            http_only: false,
            host_only,
        };
        // Domain 后缀：子域也带
        let mut jar = CookieJar::default();
        jar.store(jar_c("example.com", false));
        assert!(jar.header_for("http://example.com/", 0).is_some());
        assert!(jar.header_for("http://a.example.com/", 0).is_some());
        // 但不能被 evilexample.com 命中
        assert!(jar.header_for("http://evilexample.com/", 0).is_none());
        assert!(jar.header_for("http://example.com.cn/", 0).is_none());
        // host-only：只认精确 host
        let mut jar2 = CookieJar::default();
        jar2.store(jar_c("example.com", true));
        assert!(jar2.header_for("http://example.com/", 0).is_some());
        assert!(jar2.header_for("http://a.example.com/", 0).is_none());
    }

    #[test]
    fn path_and_secure_filtering() {
        let mut jar = CookieJar::default();
        jar.store(Cookie {
            name: "p".into(),
            value: "1".into(),
            domain: "h".into(),
            path: "/api".into(),
            expires: None,
            secure: false,
            http_only: false,
            host_only: true,
        });
        jar.store(Cookie {
            name: "s".into(),
            value: "2".into(),
            domain: "h".into(),
            path: "/".into(),
            expires: None,
            secure: true,
            http_only: false,
            host_only: true,
        });
        // 路径边界：/apix 不该匹配 /api；http 上不发 Secure cookie
        assert_eq!(
            jar.header_for("https://h/api/x", 0).as_deref(),
            Some("p=1; s=2")
        );
        assert_eq!(jar.header_for("https://h/apix", 0).as_deref(), Some("s=2"));
        assert_eq!(jar.header_for("http://h/apix", 0).as_deref(), None);
        assert_eq!(jar.header_for("http://h/api/x", 0).as_deref(), Some("p=1"));
        assert_eq!(jar.header_for("https://h/", 0).as_deref(), Some("s=2"));
    }

    #[test]
    fn replace_prune_clear() {
        let mut jar = CookieJar::default();
        let mk = |v: &str, exp: Option<i64>| Cookie {
            name: "a".into(),
            value: v.into(),
            domain: "h".into(),
            path: "/".into(),
            expires: exp,
            secure: false,
            http_only: false,
            host_only: true,
        };
        jar.store(mk("1", None));
        jar.store(mk("2", None)); // 同名同域同路径 → 覆盖
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.header_for("http://h/", 0).as_deref(), Some("a=2"));
        // 换名字 = 新增一条（同名才会覆盖）
        let mut tmp = mk("old", Some(50));
        tmp.name = "b".into();
        jar.store(tmp);
        assert_eq!(jar.len(), 2);
        assert_eq!(jar.prune(100), 1); // 过期被清
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.header_for("http://h/", 100).as_deref(), Some("a=2"));
        let mut c2 = mk("x", None);
        c2.name = "other".into();
        jar.store(c2);
        assert_eq!(jar.len(), 2);
        assert_eq!(jar.clear(Some("h")), 2);
        assert!(jar.is_empty());
    }

    #[test]
    fn serde_roundtrip() {
        let mut jar = CookieJar::default();
        jar.store_from_headers(
            "https://a.example.com/login",
            &[("set-cookie".into(), "sid=xyz; Path=/; Max-Age=60".into())],
            1000,
        );
        let json = serde_json::to_string(&jar).unwrap();
        let back: CookieJar = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.cookies[0].name, "sid");
        assert_eq!(back.cookies[0].domain, "a.example.com");
        assert_eq!(back.cookies[0].expires, Some(1060));
    }
}
