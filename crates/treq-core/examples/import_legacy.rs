//! 把本机 Insomnium / Yaak 的接口迁移进 treq 工作区（按「空间 → 服务」聚合）。
//!
//! 用法：
//!   cargo run -p treq-core --example import_legacy -- <目标工作区目录> [insomnium|yaak|all]
//! 例：
//!   cargo run -p treq-core --example import_legacy -- ~/Documents/treq all
//!
//! 幂等：目标里已有同名 collection 就跳过该来源，重复执行不会翻倍。

use std::collections::HashSet;
use treq_core::WorkspaceStore;
use treq_core::legacy_import::{
    ImportStats, default_insomnium_dir, default_yaak_db, import_insomnium, import_yaak,
};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(target) = args.first() else {
        eprintln!("用法: import_legacy <目标工作区目录> [insomnium|yaak|all]");
        std::process::exit(2);
    };
    let which = args.get(1).map(|s| s.as_str()).unwrap_or("all");
    let target = std::path::PathBuf::from(shellexpand(target));

    let store = WorkspaceStore::new(target.clone());
    store.ensure_root()?;
    let mut existing: HashSet<String> = store
        .load()?
        .collections
        .into_iter()
        .map(|c| c.name)
        .collect();
    println!("目标工作区: {}", target.display());
    println!("已有 collection: {} 个", existing.len());

    let mut stats = ImportStats::default();
    if which == "all" || which == "insomnium" {
        let dir = default_insomnium_dir();
        println!("读取 Insomnium: {}", dir.display());
        import_insomnium(&store, &dir, &mut existing, &mut stats)?;
    }
    if which == "all" || which == "yaak" {
        let db = default_yaak_db();
        println!("读取 Yaak: {}", db.display());
        import_yaak(&store, &db, &mut existing, &mut stats)?;
    }
    println!("完成: {}", stats.summary());
    Ok(())
}

/// 支持 `~/x` 形式（不做完整 shell 展开）
fn shellexpand(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{}/{}", home, rest);
    }
    p.to_string()
}
