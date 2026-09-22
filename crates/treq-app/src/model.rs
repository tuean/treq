use crate::i18n::{Locale, tr};
use crate::nav::{NavEntry, NavStack};
use crate::panes::{fmt_duration_ms, truncate_url};
use crate::settings::{self, DropdownStyle, Settings, WorkspaceRef};
use crate::theme;
use crate::widgets::TextField;
use gpui::{prelude::*, *};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use treq_core::cookies::CookieJar;
use treq_core::vars;
use treq_core::{
    BodyKind, CodegenLang, Environment, FormField, Group, HistoryEntry, HistoryStore, Kv,
    RequestItem, ResponseData, TrashStore, Workspace, WorkspaceStore,
};
// 按主题拆出去的 impl 块（都是 `impl AppModel`，只是分文件放，见各自文件头注释）
mod backup;
mod codegen;
pub mod tree_drag;
mod completion;
mod cookies_net;
mod move_dialog;
pub use move_dialog::MoveDialog;
mod env_vars;
mod history;
mod import_ui;
mod kv_zoom;
pub use kv_zoom::{KvZoom, kv_zoom_lines};
mod quick;
mod request_edit;
mod response_ui;
mod send;
mod vars_save;
/// 启动时读 cookie 罐：文件坏了就改名留证、从空罐开始（不能因为坏文件起不来）。
fn load_cookie_jar() -> CookieJar {
    let p = settings::cookie_path();
    let Ok(txt) = std::fs::read_to_string(&p) else {
        return CookieJar::default();
    };
    match serde_json::from_str::<CookieJar>(&txt) {
        Ok(mut jar) => {
            jar.prune(treq_core::cookies::now_unix());
            jar
        }
        Err(e) => {
            eprintln!("treq: cookie 罐解析失败（{}）：{}", p.display(), e);
            let _ = std::fs::rename(&p, p.with_extension("json.bad"));
            CookieJar::default()
        }
    }
}

/// 拖动中的分隔条目标。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragTarget {
    Tree,
    Panel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Collection(String),
    Group(String),
    Request(String),
}

/// 弹出菜单状态（树右键 / 环境下拉 / 方法下拉 / 类型下拉共用）。
pub struct PopupMenu {
    pub open: bool,
    pub x: f32,
    pub y: f32,
    /// 打开它的控件 id：再次点同一个控件即关闭（toggle）
    pub source: Option<String>,
    /// (action_id, label)
    pub items: Vec<(String, String)>,
}

impl Default for PopupMenu {
    fn default() -> Self {
        Self {
            open: false,
            x: 0.0,
            y: 0.0,
            source: None,
            items: vec![],
        }
    }
}

impl PopupMenu {
    pub fn close(&mut self) {
        self.open = false;
        self.source = None;
    }

    /// 菜单估算尺寸：给弹出位置做窗口内夹取用。
    pub fn size_hint(&self) -> (f32, f32) {
        let widest = self
            .items
            .iter()
            .map(|(_, label)| {
                label
                    .chars()
                    .map(|c| if c.is_ascii() { 7.5 } else { 13.5 })
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max);
        let w = (widest + 40.0).clamp(180.0, 420.0);
        let h = self.items.len() as f32 * theme::control_h().to_f64() as f32 + 10.0;
        (w, h)
    }
}

/// 响应正文的鼠标选中（格式化 / 原始视图都能选）。
#[derive(Debug, Clone, Copy)]
pub struct RespSel {
    /// 按下处（可见行, 字节列）
    pub a_row: usize,
    pub a_col: usize,
    /// 拖动处（可见行, 字节列）
    pub c_row: usize,
    pub c_col: usize,
}

impl RespSel {
    /// 排序后的区间（起点准在前）
    pub fn range(&self) -> ((usize, usize), (usize, usize)) {
        if (self.c_row, self.c_col) < (self.a_row, self.a_col) {
            ((self.c_row, self.c_col), (self.a_row, self.a_col))
        } else {
            ((self.a_row, self.a_col), (self.c_row, self.c_col))
        }
    }

    /// 这一行里要选中的字节区间（没选中返回 None）。
    pub fn row_range(&self, row: usize, row_len: usize) -> Option<(usize, usize)> {
        let ((sr, sc), (er, ec)) = self.range();
        if row < sr || row > er {
            return None;
        }
        let from = if row == sr { sc.min(row_len) } else { 0 };
        let to = if row == er { ec.min(row_len) } else { row_len };
        (from < to).then_some((from, to))
    }
}

/// 行内重命名状态。
pub struct RenameState {
    pub target: Selection,
    pub field: Entity<TextField>,
    /// 刚进入改名：下一帧把焦点放进输入框（新建后直接能打字，不用再点一下）
    pub focus: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    Params,
    Headers,
    Body,
    Auth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvWhich {
    Params,
    Headers,
    /// multipart/form-data 字段表
    FormData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseTab {
    Body,
    Headers,
    Cookies,
    Timeline,
    History,
}

/// cURL 导入对话框状态。
pub struct ImportDialog {
    pub draft: Entity<TextField>,
    pub error: Option<String>,
}

/// 网络设置对话框状态（代理 + 超时）。
pub struct NetDialog {
    pub proxy: Entity<TextField>,
    pub timeout: Entity<TextField>,
    pub error: Option<String>,
}

/// 配置页的 tab（None = 关闭）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    /// 外观/下拉框样式
    Styles,
    /// 语言、工作区
    General,
    /// 代理、超时
    Network,
    /// 自动备份、备份与恢复
    Backup,
}

/// 配置页「外观」里可调的四项字体设置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontField {
    /// 界面字体家族（空 = 系统默认）
    UiFamily,
    UiSize,
    MonoFamily,
    MonoSize,
}

/// 把响应里的值存成环境变量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarSource {
    /// 响应正文按 JSONPath 取
    Body,
    /// 响应头按名字取
    Header,
}

/// 「存为变量」对话框状态。
pub struct SaveVarDialog {
    pub source: VarSource,
    pub path: Entity<TextField>,
    pub name: Entity<TextField>,
    pub error: Option<String>,
}

/// 备份 / 恢复对话框状态。
pub struct RestoreDialog {
    /// 备份文件（新 → 旧）
    pub items: Vec<treq_core::backup::BackupInfo>,
    pub selected: usize,
    /// 备份目录（显示用）
    pub dir: PathBuf,
    /// 操作结果/错误提示
    pub message: Option<String>,
    pub failed: bool,
}

/// Ctrl+K 命令面板：搜索请求 / 历史。
pub struct QuickPalette {
    pub field: Entity<TextField>,
    /// 过滤关键字
    pub query: String,
    /// (目标, 显示标签)
    pub list: Vec<(QuickTarget, String)>,
    pub selected: usize,
}

#[derive(Clone, PartialEq)]
pub enum QuickTarget {
    History(i64),
    /// 别的（或当前）工作区里的请求：命中后先切工作区再选中
    CrossRequest {
        workspace: PathBuf,
        request: String,
    },
}

/// 侧栏树的一行（扁平化后交给 uniform_list 虚拟滚动渲染）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Collection,
    Group,
    Request,
}

/// 行在工作区树里的位置（Copy，零分配）：
/// `req` 有值 = 请求行；仅 `grp` 有值 = 分组行；都为空 = 集合行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRef {
    pub col: usize,
    pub grp: Option<usize>,
    pub req: Option<usize>,
}

/// 侧栏树的一行：只存“位置 + 状态”，名称/URL 等渲染时按需取，
/// 所以每帧摊平 1400 行不产生堆分配。
#[derive(Clone, Copy)]
pub struct TreeRow {
    pub at: RowRef,
    /// 请求行最近一次发送的状态码
    pub status: Option<u16>,
    pub selected: bool,
    pub renaming: bool,
    pub collapsed: bool,
    /// 缩进层级（见 [`crate::theme::tree_indent`]）
    pub depth: u8,
}

/// 跨工作区索引项（命令面板用）。
#[derive(Clone)]
pub struct CrossHit {
    pub workspace: PathBuf,
    pub workspace_name: String,
    pub request: RequestItem,
    /// "集合/分组"
    pub container: String,
    /// 预拼好的小写检索串：名称/URL/方法/集合(分组)/工作区名。
    /// 建索引时算一次，之后每次按键只做子串匹配（不再逐项 to_lowercase 分配）。
    pub index_text: String,
}

impl CrossHit {
    pub fn new(
        workspace: PathBuf,
        workspace_name: String,
        request: RequestItem,
        container: String,
    ) -> Self {
        let index_text = crate::search::haystack(&[
            &request.name,
            &request.url,
            &request.method,
            &container,
            &workspace_name,
        ]);
        Self {
            workspace,
            workspace_name,
            request,
            container,
            index_text,
        }
    }
}

impl crate::search::Hit for CrossHit {
    fn workspace(&self) -> &std::path::Path {
        &self.workspace
    }
    fn haystack(&self) -> &str {
        &self.index_text
    }
}

/// 环境编辑器状态。
pub struct EnvEditor {
    /// "base" 或环境 id
    pub target: String,
    pub vars: Vec<(String, String)>,
    pub fields: Vec<(Entity<TextField>, Entity<TextField>)>,
}

/// 请求编辑器字段持久化缓存：key = "{which}:{req_id}:{index}"。
/// 结构变化（增删行/换选中）时整体重建，避免 TextField 失焦与错位。
pub struct EditorFields {
    /// 认证表单的两个输入框（按 slot 缓存；换认证方式时清空重建）
    pub auth: std::collections::HashMap<String, Entity<TextField>>,
    pub auth_kind: Option<String>,
    pub kv: HashMap<String, Entity<TextField>>,
    pub url: Option<Entity<TextField>>,
    pub body: Option<Entity<TextField>>,
    /// File 类型的请求体：文件路径输入框
    pub body_file: Option<Entity<TextField>>,
    /// Docs 页的请求说明（多行）
    pub docs: Option<Entity<TextField>>,
    /// 当前缓存对应的请求 id；选中变化时重建
    pub for_request: Option<String>,
    /// URL 解析预览（随输入更新）
    pub resolved_url: String,
}

impl EditorFields {
    pub(crate) fn new() -> Self {
        Self {
            auth: HashMap::new(),
            auth_kind: None,
            kv: HashMap::new(),
            url: None,
            body: None,
            body_file: None,
            docs: None,
            for_request: None,
            resolved_url: String::new(),
        }
    }
}

/// 数一块数据里的 SSE 事件分隔（空行）；CRLF 也认。
fn count_event_breaks(bytes: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            // \n\n 或 \r\n\r\n（后者在上一个 \n 处已计过一次，这里跳过）
            if bytes.get(i + 1) == Some(&b'\n') {
                n += 1;
                i += 2;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'\r') && bytes.get(i + 2) == Some(&b'\n') {
                n += 1;
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    n
}

/// 人类可读的字节数（状态栏用）
fn fmt_bytes(n: usize) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.)
    } else {
        format!("{:.2} MB", n as f64 / (1024. * 1024.))
    }
}

/// 事件流正文最多留在界面里多少字节（可能无限长，再长只提示已截断）
const MAX_STREAM_BYTES: usize = 2 * 1024 * 1024;
/// 普通（非事件流）响应的接收上限：收完再渲染，显示层另有 2MB 渲染护栏。
/// 64MB 只是防病态响应的兜底，正常业务数据远小于它。
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// 流式界面刷新节流：最快 8 帧/秒，别让每来一块就重建行缓存
const STREAM_FLUSH_MS: u64 = 120;

/// 拖动侧栏节点时的落点位置。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropZone {
    /// 插到这一行前面
    Before,
    /// 放进这一行里面（分组 / 集合）
    Inside,
    /// 插到这一行后面
    After,
}

/// 当前落点（高亮指示用）。
#[derive(Clone, Copy, Debug)]
pub struct DropTarget {
    pub at: RowRef,
    pub zone: DropZone,
    /// 目标行的屏幕矩形：指示线按它画
    pub rect: Bounds<Pixels>,
}

/// 侧栏拖拽中的节点。
#[derive(Clone, Debug)]
pub(crate) struct TreeDrag {
    pub kind: RowKind,
    /// 请求 / 分组 id（重载后仍能定位）
    pub id: String,
    pub target: Option<DropTarget>,
}

/// 按下但还没开始拖的那一行。
#[derive(Clone, Debug)]
pub(crate) struct TreePress {
    pub kind: RowKind,
    pub id: String,
    pub y: f32,
}

/// 正在拖的滚动条：目标 + 鼠标相对滑块的抓取点 + 轨道几何。
struct BarDrag {
    target: crate::widgets::BarTarget,
    /// 滚动区矩形（算进度、换算行号用）
    view: Bounds<Pixels>,
    /// 鼠标相对滑块顶部的距离：抓住的那一点始终跟着指针
    grab: f32,
    track_top: f32,
    track_h: f32,
    thumb_h: f32,
}

/// `ScrollHandle` 没有「设到某个像素偏移」的接口，只能「滚到第 n 项顶部」。
/// 树 / KV 表都是行高一致的 uniform_list，所以按行高换算过去是准的。
fn scroll_handle_to_progress(h: &ScrollHandle, prog: f32) {
    let max = f32::from(h.max_offset().height);
    if max <= 0. {
        return;
    }
    // 往下滚 offset.y 越来越负（gpui 约定），所以目标偏移直接取负值
    let x = f32::from(h.offset().x);
    h.set_offset(point(px(x), px(-prog * max)));
}

pub struct AppModel {
    pub settings: Settings,
    /// cookie 罐：响应里的 Set-Cookie 存这儿，之后同域请求自动带上
    pub cookies: std::sync::Arc<std::sync::Mutex<CookieJar>>,
    pub cookie_dialog: bool,
    pub cookie_message: Option<String>,
    /// 导入结果的提示（状态栏那行）。
    pub flash: Option<String>,
    /// toast 令牌：自动消失的定时器只清自己那条（后来的消息不被前面那条的定时器清掉）
    pub(crate) flash_token: u64,
    /// 变量面板是否打开 / 是否显示明文（默认打码）
    pub vars_panel: bool,
    pub vars_reveal: bool,
    pub store: WorkspaceStore,
    pub workspace: Workspace,
    pub selection: Option<Selection>,
    pub response: Option<ResponseData>,
    pub sending: bool,
    /// 正在接收的请求 id：响应/事件只回填给它（切走或换请求后旧流的事件一律丢弃）
    pub stream_req: Option<String>,
    /// 事件流的取消开关（core 每秒看一眼，1 秒内断开连接）
    pub stream_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// 响应头声明是事件流（SSE）时为真：正文边收边显示
    pub stream_live: bool,
    pub stream_events: usize,
    pub stream_bytes: usize,
    pub stream_started: Option<std::time::Instant>,
    pub stream_flush: Option<std::time::Instant>,
    /// 流式收尾提示：已停止 / 已截断
    pub stream_notice: Option<String>,
    /// 事件流：每块正文首行的行号 + 到达时间（"HH:MM:SS.mmm"）。只用于显示时间列，不进正文
    pub sse_marks: Vec<(usize, String)>,
    /// 事件流累计收到的正文行数（给下一块算起始行号）
    pub sse_lines: usize,
    /// 事件流正文左侧是否显示到达时间（响应工具条可切，记进设置）
    pub sse_show_time: bool,
    /// 外观栏的「编辑器行高」输入框（只建一次，避免每帧重建丢焦点）
    pub editor_line_field: Option<Entity<TextField>>,
    /// 字体设置的四个输入框（按 `FontField` 顺序）
    pub font_fields: [Option<Entity<TextField>>; 4],
    /// 本帧活着的滚动区 id（根部按这些画常显竖滚动条；每帧开头清空）
    pub scroll_live: Vec<&'static str>,
    /// 变高列表（gpui::list）单独登记一根（它自带滚动条接口）
    pub scroll_lists: Vec<gpui::ListState>,
    /// 滚动区句柄表（id → 句柄；跨帧留着，滚动位置不丢）
    pub scroll_handles: std::collections::HashMap<&'static str, ScrollHandle>,
    pub popup: PopupMenu,
    pub rename: Option<RenameState>,
    pub editor_tab: EditorTab,
    pub fields: EditorFields,
    pub response_tab: ResponseTab,
    /// Body 视图：Pretty（JSON 美化着色）或 Raw（原文）
    pub response_pretty: bool,
    /// 当前选中请求的历史（响应区 History tab 只展示这一份）
    pub history: Vec<HistoryEntry>,
    /// 全部请求的最近若干条（命令面板按 URL/名字搜索用）
    pub recent_all: Vec<HistoryEntry>,
    /// 浏览器式前进/后退栈（跨工作区）
    pub nav: NavStack,
    /// Ctrl+K 跨工作区索引：打开面板时构建一次，之后按键只做内存过滤
    pub cross_index: Vec<CrossHit>,
    pub history_store: HistoryStore,
    pub import_dialog: Option<ImportDialog>,
    pub net_dialog: Option<NetDialog>,
    /// 配置页（设置）打开时的 tab
    pub settings_page: Option<SettingsTab>,
    /// 配置页样式 tab 里正在预览打开态的那一款（点样例时设置）。
    pub style_preview: Option<DropdownStyle>,
    /// 「查看完整 URL」弹框（工具条 URL 框放不下时用）。
    pub url_dialog: bool,
    /// 长值全量查看弹框（Params/Headers/Form-Data 的 value 太长时）
    pub kv_zoom: Option<KvZoom>,
    pub restore_dialog: Option<RestoreDialog>,
    /// 自动备份线程要盯的配置（工作区 / 间隔 / 保留份数，改动时立刻写这里，线程下一轮就按新的来）
    pub backup_cfg: std::sync::Arc<std::sync::Mutex<backup::BackupCfg>>,
    pub env_editor: Option<EnvEditor>,
    pub trash_store: TrashStore,
    pub trash_dialog: bool,
    /// 回收站「清空」待确认（第一次点只是亮一下）
    pub trash_confirm: bool,
    /// 左树宽度（可拖动，持久化）
    pub tree_w: f32,
    /// 响应面板宽度（可拖动，持久化）
    pub panel_w: f32,
    /// 拖动状态：(目标, 起点 x, 起点宽度)
    pub drag: Option<(DragTarget, f32, f32)>,
    /// 代码生成弹窗：当前语言
    /// JSON 正文「美化」的报错（内容一改就清）
    pub body_error: Option<String>,
    pub codegen: Option<CodegenLang>,
    /// 代码生成针对哪个请求（树右键指定；None = 当前选中）
    pub codegen_pin: Option<String>,
    /// 侧栏搜索关键字
    pub search_query: String,
    /// 侧栏树摊平后的可见行（每帧重建；虚拟列表只渲染其中可见的一段）
    pub rows: Vec<TreeRow>,
    /// 侧栏搜索输入框实体
    pub search_field: Option<Entity<TextField>>,
    /// 每个请求最近一次状态码：req_id -> (sent_at, status)
    pub last_status: HashMap<String, (i64, Option<u16>)>,
    /// 已折叠的集合/分组 id（会话内状态）
    pub collapsed: HashSet<String>,
    /// Ctrl+K 命令面板
    pub quick: Option<QuickPalette>,
    /// 当前窗口标题缓存（避免每帧重复设置平台标题）
    pub last_window_title: String,
    /// KV 行描述是否展开（Insomnia 的 Toggle Description）
    pub kv_show_desc: bool,
    /// 关闭弹窗时保留的草稿：cURL 导入文本
    pub import_draft: String,
    pub proxy_draft: String,
    pub timeout_draft: String,
    /// 关闭弹窗时保留的草稿：环境变量编辑（target -> vars）
    pub env_drafts: HashMap<String, Vec<(String, String)>>,
    /// 关闭命令面板时保留的搜索词
    pub quick_query: String,
    /// 「存为变量」对话框
    pub save_var_dialog: Option<SaveVarDialog>,
    /// 「移动到…」选择器
    pub move_dialog: Option<MoveDialog>,
    /// 响应面板 JSONPath 过滤条
    pub resp_filter: String,
    pub resp_filter_field: Option<Entity<TextField>>,
    /// 大响应：用户点了「完整」就强行渲染（默认跳过，见 jsonview::MAX_RENDER_BYTES）
    pub resp_force_full: bool,
    /// 响应体显示行缓存（pretty/filter 后的结果）+ 缓存键 + 序号
    pub resp_lines: Vec<String>,
    /// 折叠前的完整行（resp_lines 是折叠后的可见行）+ 配对表 + 折叠集合
    pub resp_all_lines: Vec<String>,
    pub resp_fold_ends: Vec<Option<usize>>,
    pub resp_folded: std::collections::HashSet<usize>,
    /// 每个可见行对应 resp_all_lines 里的哪一行（行号 / 箭头都按原始行来）
    pub resp_visible_idx: Vec<usize>,
    pub resp_lines_json: bool,
    pub resp_lines_notice: Option<String>,
    // (resp_gen, 过滤词, pretty, 强行完整渲染, 中英文)
    pub(crate) resp_lines_key: Option<(u64, String, bool, bool, bool)>,
    /// 每次收到响应自增，用于让上面的缓存失效
    pub(crate) resp_gen: u64,
    /// 每个请求「上次的响应」：切走再切回来还看得到（Postman/Insomnia 的老规矩），
    /// 按插入顺序当 LRU 用，超过 16 条或 32MB 从头丢。
    pub resp_cache: Vec<(String, ResponseData)>,
    /// 响应正文的鼠标选中 + 选中期间左键是否还按着
    pub resp_sel: Option<RespSel>,
    pub(crate) resp_sel_drag: bool,
    /// 最近一次鼠标悬停在正文哪一列（行由那一行元素报上来）—— 按下时拿它当起点
    pub(crate) resp_hover_col: Option<usize>,
    /// 正在拖的滚动条。挂在模型上：指针移出窄带后根部监听接着管，手势不会断。
    bar_drag: Option<BarDrag>,
    /// 侧栏按下的行 / 正在拖的节点 / 当前落点
    tree_press: Option<TreePress>,
    tree_drag: Option<TreeDrag>,
    /// 正文容器的焦点：点正文才拿焦点，⌘C 复制选中文本
    pub resp_focus: FocusHandle,
    /// 响应体的变高虚拟列表状态（只测量可见行）
    pub resp_list: gpui::ListState,
    /// 上次测量时的宽度：宽度变了行高就变了，必须重新测量
    pub(crate) resp_list_w: (f32, f32),
    /// 侧栏树列表的滚动句柄（跳转后把选中行滚进视野）
    pub tree_scroll: UniformListScrollHandle,
    /// 请求区内容（Body 编辑器等）的滚动句柄
    pub editor_scroll: ScrollHandle,
    /// 待滚进视野的请求 id（下一帧 rebuild_rows 消费一次）
    pub(crate) pending_reveal: Option<Selection>,
}

/// 用导入描述里的字段覆盖刚建的占位请求，再写盘（文件名/位置保持 store 决定的那份）。
fn save_imported_request(
    store: &WorkspaceStore,
    col_id: &str,
    group_id: Option<&str>,
    src: &RequestItem,
) -> anyhow::Result<RequestItem> {
    let mut created = store.create_request_in(col_id, group_id, &src.name)?;
    let id = created.id.clone();
    created.method = src.method.clone();
    created.url = src.url.clone();
    created.params = src.params.clone();
    created.headers = src.headers.clone();
    created.body = src.body.clone();
    created.description = src.description.clone();
    created.auth = src.auth.clone();
    created.docs_open = false;
    created.id = id;
    store.save_request(&created)?;
    Ok(created)
}

impl AppModel {
    /// 打开/关闭弹出菜单。
    /// 关键：必须 stop_propagation —— 菜单是 mousedown 打开的，事件继续冒泡到根节点
    /// 的「点击任意处关闭菜单」处理器，会把刚打开的菜单立刻关掉。
    pub(crate) fn toggle_popup(
        &mut self,
        source: &str,
        x: f32,
        y: f32,
        items: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        let same = self.popup.open && self.popup.source.as_deref() == Some(source);
        if same {
            self.popup.close();
        } else {
            self.popup.open = true;
            self.popup.x = x;
            self.popup.y = y;
            self.popup.source = Some(source.to_string());
            self.popup.items = items;
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// 当前请求解析后的完整 URL（变量已替换、缺协议补 http://）；没选请求就是空。
    pub fn full_url(&self) -> String {
        let Some(req) = self.selected_request() else {
            return String::new();
        };
        let vars = vars::merge_env(&self.workspace.base_env, self.active_env());
        treq_core::http::normalize_url(&vars::resolve_request(req, &vars).url)
    }

    pub fn open_url_dialog(&mut self, cx: &mut Context<Self>) {
        self.ensure_editor_fields(cx);
        self.popup.close();
        self.url_dialog = true;
        cx.notify();
    }

    pub fn close_url_dialog(&mut self, cx: &mut Context<Self>) {
        self.url_dialog = false;
        cx.notify();
    }

    /// 配置页样式样例被点：选中该款 + 用真实菜单弹出打开态预览。
    pub fn preview_dropdown_style(
        &mut self,
        style: DropdownStyle,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.set_dropdown_style(style, cx);
        self.style_preview = Some(style);
        let items = ["editor.tab.json", "editor.tab.text", "editor.tab.form"]
            .iter()
            .enumerate()
            .map(|(i, key)| (format!("style-preview:{}", i), self.t(key).to_string()))
            .collect();
        let p = (pos.x.to_f64() as f32, pos.y.to_f64() as f32 + 4.0);
        self.toggle_popup("style-preview", p.0, p.1, items, cx)
    }

    /// 某个下拉/菜单当前是否打开（画打开态用）。
    pub fn dropdown_open(&self, source: &str) -> bool {
        self.popup.open && self.popup.source.as_deref() == Some(source)
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = settings::load();
        let tree_w = settings.tree_width.unwrap_or(theme::DEFAULT_TREE_W);
        let panel_w = settings.panel_width.unwrap_or(theme::DEFAULT_PANEL_W);
        let sse_show_time = settings.sse_show_time.unwrap_or(true);
        let store = WorkspaceStore::new(settings.workspace_root.clone());
        let mut m = AppModel {
            workspace: Workspace {
                collections: vec![],
                base_env: Environment {
                    id: "base".into(),
                    name: "Base".into(),
                    variables: Default::default(),
                },
                environments: vec![],
            },
            settings,
            cookies: std::sync::Arc::new(std::sync::Mutex::new(load_cookie_jar())),
            cookie_dialog: false,
            cookie_message: None,
            flash: None,
            flash_token: 0,
            vars_panel: false,
            vars_reveal: false,
            store,
            selection: None,
            response: None,
            sending: false,
            stream_req: None,
            stream_cancel: None,
            stream_live: false,
            stream_events: 0,
            stream_bytes: 0,
            stream_started: None,
            stream_flush: None,
            stream_notice: None,
            sse_marks: Vec::new(),
            sse_lines: 0,
            sse_show_time,
            editor_line_field: None,
            font_fields: Default::default(),
            scroll_live: Vec::new(),
            scroll_lists: Vec::new(),
            scroll_handles: std::collections::HashMap::new(),
            popup: PopupMenu::default(),
            rename: None,
            editor_tab: EditorTab::Params,
            fields: EditorFields::new(),
            // 初始显示历史记录（尚未发送时响应面板也有内容）
            response_tab: ResponseTab::History,
            response_pretty: true,
            history: vec![],
            recent_all: vec![],
            nav: NavStack::new(),
            cross_index: vec![],
            env_editor: None,
            trash_store: TrashStore::new(
                dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("com.treq.app")
                    .join("trash"),
            ),
            trash_dialog: false,
            trash_confirm: false,
            tree_w,
            panel_w,
            drag: None,
            codegen: None,
            body_error: None,
            codegen_pin: None,
            history_store: HistoryStore::open(
                dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("com.treq.app")
                    .join("history.db"),
            )
            .unwrap_or_else(|_| {
                HistoryStore::open(std::env::temp_dir().join("treq-history.db")).unwrap()
            }),
            import_dialog: None,
            net_dialog: None,
            settings_page: None,
            style_preview: None,
            url_dialog: false,
            kv_zoom: None,
            restore_dialog: None,
            backup_cfg: std::sync::Arc::new(std::sync::Mutex::new(backup::BackupCfg::default())),
            search_query: String::new(),
            rows: Vec::new(),
            search_field: None,
            last_status: HashMap::new(),
            collapsed: HashSet::new(),
            quick: None,
            last_window_title: String::new(),
            kv_show_desc: false,
            import_draft: String::new(),
            proxy_draft: String::new(),
            timeout_draft: String::new(),
            env_drafts: HashMap::new(),
            quick_query: String::new(),
            save_var_dialog: None,
            move_dialog: None,
            resp_filter: String::new(),
            resp_filter_field: None,
            resp_force_full: false,
            resp_lines: Vec::new(),
            resp_all_lines: Vec::new(),
            resp_fold_ends: Vec::new(),
            resp_folded: std::collections::HashSet::new(),
            resp_visible_idx: Vec::new(),
            resp_lines_json: false,
            resp_lines_notice: None,
            resp_lines_key: None,
            resp_gen: 0,
            resp_cache: Vec::new(),
            resp_list: gpui::ListState::new(0, gpui::ListAlignment::Top, px(200.)),
            resp_sel: None,
            resp_sel_drag: false,
            resp_hover_col: None,
            bar_drag: None,
            tree_press: None,
            tree_drag: None,
            resp_focus: cx.focus_handle(),
            resp_list_w: (0., 0.),
            tree_scroll: UniformListScrollHandle::new(),
            editor_scroll: ScrollHandle::new(),
            pending_reveal: None,
        };
        // 内容行高是全应用共享的显示常量（正文/JSON/表格都读它）
        theme::set_line_h(settings::editor_line_h(&m.settings));
        theme::set_fonts(
            &settings::font_ui(&m.settings),
            settings::font_ui_size(&m.settings),
            &settings::font_mono(&m.settings),
            settings::font_mono_size(&m.settings),
        );
        m.reload();
        // 恢复上次打开的请求（找不到就退回第一个），右侧编辑器直接展示内容
        m.restore_last_request(cx);
        m.auto_select_first();
        m.load_history(cx);
        m.start_auto_backup();
        m
    }

    /// 后台定时备份：每 30s 醒来一次，距上次备份超过间隔且工作区有改动才打 zip。
    /// interval 为 0 表示关闭。
    pub fn locale(&self) -> Locale {
        Locale::from_str(&self.settings.locale)
    }
    pub fn t(&self, key: &str) -> &'static str {
        tr(self.locale(), key)
    }

    pub fn reload(&mut self) {
        self.store.ensure_root().ok();
        if let Ok(ws) = self.store.load() {
            self.workspace = ws;
        }
        self.fields = EditorFields::new();
        self.refresh_last_status();
    }

    /// 启动/切工作区时恢复上次打开的请求（连同它的历史记录一起加载）。
    pub fn restore_last_request(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.settings.last_request().cloned() else {
            return;
        };
        if self.request_exists(&id) {
            self.do_select_request_inner(id, cx);
        }
    }

    /// 没有任何选中时自动选中工作区第一个请求（初始/刷新后）。
    pub fn auto_select_first(&mut self) {
        if self.selection.is_some() {
            return;
        }
        let first = self
            .workspace
            .collections
            .iter()
            .flat_map(|c| {
                c.requests
                    .iter()
                    .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
            })
            .next()
            .map(|r| r.id.clone());
        if let Some(id) = first {
            self.selection = Some(Selection::Request(id));
        }
    }
    /// 从历史构建 req_id -> 最近状态码（用于树行右侧角标）。
    fn refresh_last_status(&mut self) {
        self.last_status.clear();
        if let Ok(entries) = self.history_store.list(500) {
            for h in entries {
                let e = self
                    .last_status
                    .entry(h.request.id.clone())
                    .or_insert((0, None));
                if h.sent_at >= e.0 || e.1.is_none() {
                    *e = (h.sent_at, h.status);
                }
            }
        }
    }

    /// 折叠/展开集合或分组。
    pub fn toggle_collapsed(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.collapsed.remove(id) {
            self.collapsed.insert(id.to_string());
        }
        cx.notify();
    }

    /// 是否有任何展开的集合/分组（决定「折叠全部」按钮显示哪种图标）。
    pub(crate) fn any_expanded(&self) -> bool {
        crate::search::any_expanded(&self.workspace.collections, &self.collapsed)
    }

    /// 折叠全部 / 全部展开（一键切换）。
    pub fn toggle_collapse_all(&mut self, cx: &mut Context<Self>) {
        if self.any_expanded() {
            let ids: Vec<String> = crate::search::collapse_ids(&self.workspace.collections)
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            self.collapsed.extend(ids);
        } else {
            self.collapsed.clear();
        }
        cx.notify();
    }

    /// 折叠所有节点，只留当前节点这一条路径：祖先保持展开（否则当前节点自己会被藏起来），
    /// 当前节点自己的展开/折叠状态不动。右键菜单「折叠所有（除此节点）」。
    pub fn collapse_all_except(&mut self, sel: Selection, cx: &mut Context<Self>) {
        let node = match &sel {
            Selection::Collection(id) | Selection::Group(id) | Selection::Request(id) => id.clone(),
        };
        // 原来是展开的就不折它；原来折着的就让它继续折着
        let node_was_expanded = !self.collapsed.contains(&node);
        self.collapsed = crate::search::collapse_all_except_ids(
            &self.workspace.collections,
            &sel,
            node_was_expanded,
        );
        // 折完行数变少，滚动位置要跟着重算，顺便把当前节点滚进视野
        self.pending_reveal = Some(sel);
        cx.notify();
    }

    /// 从外部（命令面板/历史）跳到某请求：清掉可能把它藏起来（过滤掉）的侧栏过滤词，
    /// 并请求把侧栏滚到该行（非严格滚动，本来就可见时不动）。
    pub fn reveal_request(&mut self, id: String, cx: &mut Context<Self>) {
        self.clear_side_filter(cx);
        self.pending_reveal = Some(Selection::Request(id.clone()));
        self.do_select_request(id, cx);
    }

    /// 消费 pending_reveal：在摊平后的行里找到目标行，滚进视野。
    pub(crate) fn consume_reveal(&mut self) {
        let Some(sel) = self.pending_reveal.clone() else {
            return;
        };
        self.pending_reveal = None;
        if let Some(i) = crate::search::node_row_index(&self.rows, &self.workspace.collections, &sel)
        {
            self.tree_scroll.scroll_to_item(i, ScrollStrategy::Center);
        }
    }

    /// 把某个节点（集合/分组/请求）滚进视野：先清掉可能藏住它的侧栏过滤词，
    /// 行要等下一帧重建后才有，所以只记 pending。
    pub fn reveal_node(&mut self, sel: Selection, cx: &mut Context<Self>) {
        self.clear_side_filter(cx);
        self.pending_reveal = Some(sel);
        cx.notify();
    }

    /// 清空侧栏过滤框（新建/跳转后想看见目标行时用）。
    fn clear_side_filter(&mut self, cx: &mut Context<Self>) {
        if self.search_query.is_empty() {
            return;
        }
        self.search_query.clear();
        if let Some(f) = self.search_field.clone() {
            f.update(cx, |f, cx| {
                f.content = SharedString::from("");
                f.selected_range = 0..0;
                f.marked_range = None;
                cx.notify();
            });
        }
    }

    /// 展开包含 id 的祖先（集合/分组），保证选中项不被折叠节点隐藏。
    fn expand_ancestors_of(&mut self, id: &str) {
        if self.collapsed.is_empty() {
            return;
        }
        let cols = &self.workspace.collections;
        let mut open: Vec<String> = Vec::new();
        for c in cols {
            if c.id == id || c.requests.iter().any(|r| r.id == id) {
                open.push(c.id.clone());
            }
            let owner = c
                .groups
                .iter()
                .find(|g| g.id == id || g.requests.iter().any(|r| r.id == id));
            if let Some(g) = owner {
                open.extend(crate::search::group_ancestors(cols, &g.id));
                open.push(g.id.clone());
            }
        }
        for id in open {
            self.collapsed.remove(&id);
        }
    }

    /// 把请求区（Body 编辑器等）滚回顶部：切请求/切 tab 时用，
    /// 否则上一个请求滚到下面的位置会带到新内容上。
    pub(crate) fn editor_scroll_top(&self) {
        self.editor_scroll.set_offset(point(px(0.), px(0.)));
    }

    /// 侧栏搜索框（按需创建，避免每次渲染重建导致失焦）。
    pub(crate) fn ensure_search_field(&mut self, cx: &mut Context<Self>) -> Entity<TextField> {
        if let Some(f) = &self.search_field {
            return f.clone();
        }
        let handle = cx.entity();
        let field = TextField::new(
            self.search_query.clone().into(),
            SharedString::from(self.t("side.search")),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| {
                    this.search_query = s.to_string();
                    cx.notify();
                });
            }),
            cx,
        );
        self.search_field = Some(field.clone());
        field
    }

    pub fn active_env(&self) -> Option<&Environment> {
        self.settings
            .active_environment_id
            .as_ref()
            .and_then(|id| self.workspace.environments.iter().find(|e| &e.id == id))
    }

    pub fn selected_request(&self) -> Option<&RequestItem> {
        let sel = self.selection.as_ref()?;
        let Selection::Request(rid) = sel else {
            return None;
        };
        self.workspace
            .collections
            .iter()
            .flat_map(|c| {
                c.requests
                    .iter()
                    .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
            })
            .find(|r| &r.id == rid)
    }

    /// 取（或建）一个 y 滚动区的句柄：所有滚动区都从这里拿，
    /// 根部最后统一画常显竖滚动条（本帧登记过的才会画）。
    pub fn scroll_handle(&mut self, id: &'static str) -> ScrollHandle {
        let h = self.scroll_handles.entry(id).or_default().clone();
        if !self.scroll_live.contains(&id) {
            self.scroll_live.push(id);
        }
        h
    }

    /// 登记一个句柄在别处的滚动区（树 / 响应正文 / 编辑区）。
    pub fn track_scrollbar(&mut self, id: &'static str, handle: &ScrollHandle) {
        self.scroll_handles.insert(id, handle.clone());
        if !self.scroll_live.contains(&id) {
            self.scroll_live.push(id);
        }
    }

    /// 变高列表（`gpui::list`）登记一根滚动条。
    pub fn track_list_scrollbar(&mut self, state: &gpui::ListState) {
        self.scroll_lists.push(state.clone());
    }

    /// 所有滚动区的常显竖滚动条：延迟绘制 → 压在所有内容之上（对话框/菜单用更高的
    /// deferred 优先级压回来）。条本身可点可拖，事件回调需要模型，所以带上 `cx`。
    pub fn scrollbars_layer(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut layer = div().absolute().left(px(0.)).top(px(0.));
        let weak = cx.weak_entity();
        for id in &self.scroll_live {
            if let Some(h) = self.scroll_handles.get(id) {
                layer = layer.child(gpui::deferred(crate::widgets::VBar::from_scroll(
                    h,
                    weak.clone(),
                )));
            }
        }
        for s in &self.scroll_lists {
            layer = layer.child(gpui::deferred(crate::widgets::VBar::from_list(
                s,
                weak.clone(),
            )));
        }
        layer.into_any()
    }

    /// 按在滚动条上：滑块上是抓住它，轨道上是先跳过去再抓（macOS 的手感）。
    pub(crate) fn bar_drag_begin(
        &mut self,
        target: crate::widgets::BarTarget,
        mouse_y: f32,
        thumb: Bounds<Pixels>,
        view: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        use crate::widgets::{BAR_INSET, BarTarget};
        if let BarTarget::List(s) = &target {
            // 拖动时要按「固定内容总高」换算，先冻住（免得边拖边重算，滑块乱跳）
            s.scrollbar_drag_started();
        }
        let thumb_top = f32::from(thumb.origin.y);
        let thumb_h = f32::from(thumb.size.height);
        let track_h = (f32::from(view.size.height) - 2. * BAR_INSET).max(1.);
        self.bar_drag = Some(BarDrag {
            target,
            view,
            grab: (mouse_y - thumb_top).clamp(0., thumb_h),
            track_top: f32::from(view.origin.y) + BAR_INSET,
            track_h,
            thumb_h,
        });
        self.bar_drag_to(mouse_y, cx);
    }

    pub(crate) fn bar_drag_move(&mut self, mouse_y: f32, cx: &mut Context<Self>) {
        if self.bar_drag.is_some() {
            self.bar_drag_to(mouse_y, cx);
        }
    }

    pub(crate) fn bar_drag_end(&mut self, cx: &mut Context<Self>) {
        if let Some(d) = self.bar_drag.take() {
            if let crate::widgets::BarTarget::List(s) = &d.target {
                s.scrollbar_drag_ended();
            }
            cx.notify();
        }
    }

    /// 把鼠标 y 换算成进度，再落到具体的滚动目标上。
    fn bar_drag_to(&mut self, mouse_y: f32, cx: &mut Context<Self>) {
        use crate::widgets::BarTarget;
        let Some(d) = self.bar_drag.as_ref() else {
            return;
        };
        let span = (d.track_h - d.thumb_h).max(1.);
        let y = (mouse_y - d.grab - d.track_top).clamp(0., span);
        let prog = y / span;
        match &d.target {
            BarTarget::List(s) => {
                let top = s.logical_scroll_top();
                let row_h = s
                    .bounds_for_item(top.item_ix)
                    .map(|b| f32::from(b.size.height))
                    .unwrap_or(0.);
                let ix = crate::widgets::list_item_for_progress(
                    d.view,
                    s.item_count(),
                    row_h,
                    prog,
                );
                s.scroll_to(gpui::ListOffset {
                    item_ix: ix,
                    offset_in_item: px(0.),
                });
            }
            BarTarget::Scroll(h) => scroll_handle_to_progress(h, prog),
        }
        cx.notify();
    }

    pub fn set_locale(&mut self, l: Locale, cx: &mut Context<Self>) {
        self.settings.locale = l.as_str().into();
        settings::save(&self.settings).ok();
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload();
        cx.notify();
    }

    // ---- 工作区（项目） ----

    /// 当前工作区路径。
    pub fn workspace_path(&self) -> PathBuf {
        self.settings.workspace_root.clone()
    }

    pub fn workspace_name(&self) -> String {
        WorkspaceRef::from_path(self.workspace_path()).name
    }

    /// 切换到指定工作区：换 store、重载树、重置选中/响应、清掉跨区索引。
    pub fn switch_workspace(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path == self.settings.workspace_root {
            // 已经在这个工作区：只把菜单收掉
            self.popup.close();
            cx.notify();
            return;
        }
        self.settings.set_active_workspace(path);
        settings::save(&self.settings).ok();
        self.store = WorkspaceStore::new(self.settings.workspace_root.clone());
        self.sync_backup_cfg();
        self.selection = None;
        self.response = None;
        self.resp_force_full = false;
        self.resp_gen = self.resp_gen.wrapping_add(1);
        self.fields = EditorFields::new();
        self.cross_index.clear();
        self.reload();
        self.restore_last_request(cx);
        self.auto_select_first();
        self.load_history(cx);
        self.popup.close();
        cx.notify();
    }

    /// 选目录新增一个工作区并切过去。
    pub fn add_workspace(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(self.t("workspace.add"))),
        });
        let handle = cx.entity();
        cx.spawn(async move |_window, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = handle.update(cx, |this, cx| this.switch_workspace(path, cx));
            }
        })
        .detach();
        self.popup.close();
        cx.notify();
    }

    /// 从列表移除工作区（不删磁盘文件）；被移除的是当前工作区时回落到列表第一个。
    pub fn forget_workspace(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let was_current = self.settings.forget_workspace(&path);
        self.nav.forget_workspace(&path);
        settings::save(&self.settings).ok();
        self.popup.close();
        if was_current {
            self.switch_workspace(self.settings.workspace_root.clone(), cx);
            // switch_workspace 对「同一路径」会提前返回，这里强制重载
            self.store = WorkspaceStore::new(self.settings.workspace_root.clone());
            self.selection = None;
            self.response = None;
            self.resp_gen = self.resp_gen.wrapping_add(1);
            self.fields = EditorFields::new();
            self.reload();
            self.auto_select_first();
            self.load_history(cx);
        }
        cx.notify();
    }

    /// 顶栏工作区面包屑菜单：切换 / 新增 / 移除。
    pub fn open_workspace_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let current = self.workspace_path();
        let mut items: Vec<(String, String)> = self
            .settings
            .workspaces
            .iter()
            .map(|w| {
                let mark = if w.path == current { "● " } else { "   " };
                (
                    format!("ws:{}", w.path.display()),
                    format!("{}{}", mark, w.name),
                )
            })
            .collect();
        items.push(("ws-add".into(), self.t("workspace.add").to_string()));
        if self.settings.workspaces.len() > 1 {
            items.push((
                format!("ws-forget:{}", current.display()),
                self.t("workspace.remove").to_string(),
            ));
        }
        let p = (pos.x.to_f64() as f32, pos.y.to_f64() as f32 + 4.0);
        self.toggle_popup("workspace", p.0, p.1, items, cx);
    }

    // ---- 前进 / 后退 ----

    /// 记录一次访问（用户主动跳转时调用）。
    fn nav_push(&mut self, req_id: &str) {
        self.nav
            .push(NavEntry::new(self.workspace_path(), req_id.to_string()));
    }

    /// 跳到导航栈里的某个位置（切换工作区 + 选中请求），不再入栈。
    fn nav_apply(&mut self, entry: NavEntry, cx: &mut Context<Self>) {
        if entry.workspace != self.settings.workspace_root {
            self.settings.set_active_workspace(entry.workspace);
            settings::save(&self.settings).ok();
            self.store = WorkspaceStore::new(self.settings.workspace_root.clone());
            self.fields = EditorFields::new();
            self.reload();
        }
        self.do_select_request_nav(entry.request, cx);
    }

    pub fn nav_back(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = self.nav.back() {
            self.nav_apply(e, cx);
        }
        cx.notify();
    }

    pub fn nav_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = self.nav.forward() {
            self.nav_apply(e, cx);
        }
        cx.notify();
    }

    // ---- 选择 ----
    /// 保留：集合行点击现在触发折叠；此方法供右键菜单等场景扩展。
    #[allow(dead_code)]
    pub fn select_collection(&mut self, id: String, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.workspace.collections.iter().any(|c| c.id == id) {
            cx.notify();
            return;
        }
        self.selection = Some(Selection::Collection(id));
        self.fields = EditorFields::new();
        cx.notify();
    }
    #[allow(dead_code)]
    pub fn select_group(&mut self, id: String, _window: &mut Window, cx: &mut Context<Self>) {
        let exists = self
            .workspace
            .collections
            .iter()
            .flat_map(|c| c.groups.iter())
            .any(|g| g.id == id);
        if !exists {
            cx.notify();
            return;
        }
        self.selection = Some(Selection::Group(id));
        self.fields = EditorFields::new();
        cx.notify();
    }

    pub fn select_request(&mut self, id: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.nav_push(&id);
        self.do_select_request(id, cx);
    }

    /// 导航回放：选中请求但不写导航栈。
    fn do_select_request_nav(&mut self, id: String, cx: &mut Context<Self>) {
        self.do_select_request_inner(id, cx);
    }

    /// select_request 的无窗口版本（Ctrl+K 面板、历史恢复用）。
    pub fn do_select_request(&mut self, id: String, cx: &mut Context<Self>) {
        self.nav_push(&id);
        self.do_select_request_inner(id, cx);
    }

    fn do_select_request_inner(&mut self, id: String, cx: &mut Context<Self>) {
        // 切走时停掉正在收的事件流：连接不必挂着，旧流的事件也会被 stream_req 校验丢弃
        if self.stream_req.as_deref() != Some(id.as_str()) {
            if let Some(c) = &self.stream_cancel {
                c.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.stream_cancel = None;
            self.stream_req = None;
            self.stream_live = false;
            self.stream_notice = None;
            self.sending = false;
            self.stream_started = None;
        }
        let exists = self
            .workspace
            .collections
            .iter()
            .flat_map(|c| {
                c.requests
                    .iter()
                    .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
            })
            .any(|r| r.id == id);
        if !exists {
            cx.notify();
            return;
        }
        self.expand_ancestors_of(&id);
        self.pending_reveal = Some(Selection::Request(id.clone()));
        self.editor_scroll_top();
        // 记住现场：下次启动直接回到这个请求
        self.settings.set_last_request(&id);
        settings::save(&self.settings).ok();
        // 切回来时把上次的响应放回来看（每个请求各存各的，不会张冠李戴）
        self.response = self.cached_response(&id);
        self.selection = Some(Selection::Request(id));
        self.fields = EditorFields::new();
        self.resp_force_full = false;
        self.resp_gen = self.resp_gen.wrapping_add(1);
        self.load_history(cx);
    }

    // ---- CRUD ----
    pub fn create_collection(&mut self, cx: &mut Context<Self>) {
        if let Ok(col) = self
            .store
            .create_collection(self.t("action.new_collection"))
        {
            // 新建的排最前面（列表是自上而下扫，放末尾要滚到底才看得见）
            self.workspace.collections.insert(0, col.clone());
            self.reveal_node(Selection::Collection(col.id.clone()), cx);
            self.begin_rename(Selection::Collection(col.id), cx);
        }
    }
    pub fn create_request(&mut self, collection_id: &str, cx: &mut Context<Self>) {
        self.create_request_in(collection_id, None, cx);
    }

    pub fn create_request_in(
        &mut self,
        collection_id: &str,
        group_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        if let Ok(req) = self
            .store
            .create_request_in(collection_id, group_id, "Untitled")
        {
            let group_id = group_id.map(|g| g.to_string());
            match &group_id {
                Some(gid) => {
                    if let Some(g) = self
                        .workspace
                        .collections
                        .iter_mut()
                        .find(|c| c.id == collection_id)
                        .and_then(|c| c.groups.iter_mut().find(|g| g.id == *gid))
                    {
                        g.requests.insert(0, req.clone());
                    }
                }
                None => {
                    if let Some(c) = self
                        .workspace
                        .collections
                        .iter_mut()
                        .find(|c| c.id == collection_id)
                    {
                        c.requests.insert(0, req.clone());
                    }
                }
            }
            // 选中 + 展开祖先 + 滚进视野（新建的行在最上面，可能不在当前视野里）
            self.reveal_request(req.id.clone(), cx);
            self.begin_rename(Selection::Request(req.id), cx);
        }
    }

    /// 新建分组；`parent` = 上级分组 id（None = 集合第一层）。
    pub fn create_group(
        &mut self,
        collection_id: &str,
        parent: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Ok(g) = self
            .store
            .create_group(collection_id, self.t("action.new_group"), parent)
        {
            if let Some(c) = self
                .workspace
                .collections
                .iter_mut()
                .find(|c| c.id == collection_id)
            {
                c.groups.insert(0, g.clone());
            }
            self.expand_ancestors_of(&g.id);
            self.reveal_node(Selection::Group(g.id.clone()), cx);
            self.begin_rename(Selection::Group(g.id), cx);
        }
    }

    pub fn delete_group(&mut self, collection_id: &str, group_id: &str, cx: &mut Context<Self>) {
        // 整个分组（含子分组、子分组里的请求）丢进回收站：原来是硬删除，删错了找不回来。
        // 子分组必须一起走，否则它们的 parent 指向已删分组，重开就从树里消失。
        let doomed = self.group_subtree_ids(collection_id, group_id);
        let ok = doomed.iter().all(|gid| {
            self.trash_store
                .trash_group(self.store.root(), collection_id, gid)
                .is_ok()
        });
        if ok {
            if let Some(c) = self
                .workspace
                .collections
                .iter_mut()
                .find(|c| c.id == collection_id)
            {
                c.groups.retain(|g| !doomed.contains(&g.id));
            }
            let clear = matches!(&self.selection, Some(Selection::Group(x)) if doomed.contains(x));
            if clear {
                self.selection = None;
            }
            cx.notify();
        }
    }

    /// 该分组 + 所有后代分组的 id（parent 链，环用 contains 挡住不会转圈）。
    fn group_subtree_ids(&self, collection_id: &str, group_id: &str) -> Vec<String> {
        self.workspace
            .collections
            .iter()
            .find(|c| c.id == collection_id)
            .map(|c| crate::search::group_subtree_ids(&c.groups, group_id))
            .unwrap_or_else(|| vec![group_id.to_string()])
    }
    pub fn delete_collection(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .trash_store
            .trash_collection(self.store.root(), id)
            .is_ok()
        {
            self.workspace.collections.retain(|c| c.id != id);
            let clear = match &self.selection {
                Some(Selection::Collection(x)) => x == id,
                Some(Selection::Group(gid)) => !self
                    .workspace
                    .collections
                    .iter()
                    .flat_map(|c| c.groups.iter())
                    .any(|g| &g.id == gid),
                Some(Selection::Request(rid)) => !self
                    .workspace
                    .collections
                    .iter()
                    .flat_map(|c| {
                        c.requests
                            .iter()
                            .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
                    })
                    .any(|r| &r.id == rid),
                None => false,
            };
            if clear {
                self.selection = None;
                self.fields = EditorFields::new();
            }
            cx.notify();
        }
    }
    pub fn delete_request(&mut self, id: &str, cx: &mut Context<Self>) {
        // 找到它到底在集合根下还是分组里 —— 组内请求的文件在 groups/<gid>/requests/ 下，
        // 不区分的话 trash_request 找不到文件，删除会静默失败。
        let Some((col_id, group_id)) = self.workspace.collections.iter().find_map(|c| {
            if c.requests.iter().any(|r| r.id == id) {
                return Some((c.id.clone(), None));
            }
            c.groups
                .iter()
                .find(|g| g.requests.iter().any(|r| r.id == id))
                .map(|g| (c.id.clone(), Some(g.id.clone())))
        }) else {
            return;
        };
        if self
            .trash_store
            .trash_request(self.store.root(), &col_id, group_id.as_deref(), id)
            .is_ok()
        {
            if let Some(c) = self
                .workspace
                .collections
                .iter_mut()
                .find(|c| c.id == col_id)
            {
                c.requests.retain(|r| r.id != id);
                for g in c.groups.iter_mut() {
                    g.requests.retain(|r| r.id != id);
                }
            }
            self.nav.forget_request(id);
            self.resp_cache.retain(|(k, _)| k != id);
            if matches!(&self.selection, Some(Selection::Request(x)) if x == id) {
                self.selection = None;
                self.fields = EditorFields::new();
                self.load_history(cx);
            }
            cx.notify();
        }
    }

    // ---- 重命名（行内编辑） ----
    pub fn begin_rename(&mut self, target: Selection, cx: &mut Context<Self>) {
        let current = match &target {
            Selection::Collection(id) => self
                .workspace
                .collections
                .iter()
                .find(|c| &c.id == id)
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            Selection::Request(id) => self
                .selected_request()
                .filter(|r| &r.id == id)
                .map(|r| r.name.clone())
                .unwrap_or_default(),
            Selection::Group(id) => self
                .workspace
                .collections
                .iter()
                .flat_map(|c| c.groups.iter())
                .find(|g| &g.id == id)
                .map(|g| g.name.clone())
                .unwrap_or_default(),
        };
        let field = TextField::new(
            current.clone().into(),
            SharedString::from(""),
            Arc::new(|_, _| {}),
            cx,
        );
        // 全选，便于直接重打
        field.update(cx, |f, cx| {
            f.selected_range = 0..f.content.len();
            cx.notify();
        });
        let handle = cx.entity();
        let target_for_submit = target.clone();
        field.update(cx, |f, _cx| {
            f.on_submit = Some(Arc::new(move |_window, app| {
                // submit 跑在改名框自己的 update 里，此刻读它会撞 gpui 双重租借
                // （panic「cannot read TextField while it is already being updated」），
                // 所以推迟到本次更新之后再提交。详见 widgets::SubmitCb
                let handle = handle.clone();
                let target = target_for_submit.clone();
                app.defer(move |app| {
                    handle.update(app, |this, cx| this.commit_rename(&target, cx));
                });
            }));
        });
        self.rename = Some(RenameState {
            target,
            field,
            focus: true,
        });
        cx.notify();
    }

    pub fn commit_rename(&mut self, target: &Selection, cx: &mut Context<Self>) {
        let Some(state) = self.rename.take() else {
            return;
        };
        let name = state.field.read(cx).content.to_string();
        let name = name.trim().to_string();
        if name.is_empty() {
            cx.notify();
            return;
        }
        match target {
            Selection::Collection(id) => {
                if let Some(c) = self.workspace.collections.iter_mut().find(|c| &c.id == id) {
                    c.name = name;
                    let _ = self.store.save_collection(c);
                }
            }
            Selection::Request(id) => {
                // 请求可能在集合下，也可能在分组里 —— 两处都要找，
                // 否则分组里的请求改名既不生效也不落盘（旧版只扫集合直属）
                let renamed = find_request_mut(&mut self.workspace, id).map(|r| {
                    r.name = name;
                    r.clone()
                });
                if let Some(r) = renamed {
                    let _ = self.store.save_request(&r);
                }
            }
            Selection::Group(id) => {
                let mut target: Option<(String, Group)> = None;
                for c in self.workspace.collections.iter_mut() {
                    if let Some(g) = c.groups.iter_mut().find(|g| &g.id == id) {
                        g.name = name;
                        target = Some((c.id.clone(), g.clone()));
                        break;
                    }
                }
                if let Some((cid, g)) = target {
                    let _ = self.store.save_group(&cid, &g);
                }
            }
        }
        cx.notify();
    }

    // ---- 弹出菜单 ----
    /// 侧栏「＋」的菜单：新建请求 / 分组 / 集合。
    ///
    /// 目标是当前选中项所在的集合（选中分组、或选中分组里的请求，就在那个分组里建请求）；
    /// 一个集合都没有的时候没地方放东西，直接建集合。
    pub fn open_new_item_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((col, group)) = self.new_item_target() else {
            self.create_collection(cx);
            return;
        };
        let items = vec![
            (
                match &group {
                    Some(gid) => format!("new-request:{}:{}", col, gid),
                    None => format!("new-request:{}", col),
                },
                self.t("action.new_request").to_string(),
            ),
            (
                match &group {
                    Some(gid) => format!("new-group:{}:{}", col, gid),
                    None => format!("new-group:{}", col),
                },
                self.t("action.new_group").to_string(),
            ),
            (
                "new-collection".into(),
                self.t("action.new_collection").to_string(),
            ),
        ];
        self.toggle_popup(
            "new-item",
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32,
            items,
            cx,
        );
    }

    /// 「＋」往哪儿放：选中请求 → 它所在的分组（没有就是集合）；选中分组 → 那个分组；
    /// 选中集合 → 那个集合；没选/选中的东西已经不在树里 → 第一个集合。
    fn new_item_target(&self) -> Option<(String, Option<String>)> {
        crate::search::new_item_target(&self.workspace.collections, self.selection.as_ref())
    }

    pub fn open_collection_menu(
        &mut self,
        col_id: String,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.toggle_popup(
            "collection",
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32,
            vec![
                (
                    format!("new-request:{}", col_id),
                    self.t("action.new_request").to_string(),
                ),
                (
                    format!("new-group:{}", col_id),
                    self.t("action.new_group").to_string(),
                ),
                (
                    "new-collection".into(),
                    self.t("action.new_collection").to_string(),
                ),
                (
                    "import-curl".into(),
                    self.t("action.import_curl").to_string(),
                ),
                (
                    "import-file".into(),
                    self.t("action.import_file").to_string(),
                ),
                (
                    format!("collapse-others-col:{}", col_id),
                    self.t("action.collapse_others").to_string(),
                ),
                (
                    format!("rename-col:{}", col_id),
                    self.t("action.rename").to_string(),
                ),
                (
                    format!("delete-col:{}", col_id),
                    self.t("action.delete").to_string(),
                ),
            ],
            cx,
        );
    }

    pub fn open_request_menu(
        &mut self,
        req_id: String,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.toggle_popup(
            "request",
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32,
            vec![
                (
                    format!("codegen-req:{}", req_id),
                    self.t("action.plugin").to_string(),
                ),
                (
                    format!("copy-url:{}", req_id),
                    self.t("action.copy_url").to_string(),
                ),
                (
                    format!("copy-curl:{}", req_id),
                    self.t("action.copy_curl").to_string(),
                ),
                (
                    format!("dup-req:{}", req_id),
                    self.t("action.duplicate").to_string(),
                ),
                (
                    format!("move-ask:{}", req_id),
                    self.t("action.move_to").to_string(),
                ),
                (
                    format!("collapse-others-req:{}", req_id),
                    self.t("action.collapse_others").to_string(),
                ),
                (
                    format!("rename-req:{}", req_id),
                    self.t("action.rename").to_string(),
                ),
                (
                    format!("delete-req:{}", req_id),
                    self.t("action.delete").to_string(),
                ),
            ],
            cx,
        );
    }

    /// 把请求挪到目标集合/分组（只动文件，然后重载工作区）。
    pub fn move_request_to(
        &mut self,
        req_id: &str,
        col_id: &str,
        group_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let where_to = self
            .workspace
            .collections
            .iter()
            .find(|c| c.id == col_id)
            .map(|c| match group_id {
                Some(gid) => c
                    .groups
                    .iter()
                    .find(|g| g.id == gid)
                    .map(|g| format!("{} / {}", c.name, g.name))
                    .unwrap_or_else(|| c.name.clone()),
                None => format!("{}{}", c.name, self.t("action.move_root")),
            })
            .unwrap_or_else(|| col_id.to_string());
        match self.store.move_request(req_id, col_id, group_id) {
            Ok(_) => {
                // 重载工作区 + 树（磁盘是唯一真相）
                self.workspace = self.store.load().unwrap_or_else(|_| self.workspace.clone());
                self.refresh(cx);
                self.popup.open = false;
                self.toast(format!("{}{}", self.t("flash.moved"), where_to), cx);
            }
            Err(e) => {
                self.toast(format!("{}{}", self.t("flash.move_failed"), e), cx);
            }
        }
        cx.notify();
    }

    pub fn open_env_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let mut items = vec![("env:base".into(), self.workspace.base_env.name.clone())];
        for e in &self.workspace.environments {
            items.push((format!("env:{}", e.id), e.name.clone()));
        }
        items.push((
            "env-edit:base".into(),
            self.t("action.edit_env").to_string(),
        ));
        items.push(("env-vars".into(), self.t("vars.title").to_string()));
        self.toggle_popup(
            "env",
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32 + 4.0,
            items,
            cx,
        );
    }

    pub fn open_group_menu(
        &mut self,
        group_id: String,
        col_id: String,
        pos: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.toggle_popup(
            "group",
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32,
            vec![
                (
                    format!("new-request:{}:{}", col_id, group_id),
                    self.t("action.new_request").to_string(),
                ),
                (
                    format!("new-group:{}:{}", col_id, group_id),
                    self.t("action.new_group").to_string(),
                ),
                (
                    format!("collapse-others-group:{}", group_id),
                    self.t("action.collapse_others").to_string(),
                ),
                (
                    format!("rename-group:{}:{}", col_id, group_id),
                    self.t("action.rename").to_string(),
                ),
                (
                    format!("delete-group:{}:{}", col_id, group_id),
                    self.t("action.delete").to_string(),
                ),
            ],
            cx,
        )
    }

    pub fn open_method_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let items = [
            "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE",
        ]
        .iter()
        .map(|m| (format!("method:{}", m), m.to_string()))
        .collect();
        let p = (pos.x.to_f64() as f32, pos.y.to_f64() as f32 + 4.0);
        self.toggle_popup("method", p.0, p.1, items, cx)
    }

    pub fn open_body_kind_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let items = [
            (BodyKind::None, "editor.tab.none"),
            (BodyKind::Json, "editor.tab.json"),
            (BodyKind::Text, "editor.tab.text"),
            (BodyKind::Form, "editor.tab.form"),
            (BodyKind::Multipart, "editor.tab.multipart"),
            (BodyKind::File, "editor.tab.file"),
            (BodyKind::Raw, "editor.tab.raw"),
        ]
        .iter()
        .map(|(k, key)| (format!("body-kind:{:?}", k), self.t(key).to_string()))
        .collect();
        let p = (pos.x.to_f64() as f32, pos.y.to_f64() as f32 + 4.0);
        self.toggle_popup("body-kind", p.0, p.1, items, cx)
    }

    /// 工具条右侧「5 Hours Ago ▾」：当前请求的历史快照（点击恢复）。
    pub fn open_history_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let req_id = match &self.selection {
            Some(Selection::Request(id)) => id.clone(),
            _ => return,
        };
        let now = treq_core::now_millis();
        let mut items = vec![];
        for h in self
            .history
            .iter()
            .filter(|h| h.request.id == req_id)
            .take(12)
        {
            let status = h
                .status
                .map(|s| s.to_string())
                .or_else(|| h.error.as_ref().map(|_| "✗".to_string()))
                .unwrap_or_default();
            let label = format!(
                "{}  {}  {}",
                status,
                fmt_duration_ms(h.duration_ms),
                self.ago_label(h.sent_at, now)
            );
            items.push((format!("hist:{}", h.id), label));
        }
        if items.is_empty() {
            return;
        }
        let p = (pos.x.to_f64() as f32, pos.y.to_f64() as f32 + 4.0);
        self.toggle_popup("menu", p.0, p.1, items, cx)
    }

    /// 状态栏「Preferences」：设置页 / 语言 / 工作区 / 刷新 / 网络 / 插件 / 回收站。
    pub fn open_prefs_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let items = vec![
            (
                "prefs:settings".to_string(),
                self.t("settings.title").to_string(),
            ),
            (
                "prefs:locale".to_string(),
                self.t("action.language").to_string(),
            ),
            (
                "prefs:workspace".to_string(),
                self.t("action.choose_workspace").to_string(),
            ),
            (
                "prefs:reload".to_string(),
                self.t("action.reload").to_string(),
            ),
            ("prefs:net".to_string(), self.t("net.title").to_string()),
            (
                "prefs:cookies".to_string(),
                format!(
                    "{}{}{}{}",
                    self.t("cookies.title"),
                    self.t("cookies.count_prefix"),
                    self.cookie_count(),
                    self.t("cookies.count_suffix")
                ),
            ),
            (
                "prefs:codegen".to_string(),
                self.t("action.plugin").to_string(),
            ),
            ("prefs:trash".to_string(), self.t("trash.title").to_string()),
        ];
        // 位置夹取交给 popup_render（贴底菜单自动上移）
        self.toggle_popup(
            "prefs",
            pos.x.to_f64() as f32,
            pos.y.to_f64() as f32,
            items,
            cx,
        )
    }

    pub fn menu_action(&mut self, id: &str, cx: &mut Context<Self>) {
        self.popup.close();
        match id {
            "new-collection" => return self.create_collection(cx),
            "import-curl" => return self.open_import_dialog(cx),
            "import-file" => return self.import_file(cx),
            "prefs:net" => return self.open_net_dialog(cx),
            "prefs:cookies" => return self.open_cookie_dialog(cx),
            "backup-now" => return self.backup_now(cx),
            "backup-restore" => return self.open_restore_dialog(cx),
            "plugin:codegen" => return self.open_codegen(cx),
            _ => {}
        }
        if id == "ws-add" {
            self.add_workspace(cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("ws-forget:") {
            self.forget_workspace(PathBuf::from(rest), cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("ws:") {
            self.switch_workspace(PathBuf::from(rest), cx);
            return;
        }
        // 代码生成弹窗里的语言/库选择
        if let Some(rest) = id.strip_prefix("codegen:") {
            if let Some(lang) = CodegenLang::from_id(rest) {
                self.set_codegen_lang(lang, cx);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("prefs:") {
            match rest {
                "settings" => return self.open_settings_page(SettingsTab::Styles, cx),
                "locale" => self.toggle_locale(cx),
                "workspace" => self.choose_workspace(cx),
                "reload" => self.refresh(cx),
                "codegen" => self.open_codegen(cx),
                "trash" => self.open_trash_dialog(cx),
                _ => {}
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("codegen-req:") {
            self.open_codegen_for(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("copy-url:") {
            self.copy_request_url(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("copy-curl:") {
            self.copy_request_curl(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("dup-req:") {
            self.duplicate_request(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("move-ask:") {
            self.open_move_dialog(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("resp-line:") {
            // resp-line:{动词}:{原始行号}
            let mut parts = rest.splitn(2, ':');
            let verb = parts.next().unwrap_or_default();
            let line = parts.next().and_then(|x| x.parse::<usize>().ok());
            if let Some(line) = line {
                match verb {
                    "copy" => self.copy_resp_line(line, cx),
                    "value" => self.copy_resp_line_value(line, cx),
                    "path" => self.copy_resp_line_path(line, cx),
                    "var" => self.save_resp_line_var(line, cx),
                    _ => {}
                }
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("hist:") {
            if let Ok(hid) = rest.parse::<i64>()
                && let Some(h) = self.history.iter().find(|h| h.id == hid).cloned()
            {
                self.restore_history(&h, cx);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("resp-view:") {
            self.response_pretty = rest == "pretty";
            self.response_tab = ResponseTab::Body;
            cx.notify();
            return;
        }
        if let Some(rest) = id.strip_prefix("new-group:") {
            // 两种形态：<col_id> 或 <col_id>:<group_id>（后者建成那个分组的子分组）
            if let Some((cid, gid)) = rest.split_once(':') {
                self.create_group(cid, Some(gid.to_string()), cx);
            } else {
                self.create_group(rest, None, cx);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("new-request:") {
            // 两种形态：<col_id> 或 <col_id>:<group_id>
            if let Some((cid, gid)) = rest.split_once(':') {
                self.create_request_in(cid, Some(gid), cx);
            } else {
                self.create_request(rest, cx);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("rename-group:") {
            if let Some((_cid, gid)) = rest.split_once(':') {
                self.begin_rename(Selection::Group(gid.to_string()), cx);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("delete-group:") {
            if let Some((cid, gid)) = rest.split_once(':') {
                self.delete_group(cid, gid, cx);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("rename-col:") {
            self.begin_rename(Selection::Collection(rest.to_string()), cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("rename-req:") {
            self.begin_rename(Selection::Request(rest.to_string()), cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("collapse-others-col:") {
            self.collapse_all_except(Selection::Collection(rest.to_string()), cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("collapse-others-group:") {
            self.collapse_all_except(Selection::Group(rest.to_string()), cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("collapse-others-req:") {
            self.collapse_all_except(Selection::Request(rest.to_string()), cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("delete-col:") {
            self.delete_collection(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("delete-req:") {
            self.delete_request(rest, cx);
            return;
        }
        if let Some(rest) = id.strip_prefix("env-edit:") {
            self.open_env_editor(rest, cx);
            return;
        }
        if id == "env-vars" {
            return self.open_vars_panel(cx);
        }
        if let Some(rest) = id.strip_prefix("env:") {
            self.set_active_env(rest, cx);
            return;
        }
        if let Some(m) = id.strip_prefix("method:") {
            self.set_method(m.to_string(), cx);
            return;
        }
        if let Some(kind) = id.strip_prefix("body-kind:") {
            let kind = match kind {
                "None" => BodyKind::None,
                "Json" => BodyKind::Json,
                "Text" => BodyKind::Text,
                "Form" => BodyKind::Form,
                "Multipart" => BodyKind::Multipart,
                "File" => BodyKind::File,
                _ => BodyKind::Raw,
            };
            self.set_body_kind(kind, cx);
            return;
        }
        cx.notify();
    }

    // ---- 分区拖动 ----
    pub fn start_drag(&mut self, target: DragTarget, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let start_x = pos.x.to_f64() as f32;
        let start_w = match target {
            DragTarget::Tree => self.tree_w,
            DragTarget::Panel => self.panel_w,
        };
        self.drag = Some((target, start_x, start_w));
        cx.notify();
    }

    pub fn drag_move(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((target, start_x, start_w)) = self.drag else {
            return;
        };
        let dx = pos.x.to_f64() as f32 - start_x;
        match target {
            // 树：向右拖变宽
            DragTarget::Tree => {
                self.tree_w = (start_w + dx).clamp(180.0, 480.0);
            }
            // 响应：向右拖变窄
            DragTarget::Panel => {
                self.panel_w = (start_w - dx).clamp(280.0, 640.0);
            }
        }
        cx.notify();
    }

    pub fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            self.settings.tree_width = Some(self.tree_w);
            self.settings.panel_width = Some(self.panel_w);
            settings::save(&self.settings).ok();
            cx.notify();
        }
    }

    pub fn choose_workspace(&mut self, cx: &mut Context<Self>) {
        let handle = cx.entity();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from("treq workspace")),
        });
        cx.spawn(async move |_window, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                handle
                    .update(cx, |this, cx| {
                        this.settings.workspace_root = path;
                        this.store = WorkspaceStore::new(this.settings.workspace_root.clone());
                        settings::save(&this.settings).ok();
                        this.reload();
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub fn toggle_locale(&mut self, cx: &mut Context<Self>) {
        let next = match self.locale() {
            Locale::Zh => Locale::En,
            Locale::En => Locale::Zh,
        };
        self.set_locale(next, cx);
    }
}

impl AppModel {
    /// 垂直分隔条（可拖动调整分区宽度）。
    pub(crate) fn h_separator(
        &mut self,
        target: DragTarget,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = matches!(self.drag, Some((t, _, _)) if t == target);
        // 面板分隔条左移自身宽度：响应区与编辑区无缝相接（发送按钮紧贴分隔线，右侧不留空隙）。
        // 拖动热区只是压在发送按钮最右 4px 上，悬停高亮仍可见，光标变左右箭头即知可拖。
        let flush = target == DragTarget::Panel;
        div()
            .id(SharedString::from(format!("sep/{:?}", target)))
            .flex_none()
            .w(px(4.))
            .when(flush, |d| d.ml(px(-4.)))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|d| d.bg(theme::bg_selected()))
            .when(active, |d| d.bg(theme::primary()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    move |this: &mut AppModel,
                          e: &MouseDownEvent,
                          _w: &mut Window,
                          cx: &mut Context<AppModel>| {
                        this.start_drag(target, e.position, cx);
                    }
                }),
            )
    }

    /// 应用顶栏：`工作区 / 请求名` 面包屑（Insomnia 顶栏）。
    pub(crate) fn app_header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = self
            .selected_request()
            .map(|r| r.name.clone())
            .unwrap_or_else(|| {
                // 集合/分组选中时也显示名称
                match &self.selection {
                    Some(Selection::Collection(id)) => self
                        .workspace
                        .collections
                        .iter()
                        .find(|c| &c.id == id)
                        .map(|c| c.name.clone())
                        .unwrap_or_default(),
                    Some(Selection::Group(id)) => self
                        .workspace
                        .collections
                        .iter()
                        .flat_map(|c| c.groups.iter())
                        .find(|g| &g.id == id)
                        .map(|g| g.name.clone())
                        .unwrap_or_default(),
                    _ => String::new(),
                }
            });

        // 同步原生窗口标题（标题栏）：当前请求/集合名 + treq
        let window_title = if title.is_empty() {
            "treq".to_string()
        } else {
            format!("{} — treq", title)
        };
        if self.last_window_title != window_title {
            self.last_window_title = window_title.clone();
            window.set_window_title(&window_title);
        }

        div()
            .id("app-header")
            .h(theme::header_h())
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::bg_base())
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::sp1())
                    // 品牌
                    .child(
                        div()
                            .px(theme::sp2())
                            .text_size(px(theme::font_body()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg_normal())
                            .child("treq"),
                    )
                    .child(div().text_size(px(theme::font_small())).text_color(theme::fg_dark()).child("/"))
                    // 工作区（项目）切换：点这里出菜单
                    .child(
                        div()
                            .id("workspace-chip")
                            .flex()
                            .items_center()
                            .gap(theme::sp2())
                            .px(theme::sp2())
                            .h(px(22.))
                            .rounded(px(3.))
                            .cursor_pointer()
                            .text_size(px(theme::font_body()))
                            .text_color(theme::fg_normal())
                            .hover(|d| d.bg(theme::bg_hover()).text_color(theme::fg_bright()))
                            .child(SharedString::from(self.workspace_name()))
                            .child(crate::widgets::chevron(
                                theme::fg_dim(),
                                self.dropdown_open("workspace"),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, _w: &mut Window, cx: &mut Context<AppModel>| {
                                    this.open_workspace_menu(e.position, cx)
                                }),
                            ),
                    )
                    // 当前请求
                    .when(!title.is_empty(), |d| {
                        d.child(div().text_size(px(theme::font_small())).text_color(theme::fg_dark()).child("/"))
                            .child(
                                div()
                                    .max_w(px(320.))
                                    .truncate()
                                    .px(theme::sp2())
                                    .text_size(px(theme::font_body()))
                                    .text_color(theme::fg_dim())
                                    .child(SharedString::from(title.clone())),
                            )
                    }),
            )
    }
}

impl Render for AppModel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 环境变量同步给输入框（供 `{{` 补全），只在变化时写
        self.sync_field_vars(cx);
        // 滚动条登记表按帧重建：只有本帧渲染到的滚动区才画条
        self.scroll_live.clear();
        self.scroll_lists.clear();
        // 配了界面字体才覆盖（空 = 保持 gpui 的系统默认字体）
        let ui_font = theme::ui_font();
        let mut root = div()
            .id("root")
            .size_full()
            .bg(theme::bg_base())
            .flex()
            .flex_col()
            .relative()
            .when_some(ui_font, |d, f| d.font(f))
            // 点击菜单外任意处关闭。打开菜单的控件会 stop_propagation，
            // 所以这里只负责「点到别处」的情况（否则菜单一打开就被这一枪关掉）。
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _e, _w, cx| {
                    if this.popup.open {
                        this.popup.close();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _e, _w, cx| {
                    if this.popup.open {
                        this.popup.close();
                        cx.notify();
                    }
                }),
            )
            // 拖着滚动条时指针可能跑出那条窄带，剩下的路在这里接着走；
            // 侧栏拖拽也一样（拖到别的行上时那些行的 hover 会抢事件，只能靠根部兜底）
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _w, cx| {
                this.bar_drag_move(f32::from(e.position.y), cx);
                this.tree_drag_move(e.position, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _e, window, cx| {
                    this.bar_drag_end(cx);
                    this.tree_drag_end(window, cx);
                }),
            )
            // Ctrl+K 面板键盘导航（Enter 提交见 TextField.on_submit）
            .on_action(cx.listener(|this, _: &QuickUp, _w, cx| this.quick_move(-1, cx)))
            .on_action(cx.listener(|this, _: &QuickDown, _w, cx| this.quick_move(1, cx)))
            .child(self.app_header(window, cx))
            .child(
                div()
                    .id("content-row")
                    .flex()
                    .flex_1()
                    .flex_row()
                    .min_h_0()
                    .child(self.rail_pane(cx))
                    .child(self.tree_pane(window, cx))
                    .child(self.h_separator(DragTarget::Tree, window, cx))
                    .child(self.editor_pane(window, cx))
                    .child(self.h_separator(DragTarget::Panel, window, cx))
                    .child(self.response_pane(window, cx))
                    // 拖动期间：全局捕捉 move/up
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _w, cx| {
                        this.drag_move(e.position, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _e, _w, cx| {
                            this.end_drag(cx);
                        }),
                    ),
            )
            .child(self.status_bar(cx));
        if let Some(field) = self.suggest_target(cx) {
            root = root.child(self.suggest_overlay(field, window, cx));
        }
        if self.settings_page.is_some() {
            root = root.child(self.settings_dialog_render(cx));
        }
        if self.restore_dialog.is_some() {
            root = root.child(self.restore_dialog_render(cx));
        }
        if self.vars_panel {
            root = root.child(self.vars_panel_render(cx));
        }
        if self.import_dialog.is_some() {
            root = root.child(self.import_dialog_render(cx));
        }
        if self.move_dialog.is_some() {
            root = root.child(self.move_dialog_render(window, cx));
        }
        if self.save_var_dialog.is_some() {
            root = root.child(self.save_var_dialog_render(cx));
        }
        if self.cookie_dialog {
            root = root.child(self.cookie_dialog_render(cx));
        }
        if self.env_editor.is_some() {
            root = root.child(self.env_editor_render(cx));
        }
        if self.trash_dialog {
            root = root.child(self.trash_dialog_render(cx));
        }
        if self.codegen.is_some() {
            root = root.child(self.codegen_dialog_render(window, cx));
        }
        if self.quick.is_some() {
            root = root.child(self.quick_render(window, cx));
        }
        if self.url_dialog {
            root = root.child(self.url_dialog_render(window, cx));
        }
        if self.kv_zoom.is_some() {
            root = root.child(self.kv_zoom_render(cx));
        }
        // 菜单最后画：对话框（如配置页的样式预览）上的菜单也在最上层。
        // 滚动条是延迟绘制的，菜单/toast 也得延迟绘制并给更高优先级才压得住。
        if self.popup.open {
            root = root.child(gpui::deferred(self.popup_render(window, cx)).with_priority(20));
        }
        // toast 比菜单还后：复制/保存反馈不能被对话框遮住
        if let Some(msg) = self.flash.clone() {
            root = root.child(gpui::deferred(self.toast_render(msg)).with_priority(30));
        }
        // 拖动中的插入线：贴在目标行上/下边缘
        if let Some(t) = self.drop_indicator()
            && t.zone != DropZone::Inside
        {
            let y = match t.zone {
                DropZone::Before => t.rect.origin.y,
                _ => t.rect.origin.y + t.rect.size.height,
            };
            root = root.child(
                div()
                    .absolute()
                    .left(t.rect.origin.x)
                    .top(y - px(1.))
                    .w(t.rect.size.width)
                    .h(px(2.))
                    .bg(theme::primary()),
            );
        }
        // 滚动条画在最上层（延迟绘制，压在正文/菜单之上）
        root = root.child(self.scrollbars_layer(cx));
        // 改名框刚出现：把键盘焦点放进去（新建后直接能打字）
        if let Some(state) = self.rename.as_mut()
            && state.focus
        {
            state.focus = false;
            let handle = state.field.read(cx).focus_handle.clone();
            window.focus(&handle);
        }
        root
    }
}

// 菜单栏动作（main.rs 注册）
actions!(
    treq,
    [
        Quit,
        ChooseWorkspace,
        ImportFile,
        ToggleLocale,
        Reload,
        QuickSwitch,
        QuickClose,
        QuickUp,
        QuickDown,
        SendNow,
        FocusResponseFilter,
        DuplicateRequest,
        OpenPrefs,
        BackupNow,
        RestoreBackup,
        NavBack,
        NavForward
    ]
);

/// 按 id 找可改的请求（集合直属或分组内都认）。
fn find_request_mut<'a>(
    ws: &'a mut treq_core::Workspace,
    id: &str,
) -> Option<&'a mut treq_core::RequestItem> {
    for c in ws.collections.iter_mut() {
        if let Some(r) = c.requests.iter_mut().find(|r| &r.id == id) {
            return Some(r);
        }
        for g in c.groups.iter_mut() {
            if let Some(r) = g.requests.iter_mut().find(|r| &r.id == id) {
                return Some(r);
            }
        }
    }
    None
}

#[cfg(test)]
mod popup_size_tests {
    use super::PopupMenu;
    use crate::theme;

    fn size(labels: &[&str]) -> (f32, f32) {
        let mut p = PopupMenu::default();
        p.items = labels.iter().map(|l| (l.to_string(), l.to_string())).collect();
        p.size_hint()
    }

    #[test]
    fn menu_width_follows_the_longest_label() {
        // 短标签 → 下限 180（太窄的菜单点起来瞎点）
        assert_eq!(size(&["新建"]).0, 180.0);
        // 超长标签 → 上限 420（再宽就顶到窗口另一边了）
        assert_eq!(
            size(&["折叠所有其它节点（除当前节点）折叠所有其它节点（除当前节点）"]).0,
            420.0
        );
        // 同样 12 个字，中文比英文宽 → 菜单也更宽
        assert!(
            size(&["中文中文中文中文中文中文"]).0 > size(&["aaaaaaaaaaaa"]).0,
            "中文标签要算得更宽"
        );
        // 宽度看的是最长的那一项，不是加起来的
        assert_eq!(size(&["新建", "删除"]).0, size(&["新建"]).0);
        assert!(size(&["新建", "重命名重命名重命名重命名重命名"]).0 > size(&["新建"]).0);
    }

    #[test]
    fn menu_height_counts_rows() {
        let (_, one) = size(&["a"]);
        let (_, three) = size(&["a", "b", "c"]);
        assert!(three > one, "条数多要高一些");
        assert_eq!(three - one, theme::control_h().to_f64() as f32 * 2.0);
        // 空菜单也不 panic（弹出瞬间 items 还没填）
        assert!(size(&[]).1 > 0.0);
    }
}

#[cfg(test)]
mod find_request_tests {
    use super::find_request_mut;
    use treq_core::{Collection, Environment, Group, RequestItem, Workspace};

    fn req(id: &str) -> RequestItem {
        RequestItem {
            id: id.into(),
            name: id.into(),
            method: "GET".into(),
            url: "https://x/".into(),
            params: vec![],
            headers: vec![],
            body: treq_core::Body::default(),
            description: String::new(),
            docs_open: true,
            auth: None,
            order: None,
        }
    }

    fn ws() -> Workspace {
        Workspace {
            collections: vec![Collection {
                id: "c1".into(),
                name: "c1".into(),
                requests: vec![req("flat")],
                groups: vec![Group {
                    id: "g1".into(),
                    parent: None,
                    name: "g1".into(),
                    requests: vec![req("nested")],
                    order: None,
                }],
            }],
            base_env: Environment {
                id: "base".into(),
                name: "Base".into(),
                variables: Default::default(),
            },
            environments: vec![],
        }
    }

    #[test]
    fn renames_requests_inside_groups_too() {
        let mut w = ws();
        // 分组里的请求以前找不到（改名丢），现在必须能找到
        find_request_mut(&mut w, "nested").expect("分组内请求").name = "新名字".into();
        find_request_mut(&mut w, "flat").expect("集合直属请求").name = "外层".into();
        assert_eq!(w.collections[0].groups[0].requests[0].name, "新名字");
        assert_eq!(w.collections[0].requests[0].name, "外层");
        assert!(find_request_mut(&mut w, "没有").is_none());
    }
}
