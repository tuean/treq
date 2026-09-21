//! 浏览器式前进 / 后退栈：记录访问过的 (工作区, 请求) 序列。
//!
//! 语义与浏览器一致：
//! - 访问新位置时，丢掉当前位置之后的前进分支，再追加；
//! - back/forward 只移动游标，不产生新记录；
//! - 连续访问同一位置不会重复入栈。

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    /// 工作区（项目）根目录
    pub workspace: PathBuf,
    /// 请求 id
    pub request: String,
}

impl NavEntry {
    pub fn new(workspace: impl Into<PathBuf>, request: impl Into<String>) -> Self {
        Self {
            workspace: workspace.into(),
            request: request.into(),
        }
    }
}

#[derive(Default)]
pub struct NavStack {
    entries: Vec<NavEntry>,
    /// 当前所处位置；None = 栈为空
    pos: Option<usize>,
}

#[allow(dead_code)]
impl NavStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn current(&self) -> Option<&NavEntry> {
        self.pos.and_then(|p| self.entries.get(p))
    }

    /// 访问新位置：截断前进分支 → 追加 → 游标移到末尾。
    /// 与当前位置相同则不重复入栈（只是刷新游标）。
    pub fn push(&mut self, entry: NavEntry) {
        if self.current() == Some(&entry) {
            return;
        }
        if let Some(p) = self.pos {
            self.entries.truncate(p + 1);
        }
        self.entries.push(entry);
        self.pos = Some(self.entries.len() - 1);
    }

    pub fn can_back(&self) -> bool {
        self.pos.is_some_and(|p| p > 0)
    }

    pub fn can_forward(&self) -> bool {
        self.pos.is_some_and(|p| p + 1 < self.entries.len())
    }

    /// 后退一格，返回要展示的位置。
    pub fn back(&mut self) -> Option<NavEntry> {
        let p = self.pos?;
        if p == 0 {
            return None;
        }
        self.pos = Some(p - 1);
        self.current().cloned()
    }

    /// 前进一格，返回要展示的位置。
    pub fn forward(&mut self) -> Option<NavEntry> {
        let p = self.pos?;
        if p + 1 >= self.entries.len() {
            return None;
        }
        self.pos = Some(p + 1);
        self.current().cloned()
    }

    /// 工作区被移除 / 请求被删除后，清掉相关记录。
    pub fn forget_request(&mut self, req_id: &str) {
        self.retain(|e| e.request != req_id);
    }

    pub fn forget_workspace(&mut self, ws: &Path) {
        self.retain(|e| e.workspace != ws);
    }

    fn retain(&mut self, keep: impl Fn(&NavEntry) -> bool) {
        let cur = self.current().cloned();
        self.entries.retain(keep);
        if self.entries.is_empty() {
            self.pos = None;
            return;
        }
        // 游标落到「原当前位置之后第一个保留下来的项」，没有就回到末尾
        self.pos = match cur {
            Some(c) => self
                .entries
                .iter()
                .position(|e| *e == c)
                .or_else(|| Some(self.entries.len() - 1)),
            None => Some(self.entries.len() - 1),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(req: &str) -> NavEntry {
        NavEntry::new("/ws", req)
    }

    #[test]
    fn push_back_forward() {
        let mut n = NavStack::new();
        assert!(!n.can_back() && !n.can_forward());
        n.push(e("a"));
        n.push(e("b"));
        n.push(e("c"));
        assert_eq!(n.current().unwrap().request, "c");
        assert!(n.can_back() && !n.can_forward());

        assert_eq!(n.back().unwrap().request, "b");
        assert_eq!(n.back().unwrap().request, "a");
        assert!(!n.can_back(), "到栈底不能继续后退");
        assert_eq!(n.back(), None, "栈底后退应返回 None");
        assert!(n.can_forward());

        assert_eq!(n.forward().unwrap().request, "b");
        assert_eq!(n.forward().unwrap().request, "c");
        assert_eq!(n.forward(), None);
    }

    #[test]
    fn new_visit_truncates_forward_branch() {
        let mut n = NavStack::new();
        n.push(e("a"));
        n.push(e("b"));
        n.push(e("c"));
        n.back();
        n.back(); // 当前 a，前进分支是 b, c
        n.push(e("d"));
        assert!(!n.can_forward(), "新访问必须丢掉前进分支");
        assert_eq!(n.current().unwrap().request, "d");
        assert_eq!(n.back().unwrap().request, "a");
        assert_eq!(n.forward().unwrap().request, "d");
    }

    #[test]
    fn same_entry_not_duplicated() {
        let mut n = NavStack::new();
        n.push(e("a"));
        n.push(e("a"));
        assert_eq!(n.len(), 1);
        assert!(!n.can_back());
    }

    #[test]
    fn forget_keeps_cursor_valid() {
        let mut n = NavStack::new();
        n.push(e("a"));
        n.push(e("b"));
        n.push(e("c"));
        n.back(); // 当前 b
        n.forget_request("c");
        assert_eq!(n.len(), 2);
        assert_eq!(n.current().unwrap().request, "b");
        assert!(!n.can_forward());

        n.forget_request("b");
        assert_eq!(n.current().unwrap().request, "a");
        n.forget_request("a");
        assert!(n.is_empty());
        assert_eq!(n.current(), None);
    }

    #[test]
    fn workspace_is_part_of_identity() {
        let mut n = NavStack::new();
        n.push(NavEntry::new("/ws1", "r"));
        n.push(NavEntry::new("/ws2", "r"));
        assert_eq!(n.len(), 2, "同一请求 id 在不同工作区是不同位置");
        assert_eq!(n.back().unwrap().workspace, PathBuf::from("/ws1"));
    }
}
