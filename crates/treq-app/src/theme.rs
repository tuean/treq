//! 全局设计 token：照搬 dsh 的两层色板结构。
//!
//! - **static 色阶**：与主题无关的固定色值（冷/暖中性、品牌蓝、语义色、以及 harbor 的暮光蓝与日落橙）。
//! - **alias 别名层**：`bg-base` / `label-secondary` / `border-l2` … 每套配色各自绑定一组别名。
//!
//! 两套方案：
//! - **[`ThemeScheme::Harbor`]**（默认）**暮光蓝港 · 日落橙辉 · 半透明夜色面板** ——
//!   黄昏海港的暮光蓝做底、日落橙做强调；面板色带 alpha，配合窗口
//!   `WindowBackgroundAppearance::Blurred` 透出底色，成「夜色毛玻璃」。
//! - **[`ThemeScheme::DshLight`]**：dsh 浅色（色值取自 dsh 设计 token 的 alias 层）。
//!
//! 切换靠 [`set_scheme`] 写全局「当前方案」，所有取色函数都从这里读 —— 与字号/行高
//! （`set_fonts` / `set_line_h`）同一套机制，所以改完下一帧全应用生效。
//!
//! 色值出处：dsh 的 `@deepseek-ai/dsh-client-ui-theme` design-platform 样式表
//! （`--dsw-static-*` 色阶 + `body` / `body[data-ds-dark-theme]` 两组 `--dsw-alias-*`）。

use gpui::{Font, Pixels, Rgba, font, px, rgb, rgba};
use std::sync::atomic::{AtomicU8, Ordering};

use crate::settings::ThemeScheme;

// ---- dsh static 色阶（与主题无关）----
/// `--dsw-static-neutral-bluish-*`（冷中性）
mod nb {
    pub const N_50: u32 = 0xf9fafb;
    pub const N_75: u32 = 0xf1f3f5;
    pub const N_100: u32 = 0xebeef2;
    pub const N_150: u32 = 0xe9ecf2;
    pub const N_300: u32 = 0xcfd3d6;
    pub const N_400: u32 = 0xadb2b8;
    pub const N_600: u32 = 0x81858c;
    pub const N_700: u32 = 0x61666b;
    pub const N_750: u32 = 0x43454a;
    pub const N_800: u32 = 0x353638;
    pub const N_875: u32 = 0x232324;
    pub const N_900: u32 = 0x1b1b1c;
    pub const N_950: u32 = 0x151517;
}
/// `--dsw-static-neutral-*`（暖中性）
mod nu {
    pub const N_100: u32 = 0xf5f5f5;
}

/// **harbor** 专用色阶：暮光蓝港的蓝 + 日落橙辉的橙（黄昏海港取色）。
mod harbor {
    // 暮光蓝港：锚点是深海靛蓝 #141829（夜色面板的基色），往上逐级提亮
    pub const CHROME: u32 = 0x0f111d; // 菜单栏 / 顶栏 / 状态栏（整圈外框，最深）
    pub const NIGHT: u32 = 0x0e111c; // 入夜海面（下沉条 / tab 条）
    pub const DEEP: u32 = 0x111524; // 深水（下沉条）
    pub const PANEL: u32 = 0x141829; // 深海靛蓝 —— 主面板基色（锚点）
    pub const HARBOR: u32 = 0x1b2138; // 港湾蓝（浮起面 / 按钮）
    pub const DUSK: u32 = 0x252c47; // 暮色蓝（边框 / 选中）
    pub const MIST: u32 = 0x333c5e; // 海雾蓝（强边框）
    pub const SKY: u32 = 0x8fa0c8; // 天色蓝（次级文字）
    // 日落橙辉：强调色一族
    pub const EMBER_DEEP: u32 = 0xb4531f; // 压暗的余烬橙（用于浅字底）
    pub const SUNSET: u32 = 0xef8b4a; // 日落橙（主强调）
    pub const SUNSET_HI: u32 = 0xffa96b; // 日落橙高亮（hover）
    pub const GOLD: u32 = 0xf6c67a; // 夕照金（文字强调）
    // 海港夜色里的文字
    pub const FOAM: u32 = 0xeaeef8; // 浪花白（最亮文字）
    pub const SAND: u32 = 0xc7cee0; // 沙岸浅灰（正文）
    pub const STEEL: u32 = 0x8d97b4; // 钢灰蓝（次要）
    pub const HAZE: u32 = 0x69738f; // 远雾（行号 / 占位）
    // 港口信号灯
    pub const SIGNAL_GREEN: u32 = 0x5fd39a; // 航道绿
    pub const SIGNAL_RED: u32 = 0xff6b6b; // 警示红
    pub const BUOY_YELLOW: u32 = 0xf2c14e; // 浮标黄
    pub const LIGHTHOUSE: u32 = 0x6fc3ff; // 灯塔蓝（info / link）
}
/// `--dsw-static-deepseek-*`（品牌蓝）
mod ds {
    pub const N_400: u32 = 0x7aaaff;
    pub const N_450: u32 = 0x5686fe;
    pub const N_500: u32 = 0x4176e6;
}
/// `--dsw-static-blue-*` / `green-*` / `red-*` / `amber-*`（语义色）
mod hue {
    pub const BLUE_400: u32 = 0x60a5fa;
    pub const BLUE_900: u32 = 0x0e3074;
    pub const GREEN_400: u32 = 0x4ed17e;
    pub const GREEN_500: u32 = 0x22c55e;
    pub const RED_400: u32 = 0xf25a5a;
    pub const RED_500: u32 = 0xef4444;
    pub const RED_600: u32 = 0xec1313;
    pub const AMBER_500: u32 = 0xf59e0b;
}

/// 一套配色的全部别名（alias 层）。
///
/// 字段按用途分组，叫法与 dsh 对齐（base/layer/border/label/state/specific）。
/// 加新色时**两套方案都要填**（有单测兜底）。
/// 色值可以是 `0xRRGGBB`（不透明）或 `0xRRGGBBAA`（带 alpha）。
struct Palette {
    // 背景
    bg_base: u32,       // alias-bg-base：应用底色
    bg_chrome: u32,     // 菜单栏 / 顶栏 / 状态栏这一圈「框」的底色
    bg_pane: u32,       // bg-layer-1：编辑区 / 响应区
    bg_sunken: u32,     // bg-module-platform：下沉条（URL 行、tab 条）
    bg_field: u32,      // bg-document-preview：内嵌盒
    bg_popup: u32,      // specific-menu：菜单 / 浮层
    bg_button: u32,     // button-elevated-fill：次要按钮 / pill 底
    bg_hover: u32,      // interactive-bg-hover
    bg_selected: u32,   // interactive-bg-hover-solid
    bg_open: u32,       // 下拉框「打开态」底色（比下沉条再亮/再深一档）
    bg_close_hover: u32, // 危险按钮 hover（没用到就与 bg_button 同值）
    bg_success: u32,    // state-success-primary（2xx pill 底）
    // 边框
    border: u32,        // border-l2
    border_strong: u32, // border-l3
    border_input: u32,  // border-l4
    // 文字
    fg_dark: u32,     // label-caption：行号 / 占位
    fg_dim: u32,      // label-tertiary：次要
    fg_normal: u32,   // label-secondary：正文
    fg_bright: u32,   // label-primary：强调正文
    fg_white: u32,    // 纯白（品牌底上的字）
    // 品牌 / 语义
    primary: u32,  // brand-primary（焦点、选中条）
    accent: u32,   // button-primary-fill（主按钮底）
    accent_hi: u32, // button-primary-hover
    accent_deep: u32, // 强调色的暗端（渐变 / 压深）
    on_accent: u32,   // 强调底上的文字色
    blue: u32,     // link
    green: u32,    // state-success-secondary（成功文字）
    red: u32,      // state-error-primary
    orange: u32,   // state-warn-primary
    // JSON 语法
    json_key: u32,
    json_string: u32,
    json_num: u32,
    json_const: u32,
    // HTTP 方法
    m_get: u32,
    m_post: u32,
    m_put: u32,
    m_patch: u32,
    m_delete: u32,
    m_other: u32,
}

impl Palette {
    /// `0xRRGGBB` 或 `0xRRGGBBAA` → gpui 颜色（alpha 缺省按不透明）。
    fn c(&self, v: u32) -> Rgba {
        if v > 0x00ff_ffff {
            rgba(v)
        } else {
            rgb(v)
        }
    }
    /// 给某个色值换上指定 alpha（`0..=255`），用来在同一色相上做半透明变体。
    fn a(&self, v: u32, alpha: u32) -> Rgba {
        rgba((v << 8) | alpha)
    }
}

/// **harbor**：暮光蓝港 · 日落橙辉 · 半透明夜色面板（默认方案）。
///
/// - 底：黄昏海面的暮光蓝，越靠「面」越亮（night → deep → twilight → harbor）。
/// - 强调：日落橙（主按钮、焦点、选中条），hover 提亮一档。
/// - 面板：`0xRRGGBBAA` 带 alpha，配窗口毛玻璃就是夜色玻璃面板。
const HARBOR: Palette = Palette {
    // 背景：**主面不透明**，色号所见即所得（锚点 #141829）；毛玻璃只留给浮起面
    bg_base: harbor::PANEL,      // 应用底：深海靛蓝 #141829
    bg_chrome: harbor::CHROME,   // 菜单栏 / 顶栏 / 状态栏：#0F111D
    bg_pane: harbor::PANEL,      // 编辑区 / 响应区：同一块面板
    bg_sunken: harbor::NIGHT,    // 下沉条（URL 行 / tab 条）比面板深一档
    bg_field: harbor::DEEP,      // 内嵌盒
    bg_popup: 0x1b2138fa,        // 浮层 / 菜单：98% —— 这里保留毛玻璃的透
    bg_button: harbor::HARBOR,   // 按钮 / pill 底（港湾蓝）
    bg_hover: 0xffffff14,      // 悬停：白色微光叠加
    bg_selected: 0xffffff1f,   // 选中行
    bg_open: 0xffffff24,       // 下拉框打开态
    bg_close_hover: 0xd14040f2, // 危险 hover：警示红
    bg_success: harbor::SIGNAL_GREEN,
    // 边框：靛蓝同族，不用白灰
    border: 0x2b3350e6,
    border_strong: 0x3a4468f0,
    border_input: 0xffffff24,
    // 文字：浪花白 → 沙岸 → 钢灰蓝 → 远雾
    fg_dark: harbor::HAZE,    // 行号 / 占位
    fg_dim: harbor::STEEL,    // 次要
    fg_normal: harbor::SAND,  // 正文
    fg_bright: harbor::FOAM,  // 强调正文
    fg_white: 0xffffff,
    // 品牌 / 语义：日落橙为主强调
    primary: harbor::SUNSET,
    accent: harbor::SUNSET,
    accent_hi: harbor::SUNSET_HI,
    accent_deep: harbor::EMBER_DEEP,
    on_accent: 0x2b1206, // 橙底上用深棕近黑字，对比比白字更稳
    blue: harbor::LIGHTHOUSE,
    green: harbor::SIGNAL_GREEN,
    red: harbor::SIGNAL_RED,
    orange: harbor::BUOY_YELLOW,
    // JSON：深靛底上的语法色（键=天色蓝、字符串=航道绿、数字=夕照金、常量=日落橙）
    json_key: harbor::SKY,
    json_string: harbor::SIGNAL_GREEN,
    json_num: harbor::GOLD,
    json_const: harbor::SUNSET,
    // 方法
    m_get: harbor::SUNSET,
    m_post: harbor::SIGNAL_GREEN,
    m_put: harbor::BUOY_YELLOW,
    m_patch: harbor::GOLD,
    m_delete: harbor::SIGNAL_RED,
    m_other: harbor::LIGHTHOUSE,
};

/// **dsh 浅色**：`body` 的 alias 层。
const LIGHT: Palette = Palette {
    // 背景：base 是白，下沉条/内嵌盒用极浅冷灰
    bg_base: 0xffffff,
    bg_chrome: nu::N_100,
    bg_pane: 0xffffff,
    bg_sunken: nb::N_50,
    bg_field: nb::N_75,
    bg_popup: 0xf8f9fa,
    bg_button: nu::N_100,
    bg_hover: nb::N_100,
    bg_selected: nb::N_150,
    bg_open: nb::N_100,
    bg_close_hover: 0xfdece9,
    bg_success: hue::GREEN_500,
    // 边框：黑色低透明叠加
    border: 0x0000001a,
    border_strong: 0x0000001f,
    border_input: 0x00000029,
    // 文字
    fg_dark: nb::N_400,   // label-caption
    fg_dim: nb::N_600,    // label-tertiary
    fg_normal: nb::N_700, // label-secondary
    fg_bright: nb::N_900,
    fg_white: 0xffffff,
    // 品牌 / 语义
    primary: hue::BLUE_900, // 浅色下用深蓝，白底才读得清
    accent: ds::N_500,
    accent_hi: ds::N_450,
    accent_deep: 0x2f4c8f,
    on_accent: 0xffffff,
    blue: ds::N_500,
    green: hue::GREEN_500,
    red: hue::RED_600,
    orange: hue::AMBER_500,
    // JSON
    json_key: 0x0550ae,
    json_string: 0x0a7d33,
    json_num: 0x953800,
    json_const: 0x6f42c1,
    // 方法
    m_get: 0x6741d9,
    m_post: 0x1f883d,
    m_put: 0xb35c00,
    m_patch: 0x8a6d00,
    m_delete: hue::RED_600,
    m_other: ds::N_500,
};

/// 当前方案（原子量存枚举下标；缺省深色 = dsh 默认）。
static SCHEME: AtomicU8 = AtomicU8::new(ThemeScheme::Harbor as u8);

fn palette_of(s: ThemeScheme) -> &'static Palette {
    match s {
        ThemeScheme::Harbor => &HARBOR,
        ThemeScheme::DshLight => &LIGHT,
    }
}

/// 当前配色方案。
pub fn scheme() -> ThemeScheme {
    match SCHEME.load(Ordering::Relaxed) {
        1 => ThemeScheme::DshLight,
        _ => ThemeScheme::Harbor,
    }
}

/// 换配色方案（`AppModel` 启动时与配置页改动时调用）。下一帧所有取色即生效。
pub fn set_scheme(s: ThemeScheme) {
    SCHEME.store(s as u8, Ordering::Relaxed);
}

/// 当前方案（取色用；下面每个函数都走它）。
fn p() -> &'static Palette {
    palette_of(scheme())
}

// ---- 背景（分区靠 1px 边框与明度差）----
/// 应用底色：活动栏 / 侧栏 / 顶栏 / 状态栏
pub fn bg_base() -> Rgba {
    p().c(p().bg_base)
}
/// 菜单栏 / 顶栏 / 状态栏这一圈「框」的底色
pub fn bg_chrome() -> Rgba {
    p().c(p().bg_chrome)
}
/// 请求编辑区与响应区
pub fn bg_pane() -> Rgba {
    p().c(p().bg_pane)
}
/// 下沉条：URL 输入行、tab 条、深色 pill
pub fn bg_sunken() -> Rgba {
    p().c(p().bg_sunken)
}
/// 内嵌盒：URL PREVIEW、弹窗内代码框
pub fn bg_field() -> Rgba {
    p().c(p().bg_field)
}
/// 弹窗 / 菜单 / 浮动层
pub fn bg_popup() -> Rgba {
    p().c(p().bg_popup)
}
/// 次要按钮 / pill 底
pub fn bg_button() -> Rgba {
    p().c(p().bg_button)
}
/// hover
pub fn bg_hover() -> Rgba {
    p().c(p().bg_hover)
}
/// 选中态（列表行）
pub fn bg_selected() -> Rgba {
    p().c(p().bg_selected)
}
/// 下拉框「打开态」底色（比下沉条再亮/再深一档）
pub fn bg_open() -> Rgba {
    p().c(p().bg_open)
}
/// 命令面板选中行背景（品牌色 15%）
pub fn bg_selected_accent() -> Rgba {
    p().a(p().accent, 0x26)
}
/// 输入框背景（透明，靠边框/下划线成盒）
pub fn bg_input() -> gpui::Hsla {
    gpui::transparent_black()
}
/// 主按钮（品牌蓝）
pub fn bg_accent() -> Rgba {
    p().c(p().accent)
}
/// 主按钮 hover
pub fn bg_accent_hover() -> Rgba {
    p().c(p().accent_hi)
}
/// 2xx 状态 pill 底（配白字）
pub fn bg_success() -> Rgba {
    p().c(p().bg_success)
}
/// 危险按钮 hover 底色
pub fn bg_close_hover() -> Rgba {
    p().c(p().bg_close_hover)
}
/// 弹窗遮罩（夜色下用更深的蓝黑，别把玻璃感压灰）
pub fn bg_backdrop() -> Rgba {
    match scheme() {
        ThemeScheme::Harbor => rgba(0x05090fcc),
        ThemeScheme::DshLight => rgba(0x00000066),
    }
}
/// 文本选中高亮（强调色低透明）
pub fn selection() -> Rgba {
    match scheme() {
        ThemeScheme::Harbor => rgba(0xef8b4a59),
        ThemeScheme::DshLight => rgba(0x3b82f659),
    }
}

// ---- 边框 ----
/// 分区线（面板之间）
pub fn border() -> Rgba {
    p().c(p().border)
}
/// 强边框（输入框、pill、卡片）
pub fn border_strong() -> Rgba {
    p().c(p().border_strong)
}
/// 弱边框（行下划线、内嵌框）
pub fn border_input() -> Rgba {
    p().c(p().border_input)
}

// ---- 文字 ----
/// 行号、占位
pub fn fg_dark() -> Rgba {
    p().c(p().fg_dark)
}
/// 次要（列表名、tab 未激活）
pub fn fg_dim() -> Rgba {
    p().c(p().fg_dim)
}
/// 正文
pub fn fg_normal() -> Rgba {
    p().c(p().fg_normal)
}
pub fn fg_bright() -> Rgba {
    p().c(p().fg_bright)
}
pub fn fg_white() -> Rgba {
    p().c(p().fg_white)
}
/// 滚动条滑块（压在内容上的半透明，深浅底都看得见）
pub fn scroll_thumb() -> Rgba {
    match scheme() {
        ThemeScheme::Harbor => rgba(0xa8c4dd59),
        ThemeScheme::DshLight => rgba(0x00000040),
    }
}
/// 主按钮上的文字（强调底上的字）
pub fn fg_on_accent() -> Rgba {
    p().c(p().on_accent)
}
/// 强调色的暗端（渐变 / 压深用）
pub fn accent_deep() -> Rgba {
    p().c(p().accent_deep)
}

// ---- 语义色 ----
/// 品牌色：焦点、选中条、拖动条
pub fn primary() -> Rgba {
    p().c(p().primary)
}
/// info
pub fn blue() -> Rgba {
    p().c(p().blue)
}
/// success 文字
pub fn green() -> Rgba {
    p().c(p().green)
}
/// danger
pub fn red() -> Rgba {
    p().c(p().red)
}
/// warning
pub fn orange() -> Rgba {
    p().c(p().orange)
}
pub fn link_blue() -> Rgba {
    p().c(p().blue)
}

/// JSON 语法色：键 / 字符串 / 数字 / 常量（true/false/null）
pub fn json_key() -> Rgba {
    p().c(p().json_key)
}
pub fn json_string() -> Rgba {
    p().c(p().json_string)
}
pub fn json_num() -> Rgba {
    p().c(p().json_num)
}
pub fn json_const() -> Rgba {
    p().c(p().json_const)
}

/// HTTP 方法颜色（GET/POST/PUT/PATCH/DELETE 各一色，其余归 info）
pub fn method_color(method: &str) -> Rgba {
    let pal = p();
    pal.c(match method.to_uppercase().as_str() {
        "GET" => pal.m_get,
        "POST" => pal.m_post,
        "PUT" => pal.m_put,
        "PATCH" => pal.m_patch,
        "DELETE" => pal.m_delete,
        _ => pal.m_other,
    })
}

// ---- 排版 ----
// 字体家族/字号由配置页驱动（settings 的 font_* 字段），改完立即生效：
// `font_body` / `mono_size` 是「当前值」，`set_fonts()` 负责写进来。
/// 代码字体家族默认值
pub const DEFAULT_MONO_FAMILY: &str = "Menlo";
pub const DEFAULT_UI_SIZE: f32 = 13.;
pub const DEFAULT_MONO_SIZE: f32 = 12.;

static UI_FAMILY: std::sync::LazyLock<std::sync::Mutex<String>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(String::new()));
static MONO_FAMILY: std::sync::LazyLock<std::sync::Mutex<String>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(DEFAULT_MONO_FAMILY.to_string()));
/// 字号存成 ×100 的整数（原子量只有整数）
static UI_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1300);
static MONO_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1200);

fn read_size(v: &std::sync::atomic::AtomicU32) -> f32 {
    v.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.
}

fn store_size(v: &std::sync::atomic::AtomicU32, size: f32) {
    let clamped = size.clamp(6., 48.);
    v.store((clamped * 100.).round() as u32, std::sync::atomic::Ordering::Relaxed);
}

/// 换字体设置（`AppModel` 启动时与配置页改动时调用）。
pub fn set_fonts(ui_family: &str, ui_size: f32, mono_family: &str, mono_size: f32) {
    let mut fam = UI_FAMILY.lock().unwrap();
    if *fam != ui_family {
        *fam = ui_family.to_string();
    }
    drop(fam);
    let mut fam = MONO_FAMILY.lock().unwrap();
    if *fam != mono_family {
        *fam = mono_family.to_string();
    }
    store_size(&UI_SIZE, ui_size);
    store_size(&MONO_SIZE, mono_size);
}

/// 界面字体。没配就返回 None（＝保持 gpui 的系统默认字体，别去改默认观感）
pub fn ui_font() -> Option<Font> {
    let fam = UI_FAMILY.lock().unwrap().clone();
    (!fam.trim().is_empty()).then(|| font(fam))
}

/// 正文/控件字号（Insomnia 默认 13px 的密排 UI）
pub fn font_body() -> f32 {
    read_size(&UI_SIZE)
}
/// 次级/元信息字号（比正文小 2px）
pub fn font_small() -> f32 {
    (font_body() - 2.).max(6.)
}
/// 等宽字体（JSON / 代码）
pub fn mono() -> Font {
    font(MONO_FAMILY.lock().unwrap().clone())
}
/// JSON / 代码的字号
pub fn mono_size() -> f32 {
    read_size(&MONO_SIZE)
}

// ---- 间距（黄金比例级联）----
// 3 / 5 / 8 / 13 / 21 / 34：相邻两级比 ≈ φ（斐波那契数列就是 φ 的有理逼近），
// 整数值不产生半像素，比 4/8/12/16 的等差台阶更紧凑。
// 用法：sp1 图标与文字、sp2 行内元素、sp3 控件之间、sp4 面板内边距、sp5 区块之间、sp6 大分区。
/// 3（微间距：图标/勾选框与其标签）
pub fn sp1() -> Pixels {
    px(3.)
}
/// 5（行内元素间距）
pub fn sp2() -> Pixels {
    px(5.)
}
/// 8（控件之间的常规间距）
pub fn sp3() -> Pixels {
    px(8.)
}
/// 13（面板内边距、缩进一级）
pub fn sp4() -> Pixels {
    px(13.)
}
/// 21（区块之间、树缩进二级）
pub fn sp5() -> Pixels {
    px(21.)
}
/// 34（大分区、空状态留白）
pub fn sp6() -> Pixels {
    px(34.)
}

/// 侧栏每行缩进：集合 0 → 8px，第一层分组 13px，之后每层 +8px。
/// （请求的 depth 是「所属分组 + 1」，所以集合直属请求 = 第二档 21px，跟以前一致。）
pub fn tree_indent(depth: u8) -> Pixels {
    match depth {
        0 => sp3(),
        1 => sp4(),
        d => px(13. + 8. * f32::from(d - 1)),
    }
}

// ---- 尺寸（高度同样按 φ 级联：26 = 21+5、34 = 21+13、39 = 34+5）----
/// 控件/行统一高度（按钮、输入框、列表行、pill 基线）
pub fn control_h() -> Pixels {
    px(26.)
}
/// kv 行高（26 + sp1，两行之间留一条发丝缝）。表格是「组件」不是正文，
/// 高度固定，不跟 [`line_h`]（那个只管 JSON/正文内容）
pub fn kv_row_h() -> Pixels {
    px(29.)
}
/// 列表行高（树/历史行：贴着控件高，不再单独留大行距）
pub fn row_h() -> Pixels {
    px(26.)
}
/// 左侧活动栏宽度
pub const RAIL_W: f32 = 48.0;
/// 顶栏高度
pub fn header_h() -> Pixels {
    px(34.)
}
/// 请求工具条高度（method + url + send + 状态 pill）
pub fn toolbar_h() -> Pixels {
    px(39.)
}
/// tab 条高度
pub fn tab_h() -> Pixels {
    px(34.)
}
/// 面板底部链接行高度
pub fn footer_h() -> Pixels {
    px(29.)
}
/// 窗口底部状态栏高度
pub fn statusbar_h() -> Pixels {
    px(26.)
}
/// 三栏默认宽度（可拖动并在 settings 持久化）：树 260 / 响应 520 / 编辑区 flex
pub const DEFAULT_TREE_W: f32 = 260.0;
pub const DEFAULT_PANEL_W: f32 = 520.0;
/// 发送按钮宽度：高度（工具条高减掉 1px 下边框）× 黄金比例 1.618。
/// 定宽而不是靠 padding 撑，中英文（发送/Send）长得一样宽。
pub fn send_btn_w() -> Pixels {
    px((f32::from(toolbar_h()) - 1.) * 1.618)
}

/// 正文/JSON 行高。**唯一真源**：请求体 JSON 编辑器、kv 表格行、响应正文与
/// 各内容列表都读它，保证「接口返回值的 JSON」和「Query/Body 编辑器」行高一致。
/// 值由设置里的「内容行高」驱动（`settings::editor_line_h`），启动和改动时调用 [`set_line_h`]。
pub fn line_h() -> f32 {
    LINE_H.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.
}

/// 设置「内容行高」（px，调用方已夹取范围）。
pub fn set_line_h(v: f32) {
    LINE_H.store((v * 100.).round() as u32, std::sync::atomic::Ordering::Relaxed);
}

static LINE_H: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1600);
/// 单行输入框的文本行高（也是光标高度）
pub fn input_line_h() -> f32 {
    18.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 取某套方案的某个色（**不动全局状态**：全局是进程级的，并行测试会互相踩）。
    fn of(s: ThemeScheme) -> &'static Palette {
        palette_of(s)
    }

    /// 全局方案是**进程级**状态，而 `cargo test` 多线程并行跑同一进程里的用例 ——
    /// 任何会读/写全局方案的用例都必须先拿这把锁，否则互相踩（曾出现过时好时坏）。
    static SCHEME_LOCK: Mutex<()> = Mutex::new(());

    fn scheme_guard() -> std::sync::MutexGuard<'static, ()> {
        SCHEME_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 临时切全局方案（只有确实要验证「全局切换」的用例才用它，跑完还原）。
    fn with_scheme<T>(s: ThemeScheme, f: impl FnOnce() -> T) -> T {
        let _guard = scheme_guard();
        let old = scheme();
        set_scheme(s);
        let out = f();
        set_scheme(old);
        out
    }

    /// 相对亮度（sRGB 加权），只用来比较明暗。
    fn luma(c: Rgba) -> f32 {
        0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
    }

    #[test]
    fn default_scheme_is_harbor() {
        assert_eq!(ThemeScheme::default(), ThemeScheme::Harbor);
    }

    #[test]
    fn the_two_palettes_differ_and_flip_contrast() {
        let (h, l) = (of(ThemeScheme::Harbor), of(ThemeScheme::DshLight));
        assert_ne!(h.bg_base, l.bg_base, "两套底色必须不同");
        assert_ne!(h.fg_normal, l.fg_normal, "两套正文色必须不同");
        assert_ne!(h.json_key, l.json_key, "两套 JSON 键色必须不同");
        // harbor 是深底浅字，dsh 浅色是浅底深字：对比方向相反
        assert!(luma(h.c(h.bg_base)) < luma(l.c(l.bg_base)));
        assert!(luma(h.c(h.fg_normal)) > luma(h.c(h.bg_base)));
        assert!(luma(l.c(l.fg_normal)) < luma(l.c(l.bg_base)));
    }

    #[test]
    fn harbor_is_a_twilight_blue_harbor_with_sunset_accent() {
        let h = of(ThemeScheme::Harbor);
        // 底子是蓝：蓝通道最大，且整体很暗（夜色）
        for (name, v) in [("bg_base", h.bg_base), ("bg_pane", h.bg_pane)] {
            let c = h.c(v);
            assert!(c.b > c.r && c.b > c.g, "{name} 应当是暮光蓝");
            assert!(luma(c) < 0.30, "{name} 应当是夜色（暗）");
        }
        // 主面板色号落在 #141829 附近（用户指定的锚点色）：靛蓝、偏暗、蓝通道最大
        let panel = h.c(h.bg_pane);
        let (r, g, b) = (
            (panel.r * 255.).round() as u32,
            (panel.g * 255.).round() as u32,
            (panel.b * 255.).round() as u32,
        );
        let d = |a: u32, b: u32| a.abs_diff(b);
        assert!(
            d(r, 0x14) <= 6 && d(g, 0x18) <= 6 && d(b, 0x29) <= 6,
            "主面板应当接近 #141829，实际 #{r:02x}{g:02x}{b:02x}"
        );
        // 强调是日落橙：红通道最大
        let acc = h.c(h.accent);
        assert!(acc.r > acc.g && acc.g > acc.b, "accent 应当是日落橙");
        // 菜单栏 / 顶栏 / 状态栏 = #0F111D，且比主面板更深，形成外框层次
        let chrome = h.c(h.bg_chrome);
        let (cr, cg, cb) = (
            (chrome.r * 255.).round() as u32,
            (chrome.g * 255.).round() as u32,
            (chrome.b * 255.).round() as u32,
        );
        assert!(
            d(cr, 0x0f) <= 4 && d(cg, 0x11) <= 4 && d(cb, 0x1d) <= 4,
            "菜单栏底应当是 #0F111D，实际 #{cr:02x}{cg:02x}{cb:02x}"
        );
        assert!(luma(chrome) < luma(panel), "菜单栏应当比主面板更深");
        // 主面板不透明（色号所见即所得），毛玻璃只留给浮起面
        assert_eq!(h.c(h.bg_pane).a, 1.0, "主面板应当是不透明实色");
        assert!(h.c(h.bg_popup).a < 1.0, "浮层应当带 alpha（夜色玻璃）");
    }

    #[test]
    fn both_palettes_bind_every_alias_to_a_solid_color() {
        // 两套方案都得给出实色，防止以后加色时漏填一边
        for s in ThemeScheme::ALL {
            let _ = with_scheme(s, || {
                // 文字/边框必须是实色；面板色在 harbor 下允许带 alpha（夜色玻璃）
                // 文字 / 语义 / 语法色：必须是不透明实色，否则叠在面板上看不清
                // （边框与面板允许带 alpha —— harbor 的夜色玻璃就靠它）
                let solid: [(&str, Rgba); 10] = [
                    ("fg_dark", fg_dark()),
                    ("fg_dim", fg_dim()),
                    ("fg_normal", fg_normal()),
                    ("fg_bright", fg_bright()),
                    ("primary", primary()),
                    ("green", green()),
                    ("red", red()),
                    ("orange", orange()),
                    ("json_key", json_key()),
                    ("json_string", json_string()),
                ];
                for (name, c) in solid {
                    assert_eq!(c.a, 1.0, "{s:?} 的 {name} 应当是实色");
                }
                // 面板色：alpha 必须有效（>0），且文字色不透明
                for (name, c) in [
                    ("bg_base", bg_base()),
                    ("bg_pane", bg_pane()),
                    ("bg_sunken", bg_sunken()),
                    ("bg_popup", bg_popup()),
                    ("bg_button", bg_button()),
                    ("bg_accent", bg_accent()),
                ] {
                    assert!(c.a > 0.0 && c.a <= 1.0, "{s:?} 的 {name} alpha 不合法：{}", c.a);
                }
                assert_eq!(fg_on_accent().a, 1.0, "{s:?} 的主按钮文字应当是实色");
                for m in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] {
                    assert_eq!(method_color(m).a, 1.0, "{s:?} 的 {m} 应当是实色");
                }
            });
        }
    }

    #[test]
    fn text_stays_readable_on_its_background() {
        // 两套方案里的关键前景/背景组合：亮度差得够开（粗判，不做 WCAG 精算）
        for s in ThemeScheme::ALL {
            with_scheme(s, || {
                let pairs = [
                    ("fg_normal/bg_pane", fg_normal(), bg_pane()),
                    ("fg_normal/bg_base", fg_normal(), bg_base()),
                    ("fg_dim/bg_base", fg_dim(), bg_base()),
                    ("fg_dark/bg_pane", fg_dark(), bg_pane()),
                    ("fg_bright/bg_popup", fg_bright(), bg_popup()),
                ];
                for (name, fg, bg) in pairs {
                    let d = (luma(fg) - luma(bg)).abs();
                    assert!(d > 0.12, "{s:?} 的 {name} 对比不足：ΔL={d:.3}");
                }
            });
        }
    }

    #[test]
    fn scheme_round_trips_through_the_atomic() {
        // 注意：`with_scheme` 内部已经持锁，这里**不能**再拿一次（会自锁）
        with_scheme(ThemeScheme::DshLight, || {
            assert_eq!(scheme(), ThemeScheme::DshLight);
            assert_eq!(bg_base(), LIGHT.c(LIGHT.bg_base), "浅色应取 LIGHT 的底色");
            assert_eq!(p().bg_base, LIGHT.bg_base);
        });
        with_scheme(ThemeScheme::Harbor, || {
            assert_eq!(scheme(), ThemeScheme::Harbor);
            assert_eq!(bg_base(), HARBOR.c(HARBOR.bg_base), "harbor 应取 HARBOR 的底色");
            assert_eq!(p().bg_base, HARBOR.bg_base);
        });
    }
}
