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

    // ---- 空间快速切换器（⌘⇧K）：可搜索、↑↓ 选、回车切 ----

    /// 重建空间列表：按名字过滤，当前空间置顶并打 ●，附上落点目录名便于区分同名。
    pub(crate) fn space_palette_rebuild(&mut self) {
        let cur = self.workspace_path();
        let q = self
            .space_palette
            .as_ref()
            .map(|p| p.query.trim().to_lowercase())
            .unwrap_or_default();
        let mut items: Vec<(PathBuf, String)> = self
            .settings
            .workspaces
            .iter()
            .filter(|w| {
                q.is_empty()
                    || w.name.to_lowercase().contains(&q)
                    || w.path.to_string_lossy().to_lowercase().contains(&q)
            })
            .map(|w| {
                let mark = if w.path == cur { "● " } else { "   " };
                let dir = w
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                (w.path.clone(), format!("{mark}{}   ·  {dir}", w.name))
            })
            .collect();
        items.sort_by(|a, b| {
            let (ac, bc) = (a.0 == cur, b.0 == cur);
            bc.cmp(&ac).then_with(|| a.1.cmp(&b.1))
        });
        if let Some(p) = self.space_palette.as_mut() {
            p.list = items;
            p.selected = p.selected.min(p.list.len().saturating_sub(1));
        }
    }

    pub fn open_space_palette(&mut self, cx: &mut Context<Self>) {
        let handle = cx.entity();
        let field = TextField::new(
            self.space_query.clone().into(),
            SharedString::from(self.t("space.switch.placeholder")),
            Arc::new(move |s: &str, app: &mut App| {
                let s = s.to_string();
                handle.update(app, |this, cx| this.space_set_query(s.clone(), cx));
            }),
            cx,
        );
        let h = cx.entity();
        field.update(cx, |f, _cx| {
            f.on_submit = Some(Arc::new(move |_window, app| {
                let h = h.clone();
                app.defer(move |app| h.update(app, |this, cx| this.space_pick_selected(cx)));
            }));
        });
        self.space_palette = Some(SpacePalette {
            field,
            query: self.space_query.clone(),
            list: Vec::new(),
            selected: 0,
        });
        self.space_palette_rebuild();
        cx.notify();
    }

    /// 关掉面板（记下输入，下次打开还在）。
    pub fn close_space_palette(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &self.space_palette {
            self.space_query = p.query.clone();
        }
        self.space_palette = None;
        cx.notify();
    }

    pub fn space_set_query(&mut self, q: String, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.space_palette {
            p.query = q;
            p.selected = 0;
        }
        self.space_palette_rebuild();
        cx.notify();
    }

    pub fn space_move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.space_palette
            && !p.list.is_empty()
        {
            let n = p.list.len() as isize;
            p.selected = ((p.selected as isize + delta).rem_euclid(n)) as usize;
            cx.notify();
        }
    }

    pub fn space_set_selected(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.space_palette {
            p.selected = i;
            cx.notify();
        }
    }

    /// 切到选中空间（选中的就是当前空间时只关面板）。
    pub fn space_pick_selected(&mut self, cx: &mut Context<Self>) {
        let Some(p) = &self.space_palette else {
            return;
        };
        let Some((path, _)) = p.list.get(p.selected).cloned() else {
            return;
        };
        self.space_palette = None;
        self.space_query.clear();
        self.switch_workspace(path, cx);
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
