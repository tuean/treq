//! 按服务名重组工作区的薄 CLI。
//!
//! 用法：`cargo run -p treq-core --example regroup_services -- <工作区目录>`
//! 会在工作区里重建 `内部接口 · 按服务` 与 `外部接口 · 按域名` 两个集合（可重复执行）。

use std::path::PathBuf;
use treq_core::regroup::regroup;
use treq_core::store::WorkspaceStore;

fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(p)
}

fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .map(|a| expand(&a))
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join("Documents")
                .join("treq")
        });
    println!("工作区: {}", root.display());
    let store = WorkspaceStore::new(root);
    let stats = regroup(&store)?;
    println!("{}", stats.summary());
    Ok(())
}
