//! Ctrl+K 命令面板（跨工作区请求索引 + 弹层）

use super::*;

impl AppModel {
    // ---- Ctrl+K 命令面板 ----
    /// 扫描所有已登记工作区，建一份跨区请求索引（只在打开面板时做一次）。
    pub(crate) fn rebuild_cross_index(&mut self) {
        let current = self.workspace_path();
        self.cross_index.clear();
        // 当前工作区已有内存树，不重复读盘
        let ws_name = self.workspace_name();
        let local: Vec<CrossHit> = self
            .all_requests()
            .into_iter()
            .map(|(cname, r, gname)| {
                CrossHit::new(
                    current.clone(),
                    ws_name.clone(),
                    r.clone(),
                    gname
                        .map(|g| format!("{}/{}", cname, g))
                        .unwrap_or_else(|| cname.to_string()),
                )
            })
            .collect();
        self.cross_index.extend(local);
        for ws in self.settings.workspaces.clone() {
            if ws.path == current {
                continue;
            }
            let Ok(loaded) = WorkspaceStore::new(ws.path.clone()).load() else {
                continue;
            };
            for c in &loaded.collections {
                for r in &c.requests {
                    self.cross_index.push(CrossHit::new(
                        ws.path.clone(),
                        ws.name.clone(),
                        r.clone(),
                        c.name.clone(),
                    ));
                }
                for g in &c.groups {
                    for r in &g.requests {
                        self.cross_index.push(CrossHit::new(
                            ws.path.clone(),
                            ws.name.clone(),
                            r.clone(),
                            format!("{}/{}", c.name, g.name),
                        ));
                    }
                }
            }
        }
    }

    pub fn open_quick(&mut self, cx: &mut Context<Self>) {
        self.rebuild_cross_index();
        let handle = cx.entity();
        let handle2 = handle.clone();
        let field = TextField::new(
            self.quick_query.clone().into(),
            SharedString::from(self.t("quick.placeholder")),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| this.quick_set_query(s.to_string(), cx));
            }),
            cx,
        );
        field.update(cx, |f, _cx| {
            let h = handle2.clone();
            f.on_submit = Some(Arc::new(move |_window, app| {
                h.update(app, |this, cx| this.quick_pick_selected(cx));
            }));
        });
        self.quick = Some(QuickPalette {
            field,
            query: self.quick_query.clone(),
            list: vec![],
            selected: 0,
        });
        self.quick_rebuild();
        cx.notify();
    }
}
