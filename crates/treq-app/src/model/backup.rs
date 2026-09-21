//! 备份/恢复对话框：自动备份、立即备份、按备份恢复（合并或整覆盖）

use super::*;

/// 自动备份线程每轮要用的三样东西：盯哪个工作区、隔多久、留几份。
/// 配置页一改就写这里，线程下一轮（≤30 秒）就按新的来，不用重启。
#[derive(Clone, Default)]
pub struct BackupCfg {
    pub root: PathBuf,
    pub interval_min: u64,
    pub keep: usize,
}

impl AppModel {
    /// 把当前设置同步给自动备份线程。
    pub(crate) fn sync_backup_cfg(&self) {
        let next = BackupCfg {
            root: self.settings.workspace_root.clone(),
            interval_min: settings::backup_interval(&self.settings),
            keep: settings::backup_keep(&self.settings),
        };
        if let Ok(mut r) = self.backup_cfg.lock() {
            *r = next;
        }
    }

    pub fn start_auto_backup(&self) {
        self.sync_backup_cfg();
        let cell = self.backup_cfg.clone();
        std::thread::spawn(move || {
            loop {
                let (ws, interval, keep) = cell
                    .lock()
                    .map(|c| (c.root.clone(), c.interval_min, c.keep))
                    .unwrap_or_default();
                if ws.is_dir() {
                    let dir = treq_core::backup::backup_dir_for(&ws);
                    let last = treq_core::backup::list_backups(&dir)
                        .first()
                        .map(|b| b.created)
                        .unwrap_or(0);
                    let now = treq_core::backup::now_secs();
                    // 到点 + 工作区有改动才备份（没改动就不占一份）
                    if treq_core::backup::due(last, now, interval)
                        && treq_core::backup::newest_change(&ws) > last
                    {
                        match treq_core::backup::create_backup(&ws, &dir, None) {
                            Ok(b) => eprintln!("treq: 已自动备份 {}", b.path.display()),
                            Err(e) => eprintln!("treq: 自动备份失败：{e}"),
                        }
                    }
                    // 每次醒来顺手只留最新 keep 份（改了份数不用等下一次备份）
                    treq_core::backup::prune_backups(&dir, keep).ok();
                }
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
        });
    }

    /// 默认选中最新一份「正常备份」：现场副本（before-restore）只在用户主动挑时才用，
    /// 否则点一下「覆盖」正好把上一次恢复撤销掉。
    pub(crate) fn default_backup_index(items: &[treq_core::backup::BackupInfo]) -> usize {
        items
            .iter()
            .position(|b| !b.name.contains("before-restore"))
            .unwrap_or(0)
    }

    /// 打开「备份与恢复」对话框（列表来自备份目录）。
    pub fn open_restore_dialog(&mut self, cx: &mut Context<Self>) {
        self.popup.close();
        let dir = treq_core::backup::backup_dir_for(&self.settings.workspace_root);
        let items = treq_core::backup::list_backups(&dir);
        let selected = Self::default_backup_index(&items);
        self.restore_dialog = Some(RestoreDialog {
            items,
            selected,
            dir,
            message: None,
            failed: false,
        });
        cx.notify();
    }

    pub fn close_restore_dialog(&mut self, cx: &mut Context<Self>) {
        self.restore_dialog = None;
        cx.notify();
    }

    pub(crate) fn set_backup_message(&mut self, msg: String, failed: bool, cx: &mut Context<Self>) {
        if let Some(d) = self.restore_dialog.as_mut() {
            d.message = Some(msg);
            d.failed = failed;
        }
        cx.notify();
    }

    /// 立即备份当前工作区。
    pub fn backup_now(&mut self, cx: &mut Context<Self>) {
        let root = self.settings.workspace_root.clone();
        let dir = treq_core::backup::backup_dir_for(&root);
        match treq_core::backup::create_backup(&root, &dir, None) {
            Ok(b) => {
                treq_core::backup::prune_backups(&dir, settings::backup_keep(&self.settings)).ok();
                eprintln!("treq: 已备份 {}", b.path.display());
                let msg = format!("{}{}", self.t("backup.done_now"), b.name);
                let items = treq_core::backup::list_backups(&dir);
                let selected = Self::default_backup_index(&items);
                if let Some(d) = self.restore_dialog.as_mut() {
                    d.items = items;
                    d.selected = selected;
                    d.failed = false;
                    d.message = Some(msg);
                } else {
                    self.restore_dialog = Some(RestoreDialog {
                        items,
                        selected: 0,
                        dir,
                        message: Some(msg),
                        failed: false,
                    });
                }
            }
            Err(e) => {
                if self.restore_dialog.is_none() {
                    self.open_restore_dialog(cx);
                }
                let msg = format!("{}{}", self.t("backup.failed"), e);
                self.set_backup_message(msg, true, cx);
            }
        }
        cx.notify();
    }

    /// 按选中项恢复；`overwrite` 为 false 时与当前工作区合并。
    pub fn restore_from_backup(&mut self, overwrite: bool, cx: &mut Context<Self>) {
        let Some(zip) = self
            .restore_dialog
            .as_ref()
            .and_then(|d| d.items.get(d.selected))
            .map(|b| b.path.clone())
        else {
            return;
        };
        let root = self.settings.workspace_root.clone();
        let mode = if overwrite {
            treq_core::backup::RestoreMode::Overwrite
        } else {
            treq_core::backup::RestoreMode::Merge
        };
        let safety_dir = treq_core::backup::backup_dir_for(&root);
        // 合并：先自己留一份现场；覆盖：由 core 自动留（免重复备两份）
        let pre = if overwrite {
            String::new()
        } else {
            treq_core::backup::create_backup(&root, &safety_dir, Some("before-restore"))
                .map(|b| b.name)
                .unwrap_or_default()
        };
        let stats = match treq_core::backup::restore(&zip, &root, mode, &safety_dir) {
            Ok(st) => st,
            Err(e) => {
                let msg = format!("{}{}", self.t("backup.restore_failed"), e);
                self.set_backup_message(msg, true, cx);
                return;
            }
        };
        let safety = stats
            .safety_backup
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(pre);
        let (cols, reqs) = treq_core::backup::workspace_counts(&root);
        let key = if overwrite {
            "backup.done_overwrite"
        } else {
            "backup.done_merge"
        };
        let zip_name = zip
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut msg = format!("{}{}", self.t(key), zip_name);
        msg.push_str(&format!(
            " · {} {} · {} {}",
            cols,
            self.t("backup.collections"),
            reqs,
            self.t("backup.requests")
        ));
        if !safety.is_empty() {
            msg.push_str(&format!(" · {} {}", self.t("backup.safety"), safety));
        }
        // 恢复到当前工作区：重新加载 + 清掉选中/响应
        self.reload();
        self.selection = None;
        self.response = None;
        self.resp_gen = self.resp_gen.wrapping_add(1);
        self.auto_select_first();
        self.load_history(cx);
        let items = treq_core::backup::list_backups(&safety_dir);
        let selected = Self::default_backup_index(&items);
        if let Some(d) = self.restore_dialog.as_mut() {
            d.items = items;
            d.selected = selected;
            d.message = Some(msg);
            d.failed = false;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod default_pick_tests {
    // 注意：别 `use super::*`（model 里 glob 进来的东西有叫 test 的，会把 #[test] 宏搞成递归）
    use crate::model::AppModel;
    use treq_core::backup::BackupInfo;

    fn info(name: &str) -> BackupInfo {
        BackupInfo {
            path: std::path::PathBuf::from(format!("/tmp/{name}")),
            name: name.to_string(),
            bytes: 10,
            created: 0,
        }
    }

    #[test]
    fn skips_before_restore_copies() {
        // 最新那份是恢复前留的现场副本 → 跳过它选下一份，否则「覆盖」会把上次恢复撤掉
        let items = vec![
            info("treq-backup-before-restore-3"),
            info("treq-backup-2"),
            info("treq-backup-1"),
        ];
        assert_eq!(AppModel::default_backup_index(&items), 1);
        // 全是现场副本 → 退回第一份，不能越界
        let only = vec![info("x-before-restore-a"), info("y-before-restore-b")];
        assert_eq!(AppModel::default_backup_index(&only), 0);
        // 空列表
        assert_eq!(AppModel::default_backup_index(&[]), 0);
    }
}
