//! 回收站：删除的集合/分组/请求移动到 trash 目录，可恢复。
//! 目录结构（id 全局唯一，无同名冲突）
// - 集合：`trash/<collection_id>/`（含 collection.yml + requests/）
// - 分组：`trash/<collection_id>/groups/<group_id>/`（含 group.yml + requests/）
// - 集合下的散件请求：`trash/<collection_id>/<request_id>.yml`
// - 分组下的请求：`trash/<collection_id>/groups/<group_id>/requests/<request_id>.yml`
// 分组整体删掉后再恢复时，上面这层 `groups/<gid>/` 一起搬回工作区。

use crate::models::*;
use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

/// 递归移动 src 下所有条目到 dst（dst 已存在时合并目录）。
fn move_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.path().is_dir() {
            move_tree(&e.path(), &to)?;
        } else if !to.exists() {
            fs::rename(e.path(), &to)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrashKind {
    Collection,
    Group,
    Request,
}

#[derive(Debug, Clone)]
pub struct TrashEntry {
    pub kind: TrashKind,
    pub collection_id: String,
    /// 分组内请求/分组本身才有
    pub group_id: Option<String>,
    pub request_id: Option<String>,
    pub name: String,
}

fn read_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_yaml::from_str(&fs::read_to_string(path).ok()?).ok()
}

pub struct TrashStore {
    root: PathBuf,
}

impl TrashStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// 删除集合：移动整个目录到 trash/<collection_id>/。
    /// 若 trash 里已有该集合目录（先删过散件请求），合并内容进去。
    pub fn trash_collection(&self, ws_root: &Path, collection_id: &str) -> Result<()> {
        let from = ws_root.join(format!("collections/{}", collection_id));
        if !from.is_dir() {
            bail!("collection {} not found", collection_id);
        }
        let to = self.root.join(collection_id);
        fs::create_dir_all(to.parent().unwrap())?;
        if !to.exists() {
            fs::rename(&from, &to)?;
            return Ok(());
        }
        // 合并：把 from 下每个条目移入 to（requests/ 目录递归合并）
        move_tree(&from, &to)?;
        fs::remove_dir_all(&from)?;
        Ok(())
    }

    /// 删除分组：移动 collections/<cid>/groups/<gid> 整个目录到 trash 下同一相对位置。
    pub fn trash_group(&self, ws_root: &Path, collection_id: &str, group_id: &str) -> Result<()> {
        let from = ws_root.join(format!("collections/{}/groups/{}", collection_id, group_id));
        if !from.is_dir() {
            bail!("group {} not found", group_id);
        }
        let to = self.root.join(collection_id).join("groups").join(group_id);
        fs::create_dir_all(to.parent().unwrap())?;
        if !to.exists() {
            fs::rename(&from, &to)?;
        } else {
            move_tree(&from, &to)?;
            fs::remove_dir_all(&from)?;
        }
        Ok(())
    }

    /// 删除请求：移动到 trash 下与工作区同构的位置（集合散件 / 分组内），恢复时按同一路径搬回。
    pub fn trash_request(
        &self,
        ws_root: &Path,
        collection_id: &str,
        group_id: Option<&str>,
        request_id: &str,
    ) -> Result<()> {
        let (from, to) = match group_id {
            Some(gid) => (
                ws_root.join(format!(
                    "collections/{}/groups/{}/requests/{}.yml",
                    collection_id, gid, request_id
                )),
                self.root
                    .join(collection_id)
                    .join("groups")
                    .join(gid)
                    .join("requests")
                    .join(format!("{}.yml", request_id)),
            ),
            None => (
                ws_root.join(format!(
                    "collections/{}/requests/{}.yml",
                    collection_id, request_id
                )),
                self.root
                    .join(collection_id)
                    .join(format!("{}.yml", request_id)),
            ),
        };
        if !from.is_file() {
            bail!("request {} not found", request_id);
        }
        fs::create_dir_all(to.parent().unwrap())?;
        fs::rename(&from, &to)?;
        Ok(())
    }

    /// 恢复：移动回工作区。目标已存在时报错（不覆盖）。
    /// `group_id` 非空 = 分组或分组内的请求；`request_id` 非空 = 请求。
    pub fn restore(
        &self,
        ws_root: &Path,
        collection_id: &str,
        group_id: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<()> {
        match (group_id, request_id) {
            // 整个集合
            (None, None) => {
                let to = ws_root.join(format!("collections/{}", collection_id));
                if to.exists() {
                    bail!("collection {} already exists in workspace", collection_id);
                }
                fs::create_dir_all(to.parent().unwrap())?;
                let src = self.root.join(collection_id);
                // 散件请求（顶层 *.yml，非 collection.yml）先归位到 requests/ 子目录，
                // 否则恢复后 load() 看不见
                let requests_dir = src.join("requests");
                if requests_dir.is_dir() {
                    for f in fs::read_dir(&src)? {
                        let f = f?;
                        let fname = f.file_name().to_string_lossy().to_string();
                        if fname.ends_with(".yml") && !fname.starts_with("collection") {
                            let dst = requests_dir.join(&fname);
                            if !dst.exists() {
                                fs::rename(f.path(), &dst)?;
                            }
                        }
                    }
                }
                fs::rename(&src, &to)?;
            }
            // 整个分组
            (Some(gid), None) => {
                let to = ws_root.join(format!("collections/{}/groups/{}", collection_id, gid));
                if to.exists() {
                    bail!("group {} already exists in workspace", gid);
                }
                fs::create_dir_all(to.parent().unwrap())?;
                let src = self.root.join(collection_id).join("groups").join(gid);
                fs::rename(&src, &to)?;
            }
            // 单个请求（分组内 / 集合散件）
            (gid, Some(rid)) => {
                let (src, to) = match gid {
                    Some(gid) => (
                        self.root
                            .join(collection_id)
                            .join("groups")
                            .join(gid)
                            .join("requests")
                            .join(format!("{}.yml", rid)),
                        ws_root.join(format!(
                            "collections/{}/groups/{}/requests/{}.yml",
                            collection_id, gid, rid
                        )),
                    ),
                    None => (
                        self.root
                            .join(collection_id)
                            .join(format!("{}.yml", rid)),
                        ws_root.join(format!(
                            "collections/{}/requests/{}.yml",
                            collection_id, rid
                        )),
                    ),
                };
                if to.exists() {
                    bail!("request {} already exists in workspace", rid);
                }
                fs::create_dir_all(to.parent().unwrap())?;
                fs::rename(&src, &to)?;
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<TrashEntry>> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for e in entries.flatten() {
            let col_path = e.path();
            let col_id = col_path
                .file_name()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_default();

            // 集合（目录含 collection.yml）
            if let Some(col) = read_yaml::<Collection>(&col_path.join("collection.yml")) {
                out.push(TrashEntry {
                    kind: TrashKind::Collection,
                    collection_id: col.id,
                    group_id: None,
                    request_id: None,
                    name: col.name,
                });
            }
            // 集合下的散件请求（顶层 *.yml）
            for f in fs::read_dir(&col_path)? {
                let f = f?;
                let fname = f.file_name().to_string_lossy().to_string();
                if fname.ends_with(".yml")
                    && !fname.starts_with("collection")
                    && let Some(r) = read_yaml::<RequestItem>(&f.path())
                {
                    out.push(TrashEntry {
                        kind: TrashKind::Request,
                        collection_id: col_id.clone(),
                        group_id: None,
                        request_id: Some(r.id),
                        name: r.name,
                    });
                }
            }
            // 分组 + 分组内的请求
            let groups_dir = col_path.join("groups");
            let Ok(groups) = fs::read_dir(&groups_dir) else {
                continue;
            };
            for g in groups.flatten() {
                if !g.path().is_dir() {
                    continue;
                }
                let gid = g.file_name().to_string_lossy().to_string();
                if let Some(grp) = read_yaml::<Group>(&g.path().join("group.yml")) {
                    out.push(TrashEntry {
                        kind: TrashKind::Group,
                        collection_id: col_id.clone(),
                        group_id: Some(gid.clone()),
                        request_id: None,
                        name: grp.name,
                    });
                }
                let req_dir = g.path().join("requests");
                let Ok(reqs) = fs::read_dir(&req_dir) else {
                    continue;
                };
                for r in reqs.flatten() {
                    if let Some(req) = read_yaml::<RequestItem>(&r.path()) {
                        out.push(TrashEntry {
                            kind: TrashKind::Request,
                            collection_id: col_id.clone(),
                            group_id: Some(gid.clone()),
                            request_id: Some(req.id),
                            name: req.name,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn empty(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

