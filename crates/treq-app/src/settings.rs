use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 已登记的工作区（项目）：名称仅用于展示，路径是唯一标识。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub name: String,
    pub path: PathBuf,
}

impl WorkspaceRef {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        Self { name, path }
    }
}

/// 下拉框外观方案（「外观」设置里可切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DropdownStyle {
    #[serde(rename = "classic")]
    Classic,
    #[serde(rename = "ghost")]
    Ghost,
    #[serde(rename = "outlined")]
    #[default]
    Outlined,
    #[serde(rename = "underline")]
    Underline,
    #[serde(rename = "pill")]
    Pill,
    #[serde(rename = "split")]
    Split,
}

impl DropdownStyle {
    pub const ALL: [DropdownStyle; 6] = [
        DropdownStyle::Classic,
        DropdownStyle::Ghost,
        DropdownStyle::Outlined,
        DropdownStyle::Underline,
        DropdownStyle::Pill,
        DropdownStyle::Split,
    ];
    /// i18n 键：样式名
    pub fn label_key(self) -> &'static str {
        match self {
            DropdownStyle::Classic => "style.classic",
            DropdownStyle::Ghost => "style.ghost",
            DropdownStyle::Outlined => "style.outlined",
            DropdownStyle::Underline => "style.underline",
            DropdownStyle::Pill => "style.pill",
            DropdownStyle::Split => "style.split",
        }
    }
    /// i18n 键：一行说明
    pub fn desc_key(self) -> &'static str {
        match self {
            DropdownStyle::Classic => "style.classic.desc",
            DropdownStyle::Ghost => "style.ghost.desc",
            DropdownStyle::Outlined => "style.outlined.desc",
            DropdownStyle::Underline => "style.underline.desc",
            DropdownStyle::Pill => "style.pill.desc",
            DropdownStyle::Split => "style.split.desc",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub workspace_root: PathBuf,
    pub locale: String, // "zh" | "en"
    pub active_environment_id: Option<String>,
    /// 左树宽度（None = 默认 250）
    #[serde(default)]
    pub tree_width: Option<f32>,
    /// 响应面板宽度（None = 默认 405）
    #[serde(default)]
    pub panel_width: Option<f32>,
    /// 代码生成上次选择的语言/库（CodegenLang::id）
    #[serde(default)]
    pub codegen_lang: Option<String>,
    /// 已登记的工作区（项目）列表；老配置为空时用 workspace_root 播种
    #[serde(default)]
    pub workspaces: Vec<WorkspaceRef>,
    /// 每个工作区上次打开的请求（工作区路径 → 请求 id）：启动时恢复现场
    #[serde(default)]
    pub last_requests: std::collections::HashMap<String, String>,
    /// 自动备份间隔（分钟，None = 默认一天；0 = 关闭自动备份）
    #[serde(default)]
    pub backup_interval_min: Option<u64>,
    /// 备份保留份数（None = 默认 10；只留最新的这么多份）
    #[serde(default)]
    pub backup_keep: Option<usize>,
    /// 多行编辑器（请求体 JSON / Docs markdown）的行高 px（None = 默认 16）
    #[serde(default)]
    pub editor_line_h: Option<f32>,
    /// 事件流正文左侧是否显示每个事件的到达时间（None = 显示）
    #[serde(default)]
    pub sse_show_time: Option<bool>,
    /// 出网代理（如 http://127.0.0.1:7897）；None/空 = 直连
    #[serde(default)]
    pub proxy: Option<String>,
    /// 单请求超时（秒，None = 默认 30）；事件流只用它做连接超时
    #[serde(default)]
    pub timeout_sec: Option<u64>,
    /// 下拉框样式（None/缺省 = outlined）
    #[serde(default)]
    pub dropdown_style: DropdownStyle,
}

/// 单请求超时（秒），None → 默认 30。
pub fn timeout(settings: &Settings) -> u64 {
    settings.timeout_sec.unwrap_or(DEFAULT_TIMEOUT_SEC)
}

/// 默认超时（秒）：内部慢接口常超 30s，可在「网络设置」里改。
pub const DEFAULT_TIMEOUT_SEC: u64 = 30;

/// 默认自动备份间隔：一天一次
pub const DEFAULT_BACKUP_INTERVAL_MIN: u64 = 1440;
/// 默认保留份数
pub const DEFAULT_BACKUP_KEEP: usize = 10;

/// 自动备份间隔（分钟），0 表示关闭。
pub fn backup_interval(settings: &Settings) -> u64 {
    settings
        .backup_interval_min
        .unwrap_or(DEFAULT_BACKUP_INTERVAL_MIN)
}

/// 多行编辑器行高默认值：从 18 收到 16（13px 字下更紧凑）
pub const DEFAULT_EDITOR_LINE_H: f32 = 16.0;

/// 编辑器行高；夹在 12~32，配置里写离谱值也不会把界面搞坏。
pub fn editor_line_h(settings: &Settings) -> f32 {
    settings
        .editor_line_h
        .unwrap_or(DEFAULT_EDITOR_LINE_H)
        .clamp(12.0, 32.0)
}

/// 备份保留份数；至少留 1 份（配置被手改成 0 也不能把备份删光）。
pub fn backup_keep(settings: &Settings) -> usize {
    settings.backup_keep.unwrap_or(DEFAULT_BACKUP_KEEP).max(1)
}

impl Default for Settings {
    fn default() -> Self {
        let root = dirs::document_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("treq");
        Settings {
            workspace_root: root,
            locale: "zh".into(),
            active_environment_id: None,
            tree_width: None,
            panel_width: None,
            codegen_lang: None,
            workspaces: Vec::new(),
            last_requests: std::collections::HashMap::new(),
            backup_interval_min: None,
            backup_keep: None,
            editor_line_h: None,
            sse_show_time: None,
            proxy: None,
            timeout_sec: None,
            dropdown_style: DropdownStyle::Outlined,
        }
    }
}

fn path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("com.treq.app")
        .join("settings.toml")
}

/// cookie 罐落盘位置（跟 settings 同目录，JSON 方便手查）。
pub fn cookie_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("com.treq.app")
        .join("cookies.json")
}

pub fn load() -> Settings {
    let mut s: Settings = std::fs::read_to_string(path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    s.ensure_workspace_registered();
    s
}

impl Settings {
    /// 把当前 workspace_root 收进列表，并去重（老配置兼容：列表为空时自动播种）。
    pub fn ensure_workspace_registered(&mut self) {
        let cur = self.workspace_root.clone();
        if !self.workspaces.iter().any(|w| w.path == cur) {
            self.workspaces.push(WorkspaceRef::from_path(cur));
        } else if let Some(w) = self.workspaces.iter_mut().find(|w| w.path == cur) {
            w.name = WorkspaceRef::from_path(&cur).name;
        }
    }

    /// 当前工作区上次打开的请求 id。
    pub fn last_request(&self) -> Option<&String> {
        self.last_requests
            .get(&self.workspace_root.to_string_lossy().to_string())
    }

    /// 记下当前工作区打开的请求（启动时用来恢复现场）。
    pub fn set_last_request(&mut self, id: &str) {
        let key = self.workspace_root.to_string_lossy().to_string();
        if self.last_requests.get(&key).map(|s| s.as_str()) != Some(id) {
            self.last_requests.insert(key, id.to_string());
        }
    }

    /// 切换当前工作区（同时保证它在列表里）。
    pub fn set_active_workspace(&mut self, path: PathBuf) {
        self.workspace_root = path;
        self.ensure_workspace_registered();
    }

    /// 从列表里移除（不动磁盘文件）；返回是否移除了当前工作区。
    pub fn forget_workspace(&mut self, path: &std::path::Path) -> bool {
        self.workspaces.retain(|w| w.path != path);
        if self.workspaces.is_empty() {
            self.ensure_workspace_registered();
            return false;
        }
        if self.workspace_root == path {
            self.workspace_root = self.workspaces[0].path.clone();
            return true;
        }
        false
    }
}

pub fn save(s: &Settings) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, toml::to_string(s)?)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_get_workspace_seeded() {
        let mut s = Settings {
            workspace_root: PathBuf::from("/tmp/proj-a"),
            workspaces: Vec::new(),
            ..Default::default()
        };
        s.ensure_workspace_registered();
        assert_eq!(s.workspaces.len(), 1);
        assert_eq!(s.workspaces[0].path, PathBuf::from("/tmp/proj-a"));
        assert_eq!(s.workspaces[0].name, "proj-a");
        // 重复调用不重复登记
        s.ensure_workspace_registered();
        assert_eq!(s.workspaces.len(), 1);
    }

    #[test]
    fn workspace_switch_and_forget() {
        let mut s = Settings {
            workspaces: vec![
                WorkspaceRef::from_path("/tmp/a"),
                WorkspaceRef::from_path("/tmp/b"),
            ],
            ..Default::default()
        };
        s.set_active_workspace(PathBuf::from("/tmp/b"));
        assert_eq!(s.workspace_root, PathBuf::from("/tmp/b"));
        assert_eq!(s.workspaces.len(), 2, "切换已在列表里的工作区不再追加");

        // 移除当前：回落到列表第一个
        assert!(s.forget_workspace(std::path::Path::new("/tmp/b")));
        assert_eq!(s.workspace_root, PathBuf::from("/tmp/a"));
        assert_eq!(s.workspaces.len(), 1);

        // 移除最后一个：自动重新播种当前，列表不为空
        assert!(!s.forget_workspace(std::path::Path::new("/tmp/a")));
        assert_eq!(s.workspaces.len(), 1);
    }

    #[test]
    fn workspaces_round_trip_toml() {
        let s = Settings {
            workspaces: vec![
                WorkspaceRef::from_path("/tmp/a"),
                WorkspaceRef::from_path("/tmp/b"),
            ],
            ..Default::default()
        };
        let toml = toml::to_string(&s).unwrap();
        let back: Settings = toml::from_str(&toml).unwrap();
        assert_eq!(back.workspaces.len(), 2);
        assert_eq!(back.workspaces[1].name, "b");
    }

    #[test]
    fn editor_line_height_defaults_and_clamps() {
        let mut s = Settings::default();
        assert_eq!(editor_line_h(&s), 16.0);
        s.editor_line_h = Some(20.5);
        assert_eq!(editor_line_h(&s), 20.5);
        s.editor_line_h = Some(2.0);
        assert_eq!(editor_line_h(&s), 12.0, "手改太小兜到底");
        s.editor_line_h = Some(999.0);
        assert_eq!(editor_line_h(&s), 32.0, "手改太大兜到顶");
    }

    #[test]
    fn backup_defaults_are_one_day_and_ten_copies() {
        let mut s = Settings::default();
        assert_eq!(backup_interval(&s), 1440, "默认一天一次");
        assert_eq!(backup_keep(&s), 10);
        s.backup_interval_min = Some(0);
        assert_eq!(backup_interval(&s), 0, "0 = 关掉");
        s.backup_keep = Some(0);
        assert_eq!(backup_keep(&s), 1, "手改成 0 也只当 1 份，不能删光");
        s.backup_keep = Some(3);
        assert_eq!(backup_keep(&s), 3);
        // 真的存得下来
        s.backup_keep = Some(7);
        let back: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(backup_keep(&back), 7);
    }

    #[test]
    fn newer_fields_round_trip_toml() {
        let s = Settings {
            last_requests: std::collections::HashMap::from([(
                "/tmp/proj".to_string(),
                "req-1".to_string(),
            )]),
            dropdown_style: DropdownStyle::Pill,
            active_environment_id: Some("env-2".into()),
            backup_interval_min: Some(30),
            sse_show_time: Some(false),
            proxy: Some("http://127.0.0.1:7897".into()),
            timeout_sec: Some(45),
            tree_width: Some(267.5),
            panel_width: Some(355.25),
            codegen_lang: Some("rust-reqwest".into()),
            ..Settings::default()
        };
        let text = toml::to_string(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.dropdown_style, DropdownStyle::Pill);
        assert_eq!(back.sse_show_time, Some(false), "事件流时间列开关要能存下来");
        assert_eq!(
            back.last_requests.get("/tmp/proj").map(String::as_str),
            Some("req-1")
        );
        assert_eq!(back.active_environment_id.as_deref(), Some("env-2"));
        assert_eq!(back.backup_interval_min, Some(30));
        assert_eq!(back.proxy.as_deref(), Some("http://127.0.0.1:7897"));
        assert_eq!(back.timeout_sec, Some(45));
        assert_eq!(back.tree_width, Some(267.5));
        assert_eq!(back.panel_width, Some(355.25));
        assert_eq!(back.codegen_lang.as_deref(), Some("rust-reqwest"));
    }

    #[test]
    fn old_settings_load_ok() {
        let s = "workspace_root = \"/tmp/x\"\nlocale = \"zh\"\n";
        let parsed: Settings = toml::from_str(s).unwrap();
        assert_eq!(parsed.workspace_root, std::path::PathBuf::from("/tmp/x"));
        assert_eq!(parsed.tree_width, None);
        assert_eq!(parsed.panel_width, None);
        assert_eq!(parsed.codegen_lang, None);
    }
}
