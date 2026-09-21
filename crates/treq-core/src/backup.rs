//! 工作区备份 / 恢复（zip）。
//!
//! 备份用系统自带的 `/usr/bin/zip`（macOS 一定有，产物是标准 zip，Finder 可直接打开），
//! 恢复用 `/usr/bin/unzip`：合并 = 覆盖同名文件（备份里有的补回来，现有的不动），
//! 覆盖 = 先给当前工作区自动打一份备份，再清空写入。

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const ZIP_BIN: &str = "/usr/bin/zip";
const UNZIP_BIN: &str = "/usr/bin/unzip";
/// 备份里要收进去的工作区成员（都是相对路径，保证 unzip 出来结构一致）
const MEMBERS: &[&str] = &["collections", "environments"];

#[derive(Debug, Clone, PartialEq)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    /// Unix 秒
    pub created: i64,
}

impl BackupInfo {
    pub fn size_label(&self) -> String {
        let kb = self.bytes as f64 / 1024.;
        if kb < 1024. {
            format!("{:.0} KB", kb)
        } else {
            format!("{:.1} MB", kb / 1024.)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    /// 合并：备份里有的补/覆盖回工作区，工作区多出来的保留
    Merge,
    /// 覆盖：先自动备份当前工作区，再清空后写入
    Overwrite,
}

#[derive(Debug, Default, PartialEq)]
pub struct RestoreStats {
    pub files: usize,
    pub safety_backup: Option<PathBuf>,
}

/// 默认备份目录：工作区的兄弟目录 `<名字>-backups`
pub fn backup_dir_for(root: &Path) -> PathBuf {
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "treq".into());
    root.parent()
        .unwrap_or(root)
        .join(format!("{}-backups", name))
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn mtime_secs(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// 备份文件名里的时间戳用系统 date（本地时区、可排序：20260920-170238）
/// 文件名时间戳：`20260920-183012`（系统 /bin/date，不引依赖）
pub fn stamp() -> String {
    Command::new("/bin/date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| now_secs().to_string())
}

/// 解压前校验条目名：只允许工作区内的相对路径（挡 zip-slip）
pub fn is_safe_entry(name: &str) -> bool {
    let t = name.trim();
    !t.is_empty()
        && !t.starts_with('/')
        && !t.starts_with('\\')
        && !t.contains(':')
        && t.split(['/', '\\']).all(|seg| seg != "..")
}

/// zip 里的条目名（目录条目以 / 结尾）
pub fn entries_of(zip: &Path) -> Result<Vec<String>> {
    let out = Command::new(UNZIP_BIN)
        .arg("-Z1")
        .arg(zip)
        .output()
        .with_context(|| format!("无法调用 {}", UNZIP_BIN))?;
    if !out.status.success() {
        bail!(
            "读取备份失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    for n in &names {
        if !is_safe_entry(n) {
            bail!("备份里有非法路径，已中止：{}", n);
        }
    }
    Ok(names)
}

/// 工作区里最新的文件时间（用来判断有没有改动）
pub fn newest_change(root: &Path) -> i64 {
    fn walk(dir: &Path, newest: &mut i64) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, newest);
            } else {
                *newest = (*newest).max(mtime_secs(&p));
            }
        }
    }
    let mut newest = 0;
    for m in MEMBERS {
        let dir = root.join(m);
        if dir.is_dir() {
            walk(&dir, &mut newest);
        }
    }
    newest
}

/// 打一份 zip 备份（先写 .tmp 再改名，避免半截文件）
pub fn create_backup(root: &Path, dir: &Path, tag: Option<&str>) -> Result<BackupInfo> {
    if !root.is_dir() {
        bail!("工作区不存在：{}", root.display());
    }
    std::fs::create_dir_all(dir)?;
    let stamp = stamp();
    let name = match tag {
        Some(t) => format!("treq-{}-{}.zip", stamp, t),
        None => format!("treq-{}.zip", stamp),
    };
    let out = dir.join(&name);
    let tmp = dir.join(format!("{}.tmp", name));
    let _ = std::fs::remove_file(&tmp);

    let mut cmd = Command::new(ZIP_BIN);
    cmd.arg("-r").arg("-q").arg("-X").arg(&tmp);
    for m in MEMBERS {
        if root.join(m).exists() {
            cmd.arg(m);
        }
    }
    cmd.current_dir(root);
    let res = cmd
        .output()
        .with_context(|| format!("无法调用 {}", ZIP_BIN))?;
    if !res.status.success() || !tmp.is_file() {
        let _ = std::fs::remove_file(&tmp);
        bail!("备份失败：{}", String::from_utf8_lossy(&res.stderr).trim());
    }
    std::fs::rename(&tmp, &out)?;
    Ok(info_of(&out))
}

fn info_of(path: &Path) -> BackupInfo {
    BackupInfo {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        created: mtime_secs(path),
        path: path.to_path_buf(),
    }
}

/// 备份列表：新的在前
pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<BackupInfo> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("zip"))
                .unwrap_or(false)
        })
        .map(|p| info_of(&p))
        .collect();
    out.sort_by(|a, b| b.created.cmp(&a.created).then(b.name.cmp(&a.name)));
    out
}

/// 到点该自动备份了吗：`interval_min = 0` 表示关掉；`last` 是上一份备份的时间（从没有过就 0）。
/// 时钟被往回调（now < last）时不算到点，否则会一口气备份一轮。
pub fn due(last: i64, now: i64, interval_min: u64) -> bool {
    interval_min > 0 && now >= last && now - last >= interval_min as i64 * 60
}

/// 只留最近 keep 份，返回删掉的数量
pub fn prune_backups(dir: &Path, keep: usize) -> Result<usize> {
    let list = list_backups(dir);
    let mut n = 0;
    for b in list.into_iter().skip(keep) {
        std::fs::remove_file(&b.path)?;
        n += 1;
    }
    Ok(n)
}

/// 恢复：Merge 就地覆盖，Overwrite 先自动备份再把工作区清空重铺
pub fn restore(
    zip: &Path,
    root: &Path,
    mode: RestoreMode,
    safety_dir: &Path,
) -> Result<RestoreStats> {
    if !zip.is_file() {
        bail!("备份文件不存在：{}", zip.display());
    }
    let names = entries_of(zip)?;
    if names.is_empty() {
        bail!("备份是空的：{}", zip.display());
    }
    std::fs::create_dir_all(root)?;

    let mut stats = RestoreStats::default();
    if mode == RestoreMode::Overwrite {
        // 恢复前先留一份现场，点错了还能回来
        stats.safety_backup = create_backup(root, safety_dir, Some("before-restore"))
            .ok()
            .map(|b| b.path);
        for m in MEMBERS {
            let p = root.join(m);
            if p.exists() {
                std::fs::remove_dir_all(&p)
                    .with_context(|| format!("清空失败：{}", p.display()))?;
            }
        }
    }

    let res = Command::new(UNZIP_BIN)
        .arg("-o")
        .arg("-q")
        .arg(zip)
        .arg("-d")
        .arg(root)
        .output()
        .with_context(|| format!("无法调用 {}", UNZIP_BIN))?;
    if !res.status.success() {
        bail!("解压失败：{}", String::from_utf8_lossy(&res.stderr).trim());
    }
    stats.files = names.iter().filter(|n| !n.ends_with('/')).count();
    Ok(stats)
}

/// 统计工作区规模（用于恢复后的提示）
pub fn workspace_counts(root: &Path) -> (usize, usize) {
    let (mut cols, mut reqs) = (0, 0);
    let dir = root.join("collections");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return (0, 0);
    };
    for e in rd.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        cols += 1;
        fn count_requests(dir: &Path, n: &mut usize) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    count_requests(&p, n);
                } else if p.extension().map(|x| x == "yml").unwrap_or(false) {
                    *n += 1;
                }
            }
        }
        count_requests(&e.path().join("requests"), &mut reqs);
        count_requests(&e.path().join("groups"), &mut reqs);
    }
    (cols, reqs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("treq-backup-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_workspace(root: &Path, col: &str, req: &str) {
        let a = root.join(format!("collections/{}/requests", col));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(
            root.join(format!("collections/{}/collection.yml", col)),
            format!("id: {}\nname: 集合{}\n", col, col),
        )
        .unwrap();
        std::fs::write(
            a.join(format!("{}.yml", req)),
            format!("id: {}\nname: 接口{}\nurl: http://h/{}\n", req, req, req),
        )
        .unwrap();
    }

    #[test]
    fn safe_entry_rejects_escapes() {
        assert!(is_safe_entry("collections/a/collection.yml"));
        assert!(!is_safe_entry(""));
        assert!(!is_safe_entry("/etc/passwd"));
        assert!(!is_safe_entry("../../evil"));
        assert!(!is_safe_entry("a/../../evil"));
        assert!(!is_safe_entry("C:/x"));
        assert!(
            is_safe_entry("collections/a b/中文.yml"),
            "空格和中文要允许"
        );
    }

    #[test]
    fn due_only_fires_when_the_interval_has_passed() {
        let day: u64 = 24 * 60; // 一天多少分钟
        let span = day as i64 * 60;
        assert!(!due(0, 0, 0), "间隔 0 = 关掉自动备份");
        assert!(!due(1_000, 1_000, day), "刚备份过");
        assert!(!due(1_000, 1_000 + span - 1, day), "差一秒也不算到点");
        assert!(due(1_000, 1_000 + span, day), "正好到点");
        assert!(due(0, 1_700_000_000, day), "从没备份过 → 立刻备一份");
        assert!(!due(5_000, 4_000, day), "时钟往回跳时不备份");
    }

    #[test]
    fn prune_keeps_only_the_newest_copies() {
        let ws = tmp("prune-ws");
        let dir = tmp("prune-dir");
        fake_workspace(&ws, "c1", "r1");
        // 名字里带 tag，同一秒建的多份也能区分新旧（列表按时间 + 名字倒序）
        for tag in ["1", "2", "3", "4"] {
            create_backup(&ws, &dir, Some(tag)).unwrap();
        }
        assert_eq!(list_backups(&dir).len(), 4);

        assert_eq!(prune_backups(&dir, 3).unwrap(), 1, "删掉最旧的一份");
        let names: Vec<String> = list_backups(&dir).into_iter().map(|b| b.name).collect();
        assert_eq!(names.len(), 3);
        assert!(names.iter().all(|n| !n.contains("-1.zip")), "留下的：{names:?}");

        // keep 比实际多 → 一个都不删
        assert_eq!(prune_backups(&dir, 99).unwrap(), 0);
        assert_eq!(list_backups(&dir).len(), 3);
        // 只留 1 份
        assert_eq!(prune_backups(&dir, 1).unwrap(), 2);
        assert!(list_backups(&dir)[0].name.contains("-4.zip"), "留下最新的");
    }

    #[test]
    fn backup_merge_and_overwrite_round_trip() {
        let dir = tmp("round");
        let root = dir.join("ws");
        std::fs::create_dir_all(&root).unwrap();
        fake_workspace(&root, "c1", "r1");
        let store_dir = dir.join("backups");

        let b = create_backup(&root, &store_dir, None).unwrap();
        assert!(b.path.is_file() && b.bytes > 0, "备份应产出非空 zip");
        assert!(list_backups(&store_dir).len() == 1);

        // 合并：删掉接口后恢复，接口回来；工作区里新加的东西保留
        std::fs::remove_file(root.join("collections/c1/requests/r1.yml")).unwrap();
        fake_workspace(&root, "c2", "r2");
        let st = restore(&b.path, &root, RestoreMode::Merge, &store_dir).unwrap();
        assert!(
            st.files >= 2 && st.safety_backup.is_none(),
            "合并不该动现场备份"
        );
        assert!(
            root.join("collections/c1/requests/r1.yml").is_file(),
            "合并要把删掉的补回来"
        );
        assert!(
            root.join("collections/c2/requests/r2.yml").is_file(),
            "合并不能删工作区里多的"
        );

        // 覆盖：c2 消失，且自动留了现场备份
        let st = restore(&b.path, &root, RestoreMode::Overwrite, &store_dir).unwrap();
        assert!(st.safety_backup.is_some(), "覆盖前应自动备份现场");
        assert!(root.join("collections/c1/requests/r1.yml").is_file());
        assert!(
            !root.join("collections/c2").exists(),
            "覆盖后工作区与备份一致"
        );
        assert_eq!(workspace_counts(&root), (1, 1));
        let list = list_backups(&store_dir);
        assert_eq!(list.len(), 2, "首次备份 + 覆盖前的现场备份");
        assert!(
            list.iter().any(|b| b.name.contains("before-restore")),
            "现场备份要能一眼认出来：{:?}",
            list.iter().map(|b| b.name.clone()).collect::<Vec<_>>()
        );

        // 保留策略
        assert_eq!(prune_backups(&store_dir, 1).unwrap(), 1);
        assert_eq!(list_backups(&store_dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_changes_and_excludes_outside_members() {
        let dir = tmp("change");
        let root = dir.join("ws");
        std::fs::create_dir_all(&root).unwrap();
        fake_workspace(&root, "c1", "r1");
        let t = newest_change(&root);
        assert!(t > 0, "应能读到文件时间");
        // 备份目录是工作区的兄弟目录，不能被算进“改动”
        let store_dir = backup_dir_for(&root);
        assert_eq!(store_dir, dir.join("ws-backups"));
        create_backup(&root, &store_dir, None).unwrap();
        assert_eq!(newest_change(&root), t, "备份不该影响工作区时间戳");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
