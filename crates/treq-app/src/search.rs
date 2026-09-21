//! 跨工作区（项目）请求搜索 + 侧栏树摊平：纯函数，单测覆盖。
//!
//! 规则：
//! - 当前工作区的命中排在前面（切工作区有成本，先给本区的）；
//! - 关键字匹配 名称 / URL / 方法 / 集合(分组) / 工作区名，大小写不敏感；
//! - 空关键字时按原顺序全部返回（面板一打开就能看到东西）。

use crate::model::{RowRef, Selection, TreeRow};
use std::collections::HashSet;
use std::path::Path;
use treq_core::Collection;

/// 摊平后一行需要附带的状态（渲染时不再回查树）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowState {
    pub selected: bool,
    pub renaming: bool,
    pub collapsed: bool,
    /// 缩进层级：集合 0，第一层分组 1，子分组 2……
    pub depth: u8,
}

/// 分组树：把平铺的分组列表按 `parent` 整理成 DFS 前序 `(组下标, 深度, 父下标)`。
///
/// 盘上所有分组是平铺的（层级只存在于 `Group::parent`），所以建/删/改路径都不受影响。
/// 悬空 parent（手改 yml、回收站还原子分组）当第一层；互相成环的分组兜底也当一个顶层，
/// 保证任何分组都不会看不见。
pub fn group_tree(groups: &[treq_core::Group]) -> Vec<(usize, u8, Option<usize>)> {
    let n = groups.len();
    let mut by_id: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, g) in groups.iter().enumerate() {
        by_id.insert(g.id.as_str(), i);
    }
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut parent_of: Vec<Option<usize>> = vec![None; n];
    let mut is_child = vec![false; n];
    for (i, g) in groups.iter().enumerate() {
        if let Some(&p) = g.parent.as_deref().and_then(|p| by_id.get(p))
            && p != i
        {
            kids[p].push(i);
            parent_of[i] = Some(p);
            is_child[i] = true;
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    // 从「不是别人孩子」的分组往下走；栈里倒着放，弹出来就是文档顺序
    let mut stack: Vec<(usize, u8)> = (0..n)
        .rev()
        .filter(|&i| !is_child[i])
        .map(|i| (i, 0))
        .collect();
    while let Some((i, d)) = stack.pop() {
        if seen[i] {
            continue;
        }
        seen[i] = true;
        out.push((i, d, parent_of[i]));
        for &k in kids[i].iter().rev() {
            stack.push((k, d.saturating_add(1)));
        }
    }
    // 环上的分组（parent 互相指）走不到，补成顶层，宁可按平铺显示也不能丢
    for i in 0..n {
        if !seen[i] {
            out.push((i, 0, None));
        }
    }
    out
}

/// 分组 id 的祖先链（集合 id 在外，然后从外到内的父分组），**不含自己**。
/// 分组嵌套后要顺着 parent 往上走；环/重复用 seen 挡住。
pub fn group_ancestors(cols: &[Collection], group_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    for c in cols {
        let Some(start) = c.groups.iter().find(|g| g.id == group_id) else {
            continue;
        };
        let mut chain = vec![c.id.clone()];
        let mut seen: HashSet<&str> = HashSet::new();
        seen.insert(start.id.as_str());
        let mut cur = start.parent.clone();
        while let Some(pid) = cur {
            let Some(g) = c.groups.iter().find(|g| g.id == pid) else {
                break;
            };
            if !seen.insert(g.id.as_str()) {
                break;
            }
            chain.push(g.id.clone());
            cur = g.parent.clone();
        }
        // chain 现在是 [集合, 父, 祖父…]（由内往外）→ 反转成由外往内
        let col_id = chain.remove(0);
        chain.reverse();
        out.push(col_id);
        out.extend(chain);
        break;
    }
    out
}

/// 某分组 + 它的所有后代分组 id（父链展开，环用 contains 挡住不会转圈）。
/// 删分组时连子分组一起进回收站，否则它们的 parent 悬空、重开就从树里消失。
pub fn group_subtree_ids(groups: &[treq_core::Group], group_id: &str) -> Vec<String> {
    let mut ids = vec![group_id.to_string()];
    let mut i = 0;
    while i < ids.len() {
        let cur = ids[i].clone();
        for g in groups {
            if g.parent.as_deref() == Some(cur.as_str()) && !ids.contains(&g.id) {
                ids.push(g.id.clone());
            }
        }
        i += 1;
    }
    ids
}

/// 某节点的**祖先**（集合 → 分组，从外到内）；节点自己不算。
/// 折叠其它节点时要把这条路径留在展开状态，否则当前节点自己被藏起来看不见。
pub fn ancestor_ids(cols: &[Collection], sel: &Selection) -> Vec<String> {
    match sel {
        Selection::Collection(_) => Vec::new(),
        Selection::Group(gid) => group_ancestors(cols, gid),
        Selection::Request(rid) => cols
            .iter()
            .find_map(|c| {
                if c.requests.iter().any(|r| &r.id == rid) {
                    return Some(vec![c.id.clone()]);
                }
                let g = c.groups.iter().find(|g| g.requests.iter().any(|r| &r.id == rid))?;
                let mut v = group_ancestors(cols, &g.id);
                v.push(g.id.clone());
                Some(v)
            })
            .unwrap_or_default(),
    }
}

/// 把「过滤 + 折叠」后的树摊平成行列表；只记位置与状态，不复制字符串。
///
/// 规则：
/// - 关键字按名称子串匹配（大小写不敏感，见 [`contains_ci`]）；
/// - 命中集合/分组名时其子树整体保留；
/// - 过滤状态下忽略折叠（结果必须可见）；
/// - 折叠的集合/分组不展开子行（分组可以嵌套任意层）。
pub fn flatten_tree(
    cols: &[Collection],
    query_lower: &str,
    collapsed: &HashSet<String>,
    selection: Option<&Selection>,
    rename: Option<&Selection>,
) -> Vec<(RowRef, RowState)> {
    let mut out = Vec::with_capacity(cols.len() + 64);
    let q = query_lower;
    let filtering = !q.is_empty();
    let sel = |s: &Selection| selection == Some(s);
    let ren = |s: &Selection| rename == Some(s);

    for (ci, col) in cols.iter().enumerate() {
        let col_m = filtering && contains_ci(&col.name, q);
        let tree = group_tree(&col.groups);
        // 过滤时：自己命中、或后代命中才显示（DFS 前序倒着扫，命中会往上带）
        let keep: Vec<bool> = {
            let mut keep = vec![false; col.groups.len()];
            for &(gi, _, parent) in tree.iter().rev() {
                let g = &col.groups[gi];
                keep[gi] |= col_m
                    || contains_ci(&g.name, q)
                    || g.requests.iter().any(|r| contains_ci(&r.name, q));
                if let Some(p) = parent {
                    keep[p] |= keep[gi];
                }
            }
            keep
        };
        let any_child =
            keep.iter().any(|&k| k) || col.requests.iter().any(|r| contains_ci(&r.name, q));
        if filtering && !(col_m || any_child) {
            continue;
        }
        let col_collapsed = !filtering && collapsed.contains(&col.id);
        out.push((
            RowRef { col: ci, grp: None, req: None },
            RowState {
                selected: sel(&Selection::Collection(col.id.clone())),
                renaming: ren(&Selection::Collection(col.id.clone())),
                collapsed: col_collapsed,
                depth: 0,
            },
        ));
        if col_collapsed {
            continue;
        }
        // 折叠着的分组的下级一律不摊平：DFS 前序里「深度 > 折叠点」的都是它的后代
        let mut hidden_below: Option<u8> = None;
        // 每一层的名字是否命中（命中则整棵子树都显示）—— 按深度存当前路径
        let mut name_hit_at: Vec<bool> = Vec::new();
        for (gi, depth, _) in tree {
            if let Some(d) = hidden_below {
                if depth > d {
                    continue;
                }
                hidden_below = None;
            }
            if filtering && !keep[gi] {
                continue;
            }
            let g = &col.groups[gi];
            let g_name_hit = filtering && contains_ci(&g.name, q);
            name_hit_at.truncate(depth as usize);
            name_hit_at.push(g_name_hit);
            let g_m = col_m || name_hit_at.iter().any(|&x| x);
            let g_collapsed = !filtering && collapsed.contains(&g.id);
            out.push((
                RowRef { col: ci, grp: Some(gi), req: None },
                RowState {
                    selected: sel(&Selection::Group(g.id.clone())),
                    renaming: ren(&Selection::Group(g.id.clone())),
                    collapsed: g_collapsed,
                    depth: depth.saturating_add(1),
                },
            ));
            if g_collapsed {
                hidden_below = Some(depth);
                continue;
            }
            for (ri, r) in g.requests.iter().enumerate() {
                if filtering && !(g_m || contains_ci(&r.name, q)) {
                    continue;
                }
                out.push((
                    RowRef { col: ci, grp: Some(gi), req: Some(ri) },
                    RowState {
                        selected: sel(&Selection::Request(r.id.clone())),
                        renaming: ren(&Selection::Request(r.id.clone())),
                        collapsed: false,
                        depth: depth.saturating_add(2),
                    },
                ));
            }
        }
        for (ri, r) in col.requests.iter().enumerate() {
            if filtering && !(col_m || contains_ci(&r.name, q)) {
                continue;
            }
            out.push((
                RowRef { col: ci, grp: None, req: Some(ri) },
                RowState {
                    selected: sel(&Selection::Request(r.id.clone())),
                    renaming: ren(&Selection::Request(r.id.clone())),
                    collapsed: false,
                    // 集合直属请求和「分组里的请求」同一档缩进
                    depth: 2,
                },
            ));
        }
    }
    out
}

/// 是否有任何展开的集合/分组（决定「折叠全部」按钮的图标与动作）。
pub fn any_expanded(cols: &[Collection], collapsed: &HashSet<String>) -> bool {
    cols.iter().any(|c| {
        let has_kids = !c.requests.is_empty() || !c.groups.is_empty();
        (has_kids && !collapsed.contains(&c.id))
            || c.groups.iter().any(|g| {
                (!g.requests.is_empty() || c.groups.iter().any(|k| k.parent.as_deref() == Some(&g.id)))
                    && !collapsed.contains(&g.id)
            })
    })
}

/// 折叠全部时要标记的 id（集合 + 分组）。
pub fn collapse_ids(cols: &[Collection]) -> Vec<&str> {
    cols.iter()
        .flat_map(|c| std::iter::once(c.id.as_str()).chain(c.groups.iter().map(|g| g.id.as_str())))
        .collect()
}

/// 侧栏「＋」新建时东西该放哪：选中请求 → 它所在的分组（没有就是集合）；选中分组 → 那个分组；
/// 选中集合 → 那个集合；没选 / 选中的东西已经不在树里 → 第一个集合。一条集合都没有 → None。
pub fn new_item_target(
    cols: &[Collection],
    sel: Option<&Selection>,
) -> Option<(String, Option<String>)> {
    let from_selection = sel.and_then(|sel| match sel {
        Selection::Request(id) => cols.iter().find_map(|c| {
            if c.requests.iter().any(|r| &r.id == id) {
                Some((c.id.clone(), None))
            } else {
                c.groups.iter().find_map(|g| {
                    g.requests
                        .iter()
                        .any(|r| &r.id == id)
                        .then(|| (c.id.clone(), Some(g.id.clone())))
                })
            }
        }),
        Selection::Group(id) => cols.iter().find_map(|c| {
            c.groups
                .iter()
                .any(|g| &g.id == id)
                .then(|| (c.id.clone(), Some(id.clone())))
        }),
        Selection::Collection(id) => cols
            .iter()
            .find(|c| &c.id == id)
            .map(|c| (c.id.clone(), None)),
    });
    from_selection.or_else(|| cols.first().map(|c| (c.id.clone(), None)))
}

/// 「折叠所有（除此节点）」该留下哪些 id 折叠：全部集合 + 分组，减掉当前节点的**祖先路径**
/// （否则节点自己被藏起来），再减掉当前节点本身（如果它原来是展开的）。
pub fn collapse_all_except_ids(
    cols: &[Collection],
    sel: &Selection,
    node_was_expanded: bool,
) -> HashSet<String> {
    let mut keep = ancestor_ids(cols, sel);
    if node_was_expanded {
        match sel {
            Selection::Collection(id) | Selection::Group(id) | Selection::Request(id) => {
                keep.push(id.clone())
            }
        }
    }
    collapse_ids(cols)
        .into_iter()
        .map(|s| s.to_string())
        .filter(|id| !keep.contains(id))
        .collect()
}

/// 在摊平后的行里找任意节点（集合/分组/请求）的行号：新建完把该行滚进视野。
pub fn node_row_index(rows: &[TreeRow], cols: &[Collection], sel: &Selection) -> Option<usize> {
    rows.iter().position(|r| {
        let Some(col) = cols.get(r.at.col) else {
            return false;
        };
        match (sel, r.at.grp, r.at.req) {
            (Selection::Collection(id), None, None) => &col.id == id,
            (Selection::Group(id), Some(gi), None) => {
                col.groups.get(gi).is_some_and(|g| &g.id == id)
            }
            (Selection::Request(id), Some(gi), Some(ri)) => col
                .groups
                .get(gi)
                .and_then(|g| g.requests.get(ri))
                .is_some_and(|r| &r.id == id),
            (Selection::Request(id), None, Some(ri)) => {
                col.requests.get(ri).is_some_and(|r| &r.id == id)
            }
            _ => false,
        }
    })
}

/// 大小写不敏感的子串判断，**不分配内存**。
///
/// 逐字节比较：ASCII 字母折叠大小写，非 ASCII 字节要求完全相等。
/// UTF-8 的续字节都 ≥ 0x80，不会与 ASCII 混淆，所以按字节比对是安全的；
/// 需要小写化的是 `needle`（调用方传 `query.to_lowercase()`）。
pub fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    let n = needle_lower.as_bytes();
    if n.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    (0..=h.len() - n.len()).any(|i| {
        h[i..i + n.len()]
            .iter()
            .zip(n)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

/// 把若干段可搜文本拼成一个小写索引串（建索引时算一次，之后每次按键只做子串匹配）。
pub fn haystack(parts: &[&str]) -> String {
    let mut out = String::new();
    for p in parts {
        if p.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&p.to_lowercase());
    }
    out
}

/// 一条索引项需要暴露给搜索的最小信息。
pub trait Hit {
    fn workspace(&self) -> &Path;
    /// 预先拼好的小写索引串（见 [`haystack`]），覆盖 名称/URL/方法/集合/工作区名。
    fn haystack(&self) -> &str;
}

/// 返回命中项的下标（已排序、已截断到 limit）。
pub fn rank_hits<T: Hit>(hits: &[T], current: &Path, query: &str, limit: usize) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    let matches = |h: &T| q.is_empty() || contains_ci(h.haystack(), &q);
    let mut out: Vec<usize> = Vec::new();
    for same_ws in [true, false] {
        for (i, h) in hits.iter().enumerate() {
            if (h.workspace() == current) != same_ws || !matches(h) {
                continue;
            }
            out.push(i);
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

/// 「移动到…」的一个候选目标：集合根目录或某个分组。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveTarget {
    pub collection_id: String,
    /// `None` = 集合根目录
    pub group_id: Option<String>,
    /// 显示用：`集合名` 或 `集合名 / 分组名`
    pub label: String,
}

/// 列出请求可以搬去的目标（集合根目录 + 各分组），并：
///
/// - **跳过请求当前所在的那个**（搬到自己那儿是空操作，列出来只会让人误点）；
/// - 带关键字时按子串过滤（大小写不敏感，匹配集合名/分组名）。
pub fn move_targets(cols: &[Collection], request_id: &str, query: &str) -> Vec<MoveTarget> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    for c in cols {
        let in_root = c.requests.iter().any(|r| r.id == request_id);
        if !in_root {
            let label = c.name.clone();
            if contains_ci(&label, &q) {
                out.push(MoveTarget {
                    collection_id: c.id.clone(),
                    group_id: None,
                    label,
                });
            }
        }
        for g in &c.groups {
            if g.requests.iter().any(|r| r.id == request_id) {
                continue;
            }
            let label = format!("{} / {}", c.name, g.name);
            if contains_ci(&label, &q) {
                out.push(MoveTarget {
                    collection_id: c.id.clone(),
                    group_id: Some(g.id.clone()),
                    label,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn req(id: &str) -> treq_core::RequestItem {
        treq_core::RequestItem {
            id: id.to_string(),
            name: id.to_string(),
            method: "GET".into(),
            url: "https://x/".into(),
            params: vec![],
            headers: vec![],
            body: treq_core::Body::default(),
            description: String::new(),
            docs_open: true,
            auth: None,
        }
    }

    fn col(id: &str, groups: &[&str], reqs: &[&str]) -> Collection {
        Collection {
            id: id.to_string(),
            name: id.to_string(),
            requests: reqs.iter().map(|r| req(r)).collect(),
            groups: groups
                .iter()
                .map(|g| treq_core::Group {
                    id: (*g).to_string(),
                    parent: None,
                    name: (*g).to_string(),
                    requests: vec![req(&format!("{g}-r"))],
                })
                .collect(),
        }
    }

    /// 造一个带层级的分组集合：groups = [(id, parent)]
    fn col_nested(id: &str, groups: &[(&str, Option<&str>)]) -> Collection {
        Collection {
            id: id.to_string(),
            name: id.to_string(),
            requests: vec![],
            groups: groups
                .iter()
                .map(|(g, p)| treq_core::Group {
                    id: (*g).to_string(),
                    parent: p.map(|p| p.to_string()),
                    name: (*g).to_string(),
                    requests: vec![req(&format!("{g}-r"))],
                })
                .collect(),
        }
    }

    /// 行 → (id, 缩进层级)
    fn rows_of(c: &Collection, collapsed: &HashSet<String>, q: &str) -> Vec<(String, u8)> {
        flatten_tree(
            std::slice::from_ref(c),
            q,
            collapsed,
            None,
            None,
        )
        .into_iter()
        .map(|(at, st)| {
            let label = match (at.grp, at.req) {
                (None, None) => c.id.clone(),
                (Some(gi), None) => c.groups[gi].id.clone(),
                (Some(gi), Some(ri)) => c.groups[gi].requests[ri].id.clone(),
                (None, Some(ri)) => c.requests[ri].id.clone(),
            };
            (label, st.depth)
        })
        .collect()
    }

    #[test]
    fn group_tree_nests_by_parent_and_never_loses_a_group() {
        let mut groups = vec![
            treq_core::Group {
                id: "a".into(),
                parent: None,
                name: "a".into(),
                requests: vec![],
            },
            treq_core::Group {
                id: "b".into(),
                parent: Some("a".into()),
                name: "b".into(),
                requests: vec![],
            },
            treq_core::Group {
                id: "c".into(),
                parent: Some("b".into()),
                name: "c".into(),
                requests: vec![],
            },
            // 悬空 parent 与自引用：都当第一层
            treq_core::Group {
                id: "ghost".into(),
                parent: Some("nope".into()),
                name: "ghost".into(),
                requests: vec![],
            },
            treq_core::Group {
                id: "self".into(),
                parent: Some("self".into()),
                name: "self".into(),
                requests: vec![],
            },
        ];
        let order: Vec<(String, u8)> = group_tree(&groups)
            .into_iter()
            .map(|(i, d, _)| (groups[i].id.clone(), d))
            .collect();
        assert_eq!(
            order,
            vec![
                ("a".to_string(), 0),
                ("b".to_string(), 1),
                ("c".to_string(), 2),
                ("ghost".to_string(), 0),
                ("self".to_string(), 0),
            ]
        );
        // 互相指成环的分组也必须都出现在树里（宁可平铺显示）
        groups.push(treq_core::Group {
            id: "x".into(),
            parent: Some("y".into()),
            name: "x".into(),
            requests: vec![],
        });
        groups.push(treq_core::Group {
            id: "y".into(),
            parent: Some("x".into()),
            name: "y".into(),
            requests: vec![],
        });
        assert_eq!(group_tree(&groups).len(), groups.len());
    }

    #[test]
    fn flatten_tree_nested_depth_and_collapse() {
        let c = col_nested(
            "c1",
            &[("a", None), ("b", Some("a")), ("c", Some("b"))],
        );
        let none = HashSet::new();
        assert_eq!(
            rows_of(&c, &none, ""),
            vec![
                ("c1".to_string(), 0),
                ("a".to_string(), 1),
                ("a-r".to_string(), 2),
                ("b".to_string(), 2),
                ("b-r".to_string(), 3),
                ("c".to_string(), 3),
                ("c-r".to_string(), 4),
            ]
        );
        // 折叠「a」：b、c 及其请求都不摊平
        let collapsed: HashSet<String> = ["a".to_string()].into_iter().collect();
        assert_eq!(
            rows_of(&c, &collapsed, ""),
            vec![("c1".to_string(), 0), ("a".to_string(), 1)]
        );
        // 折叠「b」：只少 b 的下级，a 自己的请求还在
        let collapsed: HashSet<String> = ["b".to_string()].into_iter().collect();
        assert_eq!(
            rows_of(&c, &collapsed, ""),
            vec![
                ("c1".to_string(), 0),
                ("a".to_string(), 1),
                ("a-r".to_string(), 2),
                ("b".to_string(), 2),
            ]
        );
        // 过滤命中深层请求：祖先链整条留着，同级的不留
        assert_eq!(
            rows_of(&c, &none, "c-r"),
            vec![
                ("c1".to_string(), 0),
                ("a".to_string(), 1),
                ("b".to_string(), 2),
                ("c".to_string(), 3),
                ("c-r".to_string(), 4),
            ]
        );
    }

    #[test]
    fn group_ancestors_walks_the_whole_parent_chain() {
        let mut c = col_nested("c1", &[("a", None), ("b", Some("a")), ("c", Some("b"))]);
        c.requests = vec![req("root-r")];
        let cols = vec![c];
        assert_eq!(
            group_ancestors(&cols, "c"),
            vec!["c1".to_string(), "a".to_string(), "b".to_string()]
        );
        assert_eq!(group_ancestors(&cols, "a"), vec!["c1".to_string()]);
        assert_eq!(group_ancestors(&cols, "nope"), Vec::<String>::new());
        // 请求：分组祖先链 + 所在分组自己
        assert_eq!(
            ancestor_ids(&cols, &Selection::Request("c-r".into())),
            vec![
                "c1".to_string(),
                "a".to_string(),
                "b".to_string(),
                "c".to_string()
            ]
        );
        // 集合直属请求只留集合
        assert_eq!(
            ancestor_ids(&cols, &Selection::Request("root-r".into())),
            vec!["c1".to_string()]
        );
    }

    #[test]
    fn group_subtree_ids_includes_all_descendants() {
        let c = col_nested(
            "c1",
            &[("a", None), ("b", Some("a")), ("c", Some("b")), ("d", None)],
        );
        let mut ids = group_subtree_ids(&c.groups, "a");
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "b".into(), "c".into()]);
        assert_eq!(group_subtree_ids(&c.groups, "d"), vec!["d".to_string()]);
    }

    #[test]
    fn collapse_all_toggles_between_any_expanded_and_none() {
        let cols = vec![col("c1", &["g1"], &["r1"])];
        let mut collapsed = HashSet::new();
        assert!(any_expanded(&cols, &collapsed));
        // 折叠全部：集合 + 分组全部标记
        let ids: Vec<String> = collapse_ids(&cols).iter().map(|s| s.to_string()).collect();
        assert_eq!(ids, vec!["c1", "g1"]);
        collapsed.extend(ids);
        assert!(!any_expanded(&cols, &collapsed), "全折叠后不该还有展开项");
        // 展开全部
        collapsed.clear();
        assert!(any_expanded(&cols, &collapsed));
        // 空集合不影响判断
        assert!(!any_expanded(&[], &HashSet::new()));
    }

    #[test]
    fn ancestors_keeps_the_path_to_the_node_open() {
        let cols = vec![col("c1", &["g1"], &["r1"]), col("c2", &[], &[])];
        // 集合是根：没有祖先
        assert!(ancestor_ids(&cols, &Selection::Collection("c1".into())).is_empty());
        // 分组：只到它所在的集合
        assert_eq!(
            ancestor_ids(&cols, &Selection::Group("g1".into())),
            vec!["c1".to_string()]
        );
        // 集合根下的请求：集合要展开
        assert_eq!(
            ancestor_ids(&cols, &Selection::Request("r1".into())),
            vec!["c1".to_string()]
        );
        // 分组里的请求：集合 + 分组都要展开（否则连行都看不见）
        assert_eq!(
            ancestor_ids(&cols, &Selection::Request("g1-r".into())),
            vec!["c1".to_string(), "g1".to_string()]
        );
        // 找不到（比如已删除）：不保留任何展开项
        assert!(ancestor_ids(&cols, &Selection::Request("没了".into())).is_empty());
    }

    #[test]
    fn new_item_target_follows_the_selection() {
        let cols = vec![col("c1", &["g1"], &["r1"]), col("c2", &[], &[])];
        // 选中分组里的请求 → 放进那个分组
        assert_eq!(
            new_item_target(&cols, Some(&Selection::Request("g1-r".into()))),
            Some(("c1".into(), Some("g1".into())))
        );
        // 集合直属请求 → 放进集合根
        assert_eq!(
            new_item_target(&cols, Some(&Selection::Request("r1".into()))),
            Some(("c1".into(), None))
        );
        // 选中分组 → 放进那个分组
        assert_eq!(
            new_item_target(&cols, Some(&Selection::Group("g1".into()))),
            Some(("c1".into(), Some("g1".into())))
        );
        // 选中集合 → 放进集合根
        assert_eq!(
            new_item_target(&cols, Some(&Selection::Collection("c2".into()))),
            Some(("c2".into(), None))
        );
        // 没选 / 选中的东西已经不在了 → 兜底第一个集合
        assert_eq!(new_item_target(&cols, None), Some(("c1".into(), None)));
        assert_eq!(
            new_item_target(&cols, Some(&Selection::Request("已删除".into()))),
            Some(("c1".into(), None))
        );
        // 一条集合都没有 → 没地方放（调用方会去建集合）
        assert_eq!(new_item_target(&[], None), None);
    }

    #[test]
    fn collapse_all_except_keeps_path_open() {
        let cols = vec![col("c1", &["g1"], &["r1"]), col("c2", &["g2"], &[])];
        let f = |sel: &Selection, open: bool| {
            let mut ids: Vec<String> = collapse_all_except_ids(&cols, sel, open)
                .into_iter()
                .collect();
            ids.sort();
            ids
        };
        // 分组里的请求：c1、g1 这条路径展开，其它都折
        assert_eq!(
            f(&Selection::Request("g1-r".into()), false),
            vec!["c2".to_string(), "g2".to_string()]
        );
        // 集合直属请求：只留集合展开
        assert_eq!(
            f(&Selection::Request("r1".into()), false),
            vec!["c2".to_string(), "g1".to_string(), "g2".to_string()]
        );
        // 分组原本展开 → 它自己也不折；原本折着 → 继续折
        assert_eq!(
            f(&Selection::Group("g1".into()), true),
            vec!["c2".to_string(), "g2".to_string()]
        );
        assert_eq!(
            f(&Selection::Group("g1".into()), false),
            vec!["c2".to_string(), "g1".to_string(), "g2".to_string()],
            "祖先集合仍展开，自己继续折着"
        );
        // 集合：只有它自己可能保留（原来展开）；它的分组不在路径上，照样折
        assert_eq!(
            f(&Selection::Collection("c1".into()), true),
            vec!["c2".to_string(), "g1".to_string(), "g2".to_string()]
        );
        // 找不到的节点：全折（调用方仍会把它 reveal 出来，但至少不误伤）
        assert_eq!(
            f(&Selection::Request("已删除".into()), false).len(),
            4,
            "全折"
        );
    }

    #[test]
    fn node_row_index_finds_requests_flat_and_grouped() {
        let cols = vec![col("c1", &["g1"], &["r1"])];
        let flat = flatten_tree(&cols, "", &HashSet::new(), None, None);
        let rows: Vec<TreeRow> = flat
            .into_iter()
            .map(|(at, st)| TreeRow {
                at,
                status: None,
                depth: st.depth,
                selected: st.selected,
                renaming: st.renaming,
                collapsed: st.collapsed,
            })
            .collect();
        // 行序：集合 / 分组 / 分组内请求 / 集合直属请求（flatten_tree 的顺序）
        assert_eq!(
            node_row_index(&rows, &cols, &Selection::Request("g1-r".into())),
            Some(2)
        );
        assert_eq!(
            node_row_index(&rows, &cols, &Selection::Request("r1".into())),
            Some(3)
        );
        assert_eq!(
            node_row_index(&rows, &cols, &Selection::Request("不存在".into())),
            None
        );
        // 折叠后行消失 → 找不到（模型会先展开祖先再定位）
        let mut collapsed = HashSet::new();
        collapsed.insert("c1".to_string());
        let flat = flatten_tree(&cols, "", &collapsed, None, None);
        let rows: Vec<TreeRow> = flat
            .into_iter()
            .map(|(at, st)| TreeRow {
                at,
                status: None,
                depth: st.depth,
                selected: st.selected,
                renaming: st.renaming,
                collapsed: st.collapsed,
            })
            .collect();
        assert_eq!(
            node_row_index(&rows, &cols, &Selection::Request("r1".into())),
            None
        );
    }

    #[test]
    fn node_row_index_finds_every_kind() {
        let cols = vec![col("c1", &["g1"], &["r1"]), col("c2", &[], &[])];
        let rows: Vec<TreeRow> = flatten_tree(&cols, "", &HashSet::new(), None, None)
            .into_iter()
            .map(|(at, st)| TreeRow {
                at,
                status: None,
                depth: st.depth,
                selected: st.selected,
                renaming: st.renaming,
                collapsed: st.collapsed,
            })
            .collect();
        // 行序：c1 / g1 / g1-r / r1 / c2
        assert_eq!(node_row_index(&rows, &cols, &Selection::Collection("c1".into())), Some(0));
        assert_eq!(node_row_index(&rows, &cols, &Selection::Group("g1".into())), Some(1));
        assert_eq!(node_row_index(&rows, &cols, &Selection::Request("g1-r".into())), Some(2));
        assert_eq!(node_row_index(&rows, &cols, &Selection::Request("r1".into())), Some(3));
        assert_eq!(node_row_index(&rows, &cols, &Selection::Collection("c2".into())), Some(4));
        // 类型对不上/不存在都不能误命中
        assert_eq!(node_row_index(&rows, &cols, &Selection::Group("c1".into())), None);
        assert_eq!(node_row_index(&rows, &cols, &Selection::Collection("g1".into())), None);
        assert_eq!(node_row_index(&rows, &cols, &Selection::Request("没有".into())), None);
    }

    struct H {
        ws: PathBuf,
        idx: String,
    }

    impl H {
        fn new(
            ws: &str,
            ws_name: &str,
            name: &str,
            url: &str,
            method: &str,
            container: &str,
        ) -> Self {
            Self {
                ws: ws.into(),
                idx: haystack(&[name, url, method, container, ws_name]),
            }
        }
    }

    impl Hit for H {
        fn workspace(&self) -> &Path {
            &self.ws
        }
        fn haystack(&self) -> &str {
            &self.idx
        }
    }

    fn hits() -> Vec<H> {
        vec![
            H::new(
                "/p/qiye",
                "企微项目",
                "客户详情",
                "http://h/customer",
                "GET",
                "客户",
            ),
            H::new(
                "/p/qiye",
                "企微项目",
                "发消息",
                "http://h/msg",
                "POST",
                "消息",
            ),
            H::new(
                "/p/pay",
                "支付项目",
                "下单",
                "http://h/order",
                "POST",
                "订单",
            ),
            H::new(
                "/p/pay",
                "支付项目",
                "查单",
                "http://h/order/1",
                "GET",
                "订单",
            ),
        ]
    }

    #[test]
    fn current_workspace_first() {
        let h = hits();
        let r = rank_hits(&h, Path::new("/p/pay"), "", 10);
        assert_eq!(r, vec![2, 3, 0, 1], "当前工作区命中应排在前面");
    }

    #[test]
    fn matches_name_url_method_container_workspace() {
        let h = hits();
        let cur = Path::new("/p/qiye");
        assert_eq!(rank_hits(&h, cur, "下单", 10), vec![2]);
        assert_eq!(rank_hits(&h, cur, "order/1", 10), vec![3]);
        assert_eq!(
            rank_hits(&h, cur, "get", 10),
            vec![0, 3],
            "方法名不区分大小写"
        );
        assert_eq!(rank_hits(&h, cur, "订单", 10), vec![2, 3]);
        assert_eq!(
            rank_hits(&h, cur, "企微项目", 10),
            vec![0, 1],
            "可搜工作区名"
        );
        assert!(rank_hits(&h, cur, "不存在的关键字", 10).is_empty());
    }

    #[test]
    fn contains_ci_is_case_insensitive_without_alloc() {
        assert!(contains_ci("Content-Type", "content"));
        assert!(contains_ci("Content-Type", "type"));
        assert!(!contains_ci("Content-Type", "types"));
        assert!(
            contains_ci("客户详情/queryUser", "queryuser"),
            "URL 段也要能大小写无关命中"
        );
        assert!(contains_ci("客户详情", "客户"), "中文按原样匹配");
        assert!(!contains_ci("客户详情", "用户"));
        assert!(contains_ci("任意", ""), "空关键字全命中");
        assert!(!contains_ci("ab", "abc"));
        // 多字节字符的续字节不能造成错位假命中（U+00A2 = C2 A2，后一字节与「客」的尾字节相同）
        assert!(!contains_ci("客", "¢"), "不得按续字节错位匹配");
        assert!(contains_ci("客户", "户"), "完整字符当然能匹配");
    }

    #[test]
    fn haystack_joins_lowercased_parts() {
        let h = haystack(&["客户详情", "http://h/X", "GET", "客户"]);
        assert_eq!(h, "客户详情\nhttp://h/x\nget\n客户");
        assert!(contains_ci(&h, "http://h/x"));
        assert!(haystack(&["", ""]).is_empty());
    }

    fn tree() -> Vec<Collection> {
        use treq_core::{Body, BodyKind};
        let req = |id: &str, name: &str| treq_core::RequestItem {
            id: id.into(),
            name: name.into(),
            method: "GET".into(),
            url: String::new(),
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
        };
        vec![
            Collection {
                id: "c1".into(),
                name: "企微".into(),
                groups: vec![
                    treq_core::Group {
                        id: "g1".into(),
                        parent: None,
                        name: "客户".into(),
                        requests: vec![req("r1", "客户详情")],
                    },
                    treq_core::Group {
                        id: "g2".into(),
                        parent: None,
                        name: "消息".into(),
                        requests: vec![req("r2", "发消息")],
                    },
                ],
                requests: vec![req("r3", "顶层接口")],
            },
            Collection {
                id: "c2".into(),
                name: "支付".into(),
                groups: vec![],
                requests: vec![req("r4", "下单")],
            },
        ]
    }

    #[test]
    fn flatten_keeps_all_rows_when_no_filter() {
        let cols = tree();
        let rows = flatten_tree(&cols, "", &HashSet::new(), None, None);
        // c1 + g1 + r1 + g2 + r2 + r3 + c2 + r4
        assert_eq!(rows.len(), 8);
        assert_eq!(
            rows[0].0,
            RowRef {
                col: 0,
                grp: None,
                req: None
            }
        );
        assert_eq!(
            rows[2].0,
            RowRef {
                col: 0,
                grp: Some(0),
                req: Some(0)
            }
        );
        assert_eq!(
            rows[7].0,
            RowRef {
                col: 1,
                grp: None,
                req: Some(0)
            }
        );
    }

    #[test]
    fn flatten_filters_and_keeps_matching_subtrees() {
        let cols = tree();
        // 关键字命中分组名 → 该分组整体保留（含不匹配的请求）
        let rows = flatten_tree(&cols, "客户", &HashSet::new(), None, None);
        assert_eq!(rows.len(), 3, "集合头条 + 命中的分组 + 其请求");
        assert_eq!(
            rows[1].0,
            RowRef {
                col: 0,
                grp: Some(0),
                req: None
            }
        );
        // 命中请求名 → 只保留该请求
        let rows = flatten_tree(&cols, "下单", &HashSet::new(), None, None);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1].0,
            RowRef {
                col: 1,
                grp: None,
                req: Some(0)
            }
        );
        // 无命中 → 整棵树都不出现
        assert!(flatten_tree(&cols, "不存在的接口", &HashSet::new(), None, None).is_empty());
        // 大小写不敏感
        let cols2 = vec![Collection {
            id: "c".into(),
            name: "X".into(),
            groups: vec![],
            requests: vec![{
                let mut r = tree().remove(0).requests.remove(0);
                r.name = "getUserInfo".into();
                r
            }],
        }];
        assert_eq!(
            flatten_tree(&cols2, "getuserinfo", &HashSet::new(), None, None).len(),
            2
        );
    }

    #[test]
    fn flatten_respects_collapse_but_filter_overrides_it() {
        let cols = tree();
        let mut collapsed = HashSet::new();
        collapsed.insert("c1".to_string());
        let rows = flatten_tree(&cols, "", &collapsed, None, None);
        assert_eq!(rows.len(), 3, "折叠集合后只剩集合头条 + c2 + r4");
        assert!(rows[0].1.collapsed);
        collapsed.insert("g1".to_string());
        let rows = flatten_tree(&cols, "", &collapsed, None, None);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].1.collapsed);
        // 过滤时忽略折叠：结果必须可见
        let rows = flatten_tree(&cols, "客户详情", &collapsed, None, None);
        assert_eq!(rows.len(), 3);
        assert!(!rows[1].1.collapsed, "过滤状态下分组要展开");
    }

    #[test]
    fn flatten_marks_selection_and_rename() {
        let cols = tree();
        let sel = Selection::Request("r2".into());
        let ren = Selection::Group("g1".into());
        let rows = flatten_tree(&cols, "", &HashSet::new(), Some(&sel), Some(&ren));
        let state = |r: RowRef| rows.iter().find(|(at, _)| *at == r).unwrap().1;
        assert!(
            state(RowRef {
                col: 0,
                grp: Some(1),
                req: Some(0)
            })
            .selected
        );
        assert!(
            !state(RowRef {
                col: 0,
                grp: Some(1),
                req: Some(0)
            })
            .renaming
        );
        assert!(
            state(RowRef {
                col: 0,
                grp: Some(0),
                req: None
            })
            .renaming
        );
        assert!(
            !state(RowRef {
                col: 0,
                grp: Some(0),
                req: None
            })
            .selected
        );
    }

    #[test]
    fn respects_limit() {
        let h = hits();
        assert_eq!(rank_hits(&h, Path::new("/p/qiye"), "", 2).len(), 2);
        assert_eq!(rank_hits(&h, Path::new("/p/qiye"), "", 2), vec![0, 1]);
    }

    #[test]
    fn move_targets_skips_current_and_filters() {
        let cols = vec![col("A", &["g1", "g2"], &["r1"]), col("B", &["h1"], &[])];
        // 空关键字：A 的根目录（r1 在那儿 → 跳过）、A/g1、A/g2、B 根、B/h1
        let all = move_targets(&cols, "r1", "");
        let labels: Vec<&str> = all.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, vec!["A / g1", "A / g2", "B", "B / h1"]);
        assert!(all.iter().all(|t| !t.collection_id.is_empty()));
        assert_eq!(all[0].group_id.as_deref(), Some("g1"));
        assert!(all[2].group_id.is_none(), "B 根目录是无分组的候选");

        // 请求在分组里：那个分组要被跳过
        let labels: Vec<String> = move_targets(&cols, "g1-r", "")
            .into_iter()
            .map(|t| t.label)
            .collect();
        assert!(!labels.contains(&"A / g1".to_string()), "当前所在分组不列");
        assert!(labels.contains(&"A".to_string()), "但可以挪回集合根目录");

        // 关键字过滤（大小写不敏感，匹配分组名）
        let labels: Vec<String> = move_targets(&cols, "r1", "H1")
            .into_iter()
            .map(|t| t.label)
            .collect();
        assert_eq!(labels, vec!["B / h1"]);

        // 找不到：空列表（界面显示「没有匹配的目标」）
        assert!(move_targets(&cols, "r1", "不存在").is_empty());
        // 请求不存在时不崩、全列出来
        assert_eq!(move_targets(&cols, "nope", "").len(), 5);
    }
}
