//! 「移动到…」对话框：带过滤的集合/分组选择器（634 个请求靠右键菜单二级弹层
//! 挑不动——目标有上百个，得能打字过滤）。

use super::*;

/// 对话框状态。候选列表每次渲染现算（来自工作区树），只有关键字和选中项是状态。
pub struct MoveDialog {
    pub req_id: String,
    pub request_name: String,
    pub field: Entity<TextField>,
    pub query: String,
    pub selected: usize,
}

impl AppModel {
    pub fn open_move_dialog(&mut self, req_id: &str, cx: &mut Context<Self>) {
        let name = self
            .request_by_id(req_id)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        let handle = cx.entity();
        let handle2 = handle.clone();
        let field = TextField::new(
            "".into(),
            SharedString::from(self.t("move.placeholder")),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| this.move_set_query(s.to_string(), cx));
            }),
            cx,
        );
        field.update(cx, |f, _cx| {
            let h = handle2.clone();
            f.on_submit = Some(Arc::new(move |_window, app| {
                h.update(app, |this, cx| this.move_pick_selected(cx));
            }));
        });
        self.popup.close();
        self.move_dialog = Some(MoveDialog {
            req_id: req_id.to_string(),
            request_name: name,
            field,
            query: String::new(),
            selected: 0,
        });
        cx.notify();
    }

    pub fn close_move_dialog(&mut self, cx: &mut Context<Self>) {
        self.move_dialog = None;
        cx.notify();
    }

    pub fn move_set_query(&mut self, q: String, cx: &mut Context<Self>) {
        if let Some(d) = &mut self.move_dialog {
            d.query = q;
            d.selected = 0;
        }
        cx.notify();
    }

    /// 当前关键字下的候选（渲染与选中共用一份计算）。
    /// 关键字以 `MoveDialog::query` 为准（输入框改动经 `move_set_query` 同步过来），
    /// 这样纯逻辑、渲染、键盘三处看到的是同一份。
    pub fn move_candidates(&self) -> Vec<crate::search::MoveTarget> {
        match &self.move_dialog {
            Some(d) => {
                crate::search::move_targets(&self.workspace.collections, &d.req_id, &d.query)
            }
            None => Vec::new(),
        }
    }

    pub fn move_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.move_candidates().len() as isize;
        if n == 0 {
            return;
        }
        if let Some(d) = &mut self.move_dialog {
            d.selected = ((d.selected as isize + delta).rem_euclid(n)) as usize;
        }
        cx.notify();
    }

    pub fn move_set_selected(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(d) = &mut self.move_dialog {
            d.selected = i;
        }
        cx.notify();
    }

    /// Enter / 点击某一行：搬到选中的那个目标，然后关掉对话框。
    pub fn move_pick_selected(&mut self, cx: &mut Context<Self>) {
        let Some(d) = &self.move_dialog else { return };
        let targets = self.move_candidates();
        let Some(t) = targets.get(d.selected).cloned() else {
            return;
        };
        let req_id = d.req_id.clone();
        self.move_dialog = None; // 选完就关（点击/Enter 都一样）
        self.move_request_to(&req_id, &t.collection_id, t.group_id.as_deref(), cx);
    }
}
