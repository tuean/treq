use crate::models::*;
use anyhow::{Context as _, Result, anyhow};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 读取目录下所有 *.yml 请求，连同文件路径（用于填充索引）。
fn read_requests(dir: &Path) -> Result<Vec<(PathBuf, RequestItem)>> {
    let mut reqs = Vec::new();
    if dir.is_dir() {
        for f in fs::read_dir(dir)? {
            let f = f?;
            let path = f.path();
            if path.extension().map(|e| e == "yml").unwrap_or(false) {
                match serde_yaml::from_str::<RequestItem>(&fs::read_to_string(&path)?) {
                    Ok(r) => reqs.push((path, r)),
                    // 静默丢掉会让接口从界面上凭空消失，至少留一行日志
                    Err(e) => eprintln!("treq: 跳过无法解析的请求文件 {}：{}", path.display(), e),
                }
            }
        }
    }
    Ok(reqs)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub struct WorkspaceStore {
    root: PathBuf,
    /// 请求 id → 请求文件路径索引：`load()` 时建立，增删改时维护。
    /// 没有它就每次保存都要扫 `collections/*/groups/*`（1225 条数据下约 385 次 readdir/次）。
    index: Mutex<HashMap<String, PathBuf>>,
}

impl WorkspaceStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            index: Mutex::new(HashMap::new()),
        }
    }

    fn index(&self) -> std::sync::MutexGuard<'_, HashMap<String, PathBuf>> {
        // 中毒的锁里数据仍然可用（只是写了一半时 panic），直接取回
        self.index.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 索引里的请求数（诊断/测试用）。
    pub fn indexed_requests(&self) -> usize {
        self.index().len()
    }

    /// 按路径前缀清理索引（删除集合/分组时用）。
    fn index_purge_prefix(&self, prefix: &Path) {
        self.index().retain(|_, p| !p.starts_with(prefix));
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("collections"))?;
        fs::create_dir_all(self.root.join("environments"))?;
        let base_path = self.root.join("environments/base.yml");
        if !base_path.exists() {
            let base = Environment {
                id: new_id(),
                name: "Base".into(),
                variables: BTreeMap::new(),
            };
            fs::write(base_path, serde_yaml::to_string(&base)?)?;
        }
        Ok(())
    }

    pub fn load(&self) -> Result<Workspace> {
        let mut collections = Vec::new();
        for entry in fs::read_dir(self.root.join("collections"))? {
            let entry = entry?;
            let col_dir = entry.path();
            if !col_dir.is_dir() {
                continue;
            }
            let meta = col_dir.join("collection.yml");
            if !meta.exists() {
                continue;
            }
            let mut col: Collection = serde_yaml::from_str(&fs::read_to_string(&meta)?)
                .with_context(|| format!("bad collection.yml in {}", col_dir.display()))?;
            col.requests = read_requests(&col_dir.join("requests"))?
                .into_iter()
                .map(|(p, r)| {
                    self.index().insert(r.id.clone(), p);
                    r
                })
                .collect();
            // 分组：collections/<cid>/groups/<gid>/group.yml + requests/
            let groups_dir = col_dir.join("groups");
            col.groups = if groups_dir.is_dir() {
                let mut groups = Vec::new();
                for g in fs::read_dir(&groups_dir)? {
                    let g = g?;
                    let g_dir = g.path();
                    let g_meta = g_dir.join("group.yml");
                    if g_meta.is_file() {
                        let mut group: Group = serde_yaml::from_str(&fs::read_to_string(&g_meta)?)
                            .with_context(|| format!("bad group.yml in {}", g_dir.display()))?;
                        group.requests = read_requests(&g_dir.join("requests"))?
                            .into_iter()
                            .map(|(p, r)| {
                                self.index().insert(r.id.clone(), p);
                                r
                            })
                            .collect();
                        groups.push(group);
                    }
                }
                groups
            } else {
                Vec::new()
            };
            // 悬空/自引用的 parent（手改 yml、回收站还原了子分组）当第一层，
            // 否则这些分组在树里永远看不见。
            let ids: std::collections::HashSet<String> =
                col.groups.iter().map(|g| g.id.clone()).collect();
            for g in col.groups.iter_mut() {
                if g.parent.as_ref().is_some_and(|p| p == &g.id || !ids.contains(p)) {
                    g.parent = None;
                }
            }
            collections.push(col);
        }
        let env_dir = self.root.join("environments");
        let mut base_env = Environment {
            id: new_id(),
            name: "Base".into(),
            variables: BTreeMap::new(),
        };
        let mut environments = Vec::new();
        for f in fs::read_dir(&env_dir)? {
            let f = f?;
            if f.path().extension().map(|e| e == "yml").unwrap_or(false) {
                let env: Environment = serde_yaml::from_str(&fs::read_to_string(f.path())?)?;
                if f.file_name() == "base.yml" {
                    base_env = env;
                } else {
                    environments.push(env);
                }
            }
        }
        Ok(Workspace {
            collections,
            base_env,
            environments,
        })
    }

    pub fn create_collection(&self, name: &str) -> Result<Collection> {
        let col = Collection {
            id: new_id(),
            name: name.to_string(),
            groups: Vec::new(),
            requests: Vec::new(),
        };
        let dir = self.root.join(format!("collections/{}", col.id));
        fs::create_dir_all(dir.join("requests"))?;
        fs::write(dir.join("collection.yml"), serde_yaml::to_string(&col)?)?;
        Ok(col)
    }

    pub fn create_request(&self, collection_id: &str, name: &str) -> Result<RequestItem> {
        self.create_request_in(collection_id, None, name)
    }

    /// 在集合（或集合内分组）下创建请求。
    pub fn create_request_in(
        &self,
        collection_id: &str,
        group_id: Option<&str>,
        name: &str,
    ) -> Result<RequestItem> {
        let dir = match group_id {
            Some(gid) => self.root.join(format!(
                "collections/{}/groups/{}/requests",
                collection_id, gid
            )),
            None => self
                .root
                .join(format!("collections/{}/requests", collection_id)),
        };
        fs::create_dir_all(&dir)?;
        let req = RequestItem {
            id: new_id(),
            name: name.to_string(),
            method: "GET".into(),
            url: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            body: Body {
                kind: BodyKind::None,
                content: String::new(),
                form_data: Vec::new(),
            },
            description: String::new(),
            docs_open: true,
            auth: None,
        };
        let path = dir.join(format!("{}.yml", req.id));
        fs::write(&path, serde_yaml::to_string(&req)?)?;
        self.index().insert(req.id.clone(), path);
        Ok(req)
    }

    pub fn create_group(
        &self,
        collection_id: &str,
        name: &str,
        parent: Option<String>,
    ) -> Result<Group> {
        let id = new_id();
        let dir = self
            .root
            .join(format!("collections/{}/groups/{}", collection_id, id));
        fs::create_dir_all(dir.join("requests"))?;
        let group = Group {
            id: id.clone(),
            name: name.to_string(),
            parent,
            requests: Vec::new(),
        };
        fs::write(dir.join("group.yml"), serde_yaml::to_string(&group)?)?;
        Ok(group)
    }

    pub fn save_group(&self, collection_id: &str, group: &Group) -> Result<()> {
        let f = self.root.join(format!(
            "collections/{}/groups/{}/group.yml",
            collection_id, group.id
        ));
        fs::write(f, serde_yaml::to_string(group)?)?;
        Ok(())
    }

    pub fn delete_group(&self, collection_id: &str, group_id: &str) -> Result<()> {
        let dir = self
            .root
            .join(format!("collections/{}/groups/{}", collection_id, group_id));
        fs::remove_dir_all(&dir)?;
        self.index_purge_prefix(&dir);
        Ok(())
    }

    pub fn save_request(&self, req: &RequestItem) -> Result<()> {
        // 必须写回请求**所在的那个文件**：分组内的请求若写到集合根目录，
        // 会同时留下「集合根一份 + 分组里一份」，编辑等于复制（历史 bug）。
        let f = self.find_request_file(&req.id)?;
        fs::write(f, serde_yaml::to_string(req)?)?;
        Ok(())
    }

    /// 反查请求文件路径（集合根目录或分组目录），用于原地保存。
    /// 命中索引则零磁盘扫描；索引没有（例如外部新加的文件）才回退扫盘。
    pub fn find_request_file(&self, request_id: &str) -> Result<PathBuf> {
        if let Some(p) = self.index().get(request_id)
            && p.is_file()
        {
            return Ok(p.clone());
        }
        let col_dir = self.find_collection_dir(request_id)?;
        let req_file = format!("{}.yml", request_id);
        let root_copy = col_dir.join("requests").join(&req_file);
        if root_copy.exists() {
            return Ok(root_copy);
        }
        let groups_dir = col_dir.join("groups");
        if groups_dir.is_dir() {
            for g in fs::read_dir(&groups_dir)? {
                let g = g?;
                let f = g.path().join("requests").join(&req_file);
                if f.exists() {
                    return Ok(f);
                }
            }
        }
        Err(anyhow!(
            "request {} 文件不存在（集合 {}）",
            request_id,
            col_dir.display()
        ))
    }

    /// 反查请求所在集合目录（顶层或分组内）。
    fn find_collection_dir(&self, request_id: &str) -> Result<PathBuf> {
        let req_file = format!("{}.yml", request_id);
        for entry in fs::read_dir(self.root.join("collections"))? {
            let entry = entry?;
            let col_dir = entry.path();
            if col_dir.join("requests").join(&req_file).exists() {
                return Ok(col_dir);
            }
            // 分组内
            let groups_dir = col_dir.join("groups");
            if groups_dir.is_dir() {
                for g in fs::read_dir(&groups_dir)? {
                    let g = g?;
                    if g.path().join("requests").join(&req_file).exists() {
                        return Ok(col_dir);
                    }
                }
            }
        }
        Err(anyhow!(
            "request {} not found in any collection",
            request_id
        ))
    }

    /// 把请求移到另一个集合/分组（同集合内换分组也行）。
    ///
    /// 只是把那个 yml 挪个目录：`load()` 是按目录扫出请求列表的，所以
    /// group.yml 里的 requests 不用同步。目标目录不存在就建；已经在目标
    /// 位置则原样返回（幂等）。
    pub fn move_request(
        &self,
        request_id: &str,
        to_collection: &str,
        to_group: Option<&str>,
    ) -> Result<PathBuf> {
        let from = self.find_request_file(request_id)?;
        let dir = match to_group {
            Some(gid) => self.root.join(format!(
                "collections/{}/groups/{}/requests",
                to_collection, gid
            )),
            None => self
                .root
                .join(format!("collections/{}/requests", to_collection)),
        };
        let to = dir.join(format!("{}.yml", request_id));
        if from == to {
            return Ok(to);
        }
        fs::create_dir_all(&dir)?;
        fs::rename(&from, &to)?;
        self.index().insert(request_id.to_string(), to.clone());
        Ok(to)
    }

    pub fn save_collection(&self, col: &Collection) -> Result<()> {
        let f = self
            .root
            .join(format!("collections/{}/collection.yml", col.id));
        fs::write(f, serde_yaml::to_string(col)?)?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> Result<()> {
        let dir = self.root.join(format!("collections/{}", id));
        fs::remove_dir_all(&dir)?;
        self.index_purge_prefix(&dir);
        Ok(())
    }

    /// 删除请求：顶层或分组内都查找。
    pub fn delete_request(&self, collection_id: &str, request_id: &str) -> Result<()> {
        if let Ok(p) = self.find_request_file(request_id)
            && p.starts_with(self.root.join("collections").join(collection_id))
        {
            fs::remove_file(&p)?;
            self.index().remove(request_id);
            return Ok(());
        }
        let req_file = format!("{}.yml", request_id);
        let col_dir = self.root.join(format!("collections/{}", collection_id));
        for base in [col_dir.join("requests"), col_dir.join("groups")] {
            if base.is_dir() {
                if base.join(&req_file).is_file() {
                    fs::remove_file(base.join(&req_file))?;
                    self.index().remove(request_id);
                    return Ok(());
                }
                // 分组内
                if base == col_dir.join("groups") {
                    for g in fs::read_dir(&base)? {
                        let g = g?;
                        let f = g.path().join("requests").join(&req_file);
                        if f.is_file() {
                            fs::remove_file(&f)?;
                            self.index().remove(request_id);
                            return Ok(());
                        }
                    }
                }
            }
        }
        anyhow::bail!(
            "request {} not found in collection {}",
            request_id,
            collection_id
        )
    }

    pub fn new_environment(&self, name: &str, is_base: bool) -> Result<Environment> {
        let env = Environment {
            id: new_id(),
            name: name.to_string(),
            variables: BTreeMap::new(),
        };
        self.save_environment(&env)?;
        if is_base {
            fs::remove_file(self.root.join("environments/base.yml")).ok();
            fs::rename(
                self.root.join(format!("environments/{}.yml", env.id)),
                self.root.join("environments/base.yml"),
            )?;
        }
        Ok(env)
    }

    pub fn save_environment(&self, env: &Environment) -> Result<()> {
        // Base 环境固定存在 base.yml：若 env 的 id 是 base.yml 中的 id，则写回 base.yml
        let base_path = self.root.join("environments/base.yml");
        let is_base = fs::read_to_string(&base_path)
            .ok()
            .and_then(|s| serde_yaml::from_str::<Environment>(&s).ok())
            .map(|e| e.id == env.id)
            .unwrap_or(false);
        let f = if is_base {
            base_path
        } else {
            self.root.join(format!("environments/{}.yml", env.id))
        };
        fs::write(f, serde_yaml::to_string(env)?)?;
        Ok(())
    }
}
