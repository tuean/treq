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

/// 配色方案（「外观」设置里可切换）。
///
/// - `Harbor`：**暮光蓝港 · 日落橙辉 · 半透明夜色面板**（默认）。色阶底子是 dsh 的
///   alias 层，主面不透明（#141829），浮层保留 alpha 走毛玻璃，强调色是日落橙。
/// - `InsomniaDark`：**Insomnia 暗色** —— treq 最早的取色就是照它逐点取的
///   （面板 #2c2c2c / 输入条 #212121 / 品牌紫 #8776d5 / JSON 键绿串黄）。
/// - `InsomniaLight`：Insomnia 亮色（同一个品牌紫，浅底适配过的语法色）。
/// - `DshLight`：dsh 浅色（色值取自 dsh 设计 token 的 alias 层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeScheme {
    #[serde(rename = "harbor")]
    #[default]
    Harbor,
    #[serde(rename = "insomnia-dark")]
    InsomniaDark,
    #[serde(rename = "insomnia-light")]
    InsomniaLight,
    #[serde(rename = "dsh-light")]
    DshLight,
}

impl ThemeScheme {
    pub const ALL: [ThemeScheme; 4] = [
        ThemeScheme::Harbor,
        ThemeScheme::InsomniaDark,
        ThemeScheme::InsomniaLight,
        ThemeScheme::DshLight,
    ];

    /// i18n 键：方案名
    pub fn label_key(self) -> &'static str {
        match self {
            ThemeScheme::Harbor => "theme.harbor",
            ThemeScheme::InsomniaDark => "theme.insomnia_dark",
            ThemeScheme::InsomniaLight => "theme.insomnia_light",
            ThemeScheme::DshLight => "theme.dsh_light",
        }
    }

    /// i18n 键：一行说明
    pub fn desc_key(self) -> &'static str {
        match self {
            ThemeScheme::Harbor => "theme.harbor.desc",
            ThemeScheme::InsomniaDark => "theme.insomnia_dark.desc",
            ThemeScheme::InsomniaLight => "theme.insomnia_light.desc",
            ThemeScheme::DshLight => "theme.dsh_light.desc",
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
    /// 补全候选浮层缩放（None = 1.0；0.8~2.0）
    #[serde(default)]
    pub suggest_scale: Option<f32>,
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
    /// 配色方案（None/缺省 = harbor 暮光蓝港）
    #[serde(default)]
    pub theme_scheme: ThemeScheme,
    /// 界面字体家族（空 = 系统默认字体）
    #[serde(default)]
    pub font_ui: Option<String>,
    /// 界面字号（None = 默认 13）
    #[serde(default)]
    pub font_ui_size: Option<f32>,
    /// 代码/JSON 字体家族（None = 默认 Menlo）
    #[serde(default)]
    pub font_mono: Option<String>,
    /// 代码/JSON 字号（None = 默认 12）
    #[serde(default)]
    pub font_mono_size: Option<f32>,
}

/// 界面字号；夹在 10~20，字太大控件会挤爆固定行高。
pub fn font_ui_size(settings: &Settings) -> f32 {
    settings
        .font_ui_size
        .unwrap_or(crate::theme::DEFAULT_UI_SIZE)
        .clamp(10.0, 20.0)
}

/// 代码/JSON 字号；夹在 9~24。
pub fn font_mono_size(settings: &Settings) -> f32 {
    settings
        .font_mono_size
        .unwrap_or(crate::theme::DEFAULT_MONO_SIZE)
        .clamp(9.0, 24.0)
}

/// 界面字体家族（空格/空串 = 系统默认）。
pub fn font_ui(settings: &Settings) -> String {
    settings.font_ui.clone().unwrap_or_default().trim().to_string()
}

/// 代码字体家族（空 = Menlo）。
pub fn font_mono(settings: &Settings) -> String {
    match settings.font_mono.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => crate::theme::DEFAULT_MONO_FAMILY.to_string(),
    }
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
/// 补全候选浮层缩放默认值
pub const DEFAULT_SUGGEST_SCALE: f32 = 1.0;

/// 编辑器行高；夹在 12~32，配置里写离谱值也不会把界面搞坏。
pub fn editor_line_h(settings: &Settings) -> f32 {
    settings
        .editor_line_h
        .unwrap_or(DEFAULT_EDITOR_LINE_H)
        .clamp(12.0, 32.0)
}

/// 补全候选浮层缩放；夹在 0.8~2.0（太小看不清，太大一屏放不下）。
pub fn suggest_scale(settings: &Settings) -> f32 {
    settings
        .suggest_scale
        .unwrap_or(DEFAULT_SUGGEST_SCALE)
        .clamp(0.8, 2.0)
}

/// 备份保留份数；至少留 1 份（配置被手改成 0 也不能把备份删光）。
pub fn backup_keep(settings: &Settings) -> usize {
    settings.backup_keep.unwrap_or(DEFAULT_BACKUP_KEEP).max(1)
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            workspace_root: default_workspace(),
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
            suggest_scale: None,
            font_ui: None,
            font_ui_size: None,
            font_mono: None,
            font_mono_size: None,
            sse_show_time: None,
            proxy: None,
            timeout_sec: None,
            dropdown_style: DropdownStyle::Outlined,
            theme_scheme: ThemeScheme::Harbor,
        }
    }
}

/// app 的数据目录（`~/Library/Application Support/com.treq.app`）。
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("com.treq.app")
}

/// 默认工作区：放在 app 自己的数据目录里。
///
/// **故意不放 `~/Documents`**：那是 macOS 受保护目录，首次访问会拦在 `open()` 上等
/// 用户点「允许」，而这一下发生在窗口画出来之前 —— 表现就是「一打开就失去响应」。
/// 放这里既不触发隐私授权，也跟 settings/cookies/history 待在一起。
pub fn default_workspace() -> PathBuf {
    data_dir().join("workspace")
}

/// 所有「空间」的父目录：`~/Library/Application Support/com.treq.app/spaces/`。
///
/// 每个空间是它下面一个普通目录（`<slug>/`，里面就是 `collections/`、`environments/`），
/// 与 settings / cookies / history 同处 app 数据目录 —— 不碰 macOS 受保护目录。
pub fn spaces_root() -> PathBuf {
    data_dir().join("spaces")
}

/// 由空间名算出目录名（单层、安全、可读）。
///
/// 规则：去首尾空白 → 空白与路径分隔符转 `-` → 丢掉 `:`/`*`/`?` 等文件系统敏感字符 →
/// 全是符号时回落到 `space`。**不含路径分隔符**，所以拼出来的目录一定在 spaces_root 直下。
pub fn space_dir_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        let c = if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            ch
        } else if ch.is_whitespace() || ch == '/' || ch == '\\' {
            '-'
        } else {
            continue; // 其余标点直接丢
        };
        if c == '-' {
            if last_dash || out.is_empty() {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        out.push(c);
    }
    let out = out.trim_matches('-').to_string();
    // 纯点号（`.` / `..` / `...`）要么是目录语义里的特殊名字（上级目录），
    // 要么会变成隐藏目录，统一兜底成 `space`
    if out.is_empty() || out.chars().all(|c| c == '.') {
        "space".into()
    } else {
        out
    }
}

/// 空间落点：`spaces/<slug>`；重名时依次试 `<slug>-2`、`<slug>-3`…（最多 1000 次）。
pub fn space_path_for(name: &str) -> PathBuf {
    let base = space_dir_name(name);
    let root = spaces_root();
    let first = root.join(&base);
    if !first.exists() {
        return first;
    }
    for i in 2..1000 {
        let p = root.join(format!("{base}-{i}"));
        if !p.exists() {
            return p;
        }
    }
    // 极端情况：全被占了也要给个能用的（带时间戳）
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    root.join(format!("{base}-{stamp}"))
}

fn path() -> PathBuf {
    data_dir().join("settings.toml")
}

/// cookie 罐落盘位置（跟 settings 同目录，JSON 方便手查）。
pub fn cookie_path() -> PathBuf {
    data_dir()
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

    /// 登记一个空间（已存在则只更新名字），不切换当前空间。
    pub fn register_space(&mut self, path: PathBuf, name: &str) {
        match self.workspaces.iter_mut().find(|w| w.path == path) {
            Some(w) => w.name = name.to_string(),
            None => self.workspaces.push(WorkspaceRef {
                name: name.to_string(),
                path,
            }),
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
    fn font_defaults_and_clamps() {
        let mut s = Settings::default();
        assert_eq!(font_ui_size(&s), 13.0);
        assert_eq!(font_mono_size(&s), 12.0);
        assert_eq!(font_ui(&s), "");
        assert_eq!(font_mono(&s), "Menlo");
        s.font_ui = Some("  ".into());
        assert_eq!(font_ui(&s), "", "空白当系统默认");
        s.font_mono = Some("".into());
        assert_eq!(font_mono(&s), "Menlo", "空串回落默认等宽字体");
        s.font_mono = Some("JetBrains Mono".into());
        assert_eq!(font_mono(&s), "JetBrains Mono");
        // 手改离谱值也不会把界面撑坏
        s.font_ui_size = Some(40.0);
        assert_eq!(font_ui_size(&s), 20.0);
        s.font_mono_size = Some(2.0);
        assert_eq!(font_mono_size(&s), 9.0);
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

#[cfg(test)]
mod space_tests {
    use super::{space_dir_name, space_path_for};

    #[test]
    fn dir_name_is_single_level_and_readable() {
        assert_eq!(space_dir_name("treq"), "treq");
        assert_eq!(space_dir_name("  业务产品 A  "), "业务产品-A");
        assert_eq!(space_dir_name("a/b/c"), "a-b-c");          // 不能带路径分隔符
        assert_eq!(space_dir_name("a:b*c?d"), "abcd");          // 敏感字符丢掉
        assert_eq!(space_dir_name("--x--"), "x");
        assert_eq!(space_dir_name("///"), "space");             // 全是符号 → 兜底
        assert_eq!(space_dir_name(""), "space");
        // 纯点号是上级目录语义，必须兜底（单测抓到过这个 bug）
        assert_eq!(space_dir_name("."), "space");
        assert_eq!(space_dir_name(".."), "space");
        assert_eq!(space_dir_name("..."), "space");
    }

    #[test]
    fn dir_name_never_escapes_the_spaces_root() {
        for name in ["../evil", "..", "/etc/passwd", "a/../..", r"\\server\share"] {
            let d = space_dir_name(name);
            assert!(!d.contains('/'), "{name} → {d} 不应含 /");
            assert!(!d.contains('\\'), "{name} → {d} 不应含反斜杠");
            // 真正要防的是「正好是 . 或 ..」以及多层路径；`..-evil` 这种单层名字是安全的
            assert!(d != "." && d != "..", "{name} → {d} 不能是 . 或 ..");
            assert_eq!(std::path::Path::new(&d).components().count(), 1, "{name} → {d} 必须只有一层");
            let p = space_path_for(name);
            assert_eq!(p.parent(), Some(super::spaces_root().as_path()));
        }
    }
}
