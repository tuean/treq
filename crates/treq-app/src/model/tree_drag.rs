//! 侧栏树的行拖拽：把请求/分组拖到别的分组、别的集合里，或调同层顺序。
//!
//! 拖拽用「按下先记账、移动超阈值才算拖」的土办法，不依赖 gpui 的 drag API：
//! 树的可见行矩形直接从 `ScrollHandle` 里拿（uniform_list 只渲染可见行，
//! 索引 = top_item + 可见序号），所以落点判定全天候准确，也能顺手画出插入线。

use gpui::{Bounds, Context, Pixels, Point, px};

use crate::model::{AppModel, DropTarget, DropZone, RowKind, RowRef, TreeDrag, TreePress};
use crate::search;

/// 位移超过这个像素数才算拖动（不然单击选中的手感会变差）
const DRAG_SLOP: f32 = 5.;

impl AppModel {
    /// 行上按下左键：先记下来。真正的拖动等指针动了再说。
    pub(crate) fn tree_press(&mut self, kind: RowKind, id: String, y: f32) {
        self.tree_press = Some(TreePress { kind, id, y });
    }

    /// 指针移动：越过阈值就进入拖动，并且每次都重算落点。
    pub(crate) fn tree_drag_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let y = f32::from(pos.y);
        if self.tree_drag.is_none() {
            let Some(p) = self.tree_press.clone() else {
                return;
            };
            // 集合不能嵌套集合，所以集合行只能当落点，不能被拖走
            if p.kind == RowKind::Collection || (y - p.y).abs() < DRAG_SLOP {
                return;
            }
            self.tree_drag = Some(TreeDrag {
                kind: p.kind,
                id: p.id,
                target: None,
            });
        }
        let target = self.drop_target_at(pos);
        if let Some(d) = self.tree_drag.as_mut() {
            d.target = target;
        }
        cx.notify();
    }

    /// 松手：落点有效就落下去（搬文件 + 重排 order），否则什么也不做。
    pub(crate) fn tree_drag_end(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        self.tree_press = None;
        let Some(drag) = self.tree_drag.take() else {
            return;
        };
        let Some(target) = drag.target else {
            cx.notify();
            return;
        };
        if !self.drop_allowed(&drag, target) {
            cx.notify();
            return;
        }
        let moved = if drag.kind == RowKind::Group {
            self.apply_group_drop(&drag, target)
        } else {
            self.apply_request_drop(&drag, target)
        };
        match moved {
            Ok(()) => {
                let id = drag.id.clone();
                self.reload();
                // 拖完顺手选中（请求才需要），并且把它滚进视野
                if drag.kind == RowKind::Group {
                    self.reveal_node(crate::model::Selection::Group(id.clone()), cx);
                } else {
                    self.select_request(id.clone(), window, cx);
                    self.reveal_node(crate::model::Selection::Request(id.clone()), cx);
                }
                self.toast(self.t("flash.moved").to_string(), cx);
            }
            Err(e) => {
                let msg = format!("{} {}", self.t("flash.move_failed"), e);
                self.toast(msg, cx);
            }
        }
        cx.notify();
    }

    /// 拖拽中（含刚松手）用：拿当前落点画插入线 / 高亮。
    pub fn drop_indicator(&self) -> Option<DropTarget> {
        self.tree_drag.as_ref().and_then(|d| d.target)
    }

    /// 树的行几何：uniform 行高 = 内容总高 / 行数；行 i 的屏幕 y = 视口顶 + offset + i*行高。
    /// （offset 往下滚是负值。uniform_list 不往 child_bounds 里填东西，所以
    /// `bounds_for_item`/`top_item` 在树这儿都不好使，只能自己算。）
    pub(crate) fn tree_geom(&self) -> Option<TreeGeom> {
        // last_item_size 只在 layout 过后才有；没有就说明这一帧还没量过
        let (base, total) = {
            let st = self.tree_scroll.0.borrow();
            let size = st.last_item_size.as_ref()?;
            (st.base_handle.clone(), f32::from(size.contents.height))
        };
        let count = self.rows.len();
        if count == 0 || total <= 0. {
            return None;
        }
        let view = base.bounds();
        Some(TreeGeom {
            view_top: f32::from(view.origin.y),
            view_h: f32::from(view.size.height),
            row_h: total / count as f32,
            off_y: f32::from(base.offset().y),
        })
    }

    /// 树上一行 → (行号, 屏幕矩形)。只有落在视口里的行才算数。
    /// （拖拽命中判定直接走 `tree_geom`，这个留给排查/以后要画的场景用）
    #[allow(dead_code)]
    pub(crate) fn visible_tree_rows(&self) -> Vec<(usize, Bounds<Pixels>)> {
        let Some(g) = self.tree_geom() else {
            return Vec::new();
        };
        let count = self.rows.len();
        let first = g.row_at(g.view_top).unwrap_or(0).min(count.saturating_sub(1));
        let last = g
            .row_at(g.view_top + g.view_h)
            .unwrap_or(first)
            .min(count.saturating_sub(1));
        (first..=last)
            .map(|ix| (ix, g.rect_of(ix)))
            .collect()
    }

    /// 指针下面是哪一行、落在这一行的哪个位置。
    fn drop_target_at(&self, pos: Point<Pixels>) -> Option<DropTarget> {
        let y = f32::from(pos.y);
        let Some(g) = self.tree_geom() else {
            return None;
        };
        // 指针在树视口之外就别算了（顶栏/面板上的拖拽不该命中行）
        if y < g.view_top || y > g.view_top + g.view_h {
            return None;
        }
        let ix = g.row_at(y)?;
        let rect = g.rect_of(ix);
        {
            let top = f32::from(rect.origin.y);
            let h = g.row_h.max(1.);
            let row = *self.rows.get(ix)?;
            let rel = ((y - top) / h).clamp(0., 1.);
            let kind = row_kind(row.at);
            let zone = match kind {
                // 集合行整行都算「放进去」
                RowKind::Collection => DropZone::Inside,
                // 请求行：上下各一半 = 插到前面/后面
                RowKind::Request => {
                    if rel < 0.5 {
                        DropZone::Before
                    } else {
                        DropZone::After
                    }
                }
                // 分组行：上下各 30% = 同级插到前后，中间 = 变成它的子分组
                RowKind::Group => {
                    if rel < 0.3 {
                        DropZone::Before
                    } else if rel > 0.7 {
                        DropZone::After
                    } else {
                        DropZone::Inside
                    }
                }
            };
            return Some(DropTarget { at: row.at, zone, rect });
        }
    }

    /// 落点算不算合法：分组不能拖进自己或自己的子孙里。
    fn drop_allowed(&self, drag: &TreeDrag, target: DropTarget) -> bool {
        if drag.kind != RowKind::Group {
            return true;
        }
        let Some((to_col, parent)) = self.drop_container(target) else {
            return false;
        };
        let Some(col) = self.workspace.collections.iter().find(|c| c.id == to_col) else {
            return false;
        };
        match parent {
            // 拖到顶层永远合法
            None => true,
            Some(p) => !search::group_subtree_ids(&col.groups, &drag.id).contains(&p),
        }
    }

    /// 落点对应的容器：(集合 id, 分组 id / None)
    fn drop_container(&self, target: DropTarget) -> Option<(String, Option<String>)> {
        let col = self.workspace.collections.get(target.at.col)?;
        let col_id = col.id.clone();
        match row_kind(target.at) {
            RowKind::Collection => Some((col_id, None)),
            RowKind::Request => {
                let gid = target
                    .at
                    .grp
                    .and_then(|gi| col.groups.get(gi))
                    .map(|g| g.id.clone());
                Some((col_id, gid))
            }
            RowKind::Group => {
                let g = col.groups.get(target.at.grp?)?;
                match target.zone {
                    DropZone::Inside => Some((col_id, Some(g.id.clone()))),
                    // 同级插入 = 落到「父级容器」里
                    _ => Some((col_id, g.parent.clone())),
                }
            }
        }
    }

    /// 目标容器里有顺序的请求 id（跟界面上的顺序一致）。
    fn request_ids_in(&self, col_id: &str, group: Option<&str>) -> Vec<String> {
        let Some(col) = self.workspace.collections.iter().find(|c| c.id == col_id) else {
            return Vec::new();
        };
        let list = match group {
            Some(gid) => col
                .groups
                .iter()
                .find(|g| g.id == gid)
                .map(|g| &g.requests),
            None => Some(&col.requests),
        };
        list.map(|rs| rs.iter().map(|r| r.id.clone()).collect())
            .unwrap_or_default()
    }

    /// 目标容器里有顺序的分组 id（同一父级）。
    fn group_ids_under(&self, col_id: &str, parent: Option<&str>) -> Vec<String> {
        let Some(col) = self.workspace.collections.iter().find(|c| c.id == col_id) else {
            return Vec::new();
        };
        col.groups
            .iter()
            .filter(|g| g.parent.as_deref() == parent)
            .map(|g| g.id.clone())
            .collect()
    }

    /// 把 drag 里的 id 插到 ids 的目标位置（先摘出来再插，同层上下移动才不会错位）。
    fn insert_into(ids: &mut Vec<String>, id: &str, target_id: Option<&str>, zone: DropZone) {
        let at = match (target_id, zone) {
            (_, DropZone::Inside) | (None, _) => ids.len(),
            (Some(t), DropZone::Before) => ids.iter().position(|x| x == t).unwrap_or(ids.len()),
            (Some(t), DropZone::After) => ids
                .iter()
                .position(|x| x == t)
                .map(|i| i + 1)
                .unwrap_or(ids.len()),
        };
        ids.insert(at.min(ids.len()), id.to_string());
    }

    /// 请求落下去：搬文件 + 按新顺序写 order。
    fn apply_request_drop(&self, drag: &TreeDrag, target: DropTarget) -> anyhow::Result<()> {
        let Some((to_col, to_group)) = self.drop_container(target) else {
            return Ok(());
        };
        let mut ids = self.request_ids_in(&to_col, to_group.as_deref());
        let anchor = if target.zone == DropZone::Inside {
            None
        } else {
            self.resolve_row(target.at).map(|(_, id, _, _, _)| id)
        };
        ids.retain(|x| x != &drag.id);
        Self::insert_into(&mut ids, &drag.id, anchor.as_deref(), target.zone);
        self.store
            .move_request(&drag.id, &to_col, to_group.as_deref())?;
        self.store.set_request_orders(&ids)?;
        Ok(())
    }

    /// 分组落下去：改 parent（跨集合就是搬目录）+ 写同层顺序。
    fn apply_group_drop(&self, drag: &TreeDrag, target: DropTarget) -> anyhow::Result<()> {
        let Some((to_col, parent)) = self.drop_container(target) else {
            return Ok(());
        };
        let mut ids = self.group_ids_under(&to_col, parent.as_deref());
        let anchor = if target.zone == DropZone::Inside {
            None
        } else {
            self.resolve_row(target.at).map(|(_, id, _, _, _)| id)
        };
        ids.retain(|x| x != &drag.id);
        Self::insert_into(&mut ids, &drag.id, anchor.as_deref(), target.zone);
        self.store
            .set_group_parent(&drag.id, &to_col, parent.as_deref())?;
        self.store.set_group_orders(&ids)?;
        Ok(())
    }
}

/// 树的可见行几何（uniform 行高，按滚动偏移换算屏幕坐标）。
#[derive(Clone, Copy)]
pub(crate) struct TreeGeom {
    pub(crate) view_top: f32,
    pub(crate) view_h: f32,
    pub(crate) row_h: f32,
    pub(crate) off_y: f32,
}

impl TreeGeom {
    /// 行 i 的屏幕矩形。
    pub(crate) fn rect_of(&self, ix: usize) -> Bounds<Pixels> {
        Bounds {
            origin: gpui::point(px(0.), px(self.view_top + self.off_y + ix as f32 * self.row_h)),
            size: gpui::size(px(1.), px(self.row_h)),
        }
    }

    /// 窗口 y 落在第几行（视口外也算得出来，调用方自己裁）。
    pub(crate) fn row_at(&self, y: f32) -> Option<usize> {
        if self.row_h <= 0. {
            return None;
        }
        let rel = y - self.view_top - self.off_y;
        (rel >= 0.).then(|| (rel / self.row_h) as usize)
    }
}

/// 一行是什么（TreeRow 只存位置，类型由位置推出来）。
pub fn row_kind(at: RowRef) -> RowKind {
    match (at.grp, at.req) {
        (_, Some(_)) => RowKind::Request,
        (Some(_), None) => RowKind::Group,
        (None, None) => RowKind::Collection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn insert_before_and_after_anchor() {
        let mut list = ids(&["a", "c"]);
        AppModel::insert_into(&mut list, "b", Some("c"), DropZone::Before);
        assert_eq!(list, ids(&["a", "b", "c"]));
        let mut list = ids(&["a", "b"]);
        AppModel::insert_into(&mut list, "c", Some("b"), DropZone::After);
        assert_eq!(list, ids(&["a", "b", "c"]));
        // Inside / 没锚点 = 放到最后
        let mut list = ids(&["a"]);
        AppModel::insert_into(&mut list, "b", Some("a"), DropZone::Inside);
        assert_eq!(list, ids(&["a", "b"]));
    }

    #[test]
    fn reorder_within_same_container_does_not_shift() {
        // b 往上拖到 a 前面：先摘出来再插，位置才不会差一位
        let mut list = ids(&["a", "b", "c"]);
        list.retain(|x| x != "b");
        AppModel::insert_into(&mut list, "b", Some("a"), DropZone::Before);
        assert_eq!(list, ids(&["b", "a", "c"]));
        // b 往下拖到 c 后面
        let mut list = ids(&["a", "b", "c"]);
        list.retain(|x| x != "b");
        AppModel::insert_into(&mut list, "b", Some("c"), DropZone::After);
        assert_eq!(list, ids(&["a", "c", "b"]));
    }

    #[test]
    fn row_kind_from_position() {
        assert_eq!(
            row_kind(RowRef { col: 0, grp: None, req: None }),
            RowKind::Collection
        );
        assert_eq!(
            row_kind(RowRef { col: 0, grp: Some(1), req: None }),
            RowKind::Group
        );
        assert_eq!(
            row_kind(RowRef { col: 0, grp: Some(1), req: Some(2) }),
            RowKind::Request
        );
    }
}
