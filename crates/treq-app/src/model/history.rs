//! 请求历史：加载、从历史恢复成请求、删除

use super::*;

impl AppModel {
    // ---- 历史 ----
    pub fn load_history(&mut self, cx: &mut Context<Self>) {
        self.reload_history();
        cx.notify();
    }

    /// 只刷数据不 notify（选中变化但没有 cx 的内部路径用）。
    pub(crate) fn reload_history(&mut self) {
        // 响应区历史只属于当前请求；命令面板另用 recent_all（跨请求）
        self.history = match &self.selection {
            Some(Selection::Request(id)) => self
                .history_store
                .list_for_request(id, 200)
                .unwrap_or_default(),
            _ => vec![],
        };
        self.recent_all = self.history_store.list(50).unwrap_or_default();
    }

    /// 用历史快照新建一个请求（绝不覆盖当前选中请求），并选中它。
    /// 返回新请求 id；无可用集合或创建失败时返回 None。
    pub(crate) fn create_request_from_snapshot(&mut self, req: &RequestItem) -> Option<String> {
        let col_id = self
            .workspace
            .collections
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_else(|| {
                self.store
                    .create_collection("Imported")
                    .ok()
                    .map(|c| {
                        self.workspace.collections.push(c.clone());
                        c.id
                    })
                    .unwrap_or_default()
            });
        if col_id.is_empty() {
            return None;
        }
        let mut r = self.store.create_request(&col_id, &req.name).ok()?;
        crate::panes::apply_snapshot(&mut r, req);
        let _ = self.store.save_request(&r);
        if let Some(c) = self
            .workspace
            .collections
            .iter_mut()
            .find(|c| c.id == col_id)
        {
            c.requests.push(r.clone());
        }
        self.expand_ancestors_of(&r.id);
        self.selection = Some(Selection::Request(r.id.clone()));
        self.fields = EditorFields::new();
        self.reload_history();
        Some(r.id)
    }

    pub fn restore_history(&mut self, entry: &HistoryEntry, cx: &mut Context<Self>) {
        let req = entry.request.clone();
        match &self.selection {
            Some(Selection::Request(id)) => {
                let id = id.clone();
                let ok = self
                    .workspace
                    .collections
                    .iter_mut()
                    .flat_map(|c| {
                        c.requests
                            .iter_mut()
                            .chain(c.groups.iter_mut().flat_map(|g| g.requests.iter_mut()))
                    })
                    .find(|r| r.id == id)
                    .map(|r| {
                        crate::panes::apply_snapshot(r, &req);
                        r.clone()
                    });
                if let Some(r) = ok {
                    let _ = self.store.save_request(&r);
                }
                // 覆盖后输入框实体仍是旧内容，重建
                self.fields = EditorFields::new();
            }
            _ => {
                // 无选中请求：用快照新建
                self.create_request_from_snapshot(&req);
            }
        }
        cx.notify();
    }

    pub fn delete_history(&mut self, id: i64, cx: &mut Context<Self>) {
        let _ = self.history_store.delete(id);
        self.history.retain(|h| h.id != id);
        self.recent_all.retain(|h| h.id != id);
        cx.notify();
    }
}
