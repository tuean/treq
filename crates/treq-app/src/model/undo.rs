//! 删除操作的撤销栈（Ctrl/Cmd+Z）：删除时压一份快照，撤销时原样放回。

use super::*;

/// 撤销栈上限：只留最近这些步，避免无限增长。
const UNDO_MAX: usize = 50;

impl AppModel {
    /// 压一条可撤销操作（最近的在末尾）。
    pub(crate) fn push_undo(&mut self, op: UndoOp) {
        crate::widgets::debug_log("app.push_undo");
        self.undo_stack.push(op);
        if self.undo_stack.len() > UNDO_MAX {
            self.undo_stack.remove(0);
        }
    }

    /// 撤销上一次删除。焦点在输入框里时走输入框自己的文本撤销，不会到这里。
    pub fn undo_last(&mut self, cx: &mut Context<Self>) {
        crate::widgets::debug_log("app.undo_last enter");
        crate::widgets::debug_log(&("app.undo stack=".to_string() + &self.undo_stack.len().to_string()));
        crate::widgets::debug_log(
            &("app.undo stack=".to_string() + &self.undo_stack.len().to_string()),
        );
        let Some(op) = self.undo_stack.pop() else {
            crate::widgets::debug_log("app.undo_last empty");
            return;
        };
        match op {
            UndoOp::KvRestore {
                req_id,
                which,
                index,
                rows,
            } => {
                self.edit_request(&req_id, |r| {
                    let kvs = match which {
                        KvWhich::Params => &mut r.params,
                        _ => &mut r.headers,
                    };
                    let at = index.min(kvs.len());
                    for (i, row) in rows.iter().enumerate() {
                        kvs.insert((at + i).min(kvs.len()), row.clone());
                    }
                });
                self.rebuild_kv_fields(cx);
            }
            UndoOp::FormRestore {
                req_id,
                index,
                rows,
            } => {
                self.edit_request(&req_id, |r| {
                    let at = index.min(r.body.form_data.len());
                    for (i, row) in rows.iter().enumerate() {
                        let at = (at + i).min(r.body.form_data.len());
                        r.body.form_data.insert(at, row.clone());
                    }
                });
                self.rebuild_kv_fields(cx);
            }
            UndoOp::EnvVars { target, vars } => {
                // 弹框开着就当场还原；已经关了就只写回草稿，不把它重新弹出来
                let open = self
                    .env_editor
                    .as_ref()
                    .is_some_and(|ed| ed.target == target);
                self.env_drafts.insert(target.clone(), vars.clone());
                if open {
                    self.rebuild_env_fields(target, vars, cx);
                }
            }
            UndoOp::History { entry } => {
                if self.history_store.insert(&entry).is_ok() {
                    self.load_history(cx);
                }
            }
            UndoOp::Trash { entries } => {
                for (col_id, group_id, req_id) in &entries {
                    let _ = self.trash_store.restore(
                        self.store.root(),
                        col_id,
                        group_id.as_deref(),
                        req_id.as_deref(),
                    );
                }
                self.reload();
            }
            UndoOp::Cookies { jar } => {
                if let Ok(mut j) = self.cookies.lock() {
                    *j = jar;
                }
                self.save_cookie_jar();
            }
        }
        self.toast(self.t("undo.done").to_string(), cx);
        cx.notify();
    }

    /// 按 id 找请求并改它，改完落盘（撤销的目标未必是当前选中的请求）。
    pub(crate) fn edit_request(&mut self, req_id: &str, f: impl FnOnce(&mut RequestItem)) -> bool {
        let mut updated: Option<RequestItem> = None;
        'outer: for c in self.workspace.collections.iter_mut() {
            for r in c.requests.iter_mut() {
                if r.id == req_id {
                    f(r);
                    updated = Some(r.clone());
                    break 'outer;
                }
            }
            for g in c.groups.iter_mut() {
                for r in g.requests.iter_mut() {
                    if r.id == req_id {
                        f(r);
                        updated = Some(r.clone());
                        break 'outer;
                    }
                }
            }
        }
        if let Some(req) = &updated {
            let _ = self.store.save_request(req);
        }
        updated.is_some()
    }
}
