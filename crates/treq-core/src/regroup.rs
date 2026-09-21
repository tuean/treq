//! 按「服务 / 域名」重组工作区 —— **骨架，实现留空**。
//!
//! 这个模块当初只为一次性用途写过：把某个工作区里几百个接口按后端服务拆开
//! （同一服务散落在多个域名/网关上，重组后并到一个分组）。当时的判定规则
//! 完全取决于个人环境（内网域名后缀、网关路径前缀等），不适合随仓库分发，
//! 所以**实现已经清空**，只留下接口与用法，需要的话自己补上：
//!
//! 1. [`split_url`]：把 URL 拆成 `(host, 路径段)`，通用实现，直接复用；
//! 2. 自己定义「哪些 host / 路径前缀算同一个服务、哪些算外部接口」；
//! 3. 建集合/分组，用 [`WorkspaceStore`] 把请求写到目标分组下；
//!    想做成幂等的，可以先删掉上次生成的集合（`store.delete_collection`）。
//!
//! 调用方式见 `crates/treq-core/examples/regroup_services.rs`。

use crate::store::WorkspaceStore;
use anyhow::{bail, Result};

/// 重组结果统计（字段够用就行，实现时按需增减）。
#[derive(Debug, Default, PartialEq)]
pub struct RegroupStats {
    /// 生成的分组数
    pub groups: usize,
    /// 写入的请求数
    pub requests: usize,
    /// 判定为重复、被折叠掉的请求数
    pub duplicates: usize,
}

impl RegroupStats {
    pub fn summary(&self) -> String {
        format!(
            "{} 个分组 / {} 个接口（折叠重复 {} 条）",
            self.groups, self.requests, self.duplicates
        )
    }
}

/// 拆出 host 与路径段：容忍没有 scheme、前后空白、`{{ 模板 }}`、裸路径。
///
/// 例：`http://api.example.com:8080/gw/svc/items?a=1` → `("api.example.com:8080", ["gw", "svc", "items"])`
pub fn split_url(url: &str) -> (Option<String>, Vec<String>) {
    let u = url.trim();
    if u.is_empty() {
        return (None, vec![]);
    }
    let has_scheme = u.contains("://");
    let rest = match u.find("://") {
        Some(i) => &u[i + 3..],
        None => u,
    };
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut it = rest.splitn(2, '/');
    let head = it.next().unwrap_or("");
    let segs: Vec<String> = it
        .next()
        .unwrap_or("")
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if head.is_empty() {
        return (None, segs);
    }
    // 无 scheme 时：带点/带端口的第一段是域名，否则是裸路径的第一段
    if has_scheme || head.contains('.') || head.contains(':') {
        (Some(head.to_string()), segs)
    } else {
        let mut v = segs;
        v.insert(0, head.to_string());
        (None, v)
    }
}

/// 按自己的服务/域名规则重组工作区。
///
/// 仓库里**留空**：曾经的那套规则是个人环境专用的（内网域名后缀、网关前缀），
/// 已经删掉，请照上面模块注释自己实现——或者干脆写个独立的一次性脚本，
/// 只把 [`split_url`] 当工具函数用。
pub fn regroup(_store: &WorkspaceStore) -> Result<RegroupStats> {
    bail!(
        "regroup() 是留空的骨架：这里的服务/域名规则是个人环境专用的，未随仓库分发。\
         请按自己的规则实现 crates/treq-core/src/regroup.rs（split_url 可直接复用），\
         或改用自己的一次性脚本。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_url_handles_scheme_path_and_query() {
        let (host, segs) = split_url("http://api.example.com:8080/gw/svc/items?a=1#x");
        assert_eq!(host.as_deref(), Some("api.example.com:8080"));
        assert_eq!(segs, vec!["gw", "svc", "items"]);

        let (host, segs) = split_url("https://example.com/");
        assert_eq!(host.as_deref(), Some("example.com"));
        assert!(segs.is_empty());
    }

    #[test]
    fn split_url_handles_schemeless_and_templates() {
        // 无 scheme：带点的第一段当域名
        let (host, segs) = split_url("example.com/a/b");
        assert_eq!(host.as_deref(), Some("example.com"));
        assert_eq!(segs, vec!["a", "b"]);

        // 无 scheme 的裸路径：整条都算路径段
        let (host, segs) = split_url("gw/svc/x");
        assert_eq!(host, None);
        assert_eq!(segs, vec!["gw", "svc", "x"]);

        // 模板变量与空白
        let (host, segs) = split_url("  {{ base_url }}/items  ");
        assert_eq!(host, None);
        assert_eq!(segs, vec!["{{ base_url }}", "items"]);

        assert_eq!(split_url("   "), (None, vec![]));
    }

    #[test]
    fn regroup_is_left_unimplemented() {
        let store = WorkspaceStore::new(std::env::temp_dir());
        let err = regroup(&store).unwrap_err().to_string();
        assert!(err.contains("留空"), "应提示实现留空：{err}");
    }
}
