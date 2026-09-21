//! 侧栏/搜索性能体检：量化 load / clone / 过滤 三项开销。
//!
//! 用法：cargo run -q --release -p treq-core --example perf_check -- [工作区目录]

use std::time::Instant;
use treq_core::{Collection, RequestItem, WorkspaceStore};

fn main() -> anyhow::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{}/Documents/treq", std::env::var("HOME").unwrap()));
    let store = WorkspaceStore::new(dir.clone().into());

    let t = Instant::now();
    let ws = store.load()?;
    let load = t.elapsed();

    let reqs: usize = ws
        .collections
        .iter()
        .map(|c| c.requests.len() + c.groups.iter().map(|g| g.requests.len()).sum::<usize>())
        .sum();

    // 1) 整棵树深拷贝（历史上每帧都做；现已改为索引引用）
    let t = Instant::now();
    let mut sink = 0usize;
    for _ in 0..10 {
        let cols: Vec<Collection> = ws.collections.clone();
        for col in &cols {
            let groups = col.groups.clone();
            for g in &groups {
                let rs: Vec<RequestItem> = g.requests.clone();
                sink += rs.len();
            }
            let rs: Vec<RequestItem> = col.requests.clone();
            sink += rs.len();
        }
    }
    let clone_once = t.elapsed() / 10;

    // 2) 每帧过滤：现状（每个名字都 to_lowercase 再 contains）
    let t = Instant::now();
    for _ in 0..10 {
        let q = "a";
        let mut hits = 0usize;
        for col in &ws.collections {
            let cm = col.name.to_lowercase().contains(q);
            for g in &col.groups {
                let gm = cm || g.name.to_lowercase().contains(q);
                let any = g.requests.iter().any(|r| r.name.to_lowercase().contains(q));
                if gm || any {
                    hits += 1;
                }
                for r in &g.requests {
                    if gm || r.name.to_lowercase().contains(q) {
                        hits += 1;
                    }
                }
            }
            for r in &col.requests {
                if cm || r.name.to_lowercase().contains(q) {
                    hits += 1;
                }
            }
        }
        sink += hits;
    }
    let filter_now = t.elapsed() / 10;

    // 3) 预存小写名后过滤（扁平索引，只存名字）
    let flat: Vec<(String, bool)> = ws
        .collections
        .iter()
        .flat_map(|c| {
            let mut v: Vec<(String, bool)> = vec![(c.name.to_lowercase(), true)];
            for g in &c.groups {
                v.push((g.name.to_lowercase(), true));
                v.extend(g.requests.iter().map(|r| (r.name.to_lowercase(), false)));
            }
            v.extend(c.requests.iter().map(|r| (r.name.to_lowercase(), false)));
            v
        })
        .collect();
    let t = Instant::now();
    for _ in 0..10 {
        let q = "a";
        sink += flat.iter().filter(|(n, _)| n.contains(q)).count();
    }
    let filter_indexed = t.elapsed() / 10;

    // 4) 索引构建（打开/刷新时一次）
    let t = Instant::now();
    let mut names: Vec<String> = Vec::with_capacity(reqs + 400);
    for col in &ws.collections {
        names.push(col.name.to_lowercase());
        names.push(col.name.clone());
        for g in &col.groups {
            names.push(g.name.to_lowercase());
            for r in &g.requests {
                names.push(r.name.to_lowercase());
                names.push(r.url.clone());
            }
        }
        for r in &col.requests {
            names.push(r.name.to_lowercase());
            names.push(r.url.clone());
        }
    }
    let index_build = t.elapsed();

    println!("工作区: {}", dir);
    println!(
        "集合 {} / 请求 {} / 共 {} 个字符串条目",
        ws.collections.len(),
        reqs,
        names.len()
    );
    let _ = sink;
    println!("加载(load):      {:?}", load);
    println!("全树深拷贝:      {:?}  ← 已从渲染路径移除", clone_once);
    println!("过滤(to_lowercase 逐项): {:?}", filter_now);
    println!("过滤(预存小写):  {:?}", filter_indexed);
    println!("索引构建(一次):  {:?}", index_build);
    Ok(())
}
