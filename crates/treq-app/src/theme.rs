//! 全局设计 token：仿 Insomnia 暗色主题（Insomnia/renderer 默认深色变体）。
//! 色值来自参考截图逐点取色（1364×884）：
//! 面板 #2c2c2c / 编辑器与响应区 #2a2a2a / 输入条与 tab 条 #212121 /
//! URL PREVIEW 盒 #313131 / 次要按钮与 pill #3b3b3b /
//! 品牌紫（Send）#8776d5 / 2xx pill #75ba24 /
//! 正文 #e0e0e0、次要 #999999、行号 #717171 /
//! JSON：键 #7ecf2b、字符串 #f0e137、数字 #e0e0e0、常量 #a594fb /
//! 方法：GET #a594fb、POST #7ecf2b。

use gpui::{Font, Pixels, Rgba, font, px, rgb, rgba};

// ---- 背景（分区靠 1px 边框与明度差）----
/// 应用底色：活动栏 / 侧栏 / 顶栏 / 状态栏
pub fn bg_base() -> Rgba {
    rgb(0x2c2c2c)
}
/// 请求编辑区与响应区（比侧栏略深一档）
pub fn bg_pane() -> Rgba {
    rgb(0x2a2a2a)
}
/// 下沉条：URL 输入行、tab 条、深色 pill
pub fn bg_sunken() -> Rgba {
    rgb(0x212121)
}
/// 内嵌盒：URL PREVIEW、弹窗内代码框
pub fn bg_field() -> Rgba {
    rgb(0x313131)
}
/// 弹窗 / 菜单 / 浮动层
pub fn bg_popup() -> Rgba {
    rgb(0x313131)
}
/// 次要按钮 / pill 底
pub fn bg_button() -> Rgba {
    rgb(0x3b3b3b)
}
/// hover
pub fn bg_hover() -> Rgba {
    rgb(0x363636)
}
/// 选中态（列表行）
pub fn bg_selected() -> Rgba {
    rgb(0x3a3a3a)
}
/// 命令面板选中行背景（品牌紫 15%）
pub fn bg_selected_accent() -> Rgba {
    rgba(0x8776d526)
}
/// 输入框背景（透明，靠边框/下划线成盒）
pub fn bg_input() -> gpui::Hsla {
    gpui::transparent_black()
}
/// 主按钮（品牌紫，与 Insomnia Send 同色）
pub fn bg_accent() -> Rgba {
    rgb(0x8776d5)
}
/// 主按钮 hover
pub fn bg_accent_hover() -> Rgba {
    rgb(0x9a8be0)
}
/// 2xx 状态 pill 底（配白字）
pub fn bg_success() -> Rgba {
    rgb(0x75ba24)
}
/// 弹窗遮罩
pub fn bg_backdrop() -> Rgba {
    rgba(0x0A0A16CC)
}
/// 文本选中高亮（品牌紫低透明）
pub fn selection() -> Rgba {
    rgba(0x8776d559)
}

// ---- 边框 ----
/// 分区线（面板之间）
pub fn border() -> Rgba {
    rgb(0x3a3a3a)
}
/// 强边框（输入框、pill、卡片）
pub fn border_strong() -> Rgba {
    rgb(0x464646)
}
/// 弱边框（行下划线、内嵌框）
pub fn border_input() -> Rgba {
    rgb(0x3f3f3f)
}

// ---- 文字 ----
pub fn fg_dark() -> Rgba {
    rgb(0x717171)
} // 行号、占位
pub fn fg_dim() -> Rgba {
    rgb(0x999999)
} // 次要（列表名、tab 未激活）
pub fn fg_normal() -> Rgba {
    rgb(0xe0e0e0)
} // 正文
pub fn fg_bright() -> Rgba {
    rgb(0xf5f5f5)
}
pub fn fg_white() -> Rgba {
    rgb(0xffffff)
}
/// 滚动条滑块（压在内容上的半透明白，深浅底都看得见）
pub fn scroll_thumb() -> Rgba {
    rgba(0xffffff45)
}

/// 主按钮上的文字（紫底白字）
pub fn fg_on_accent() -> Rgba {
    rgb(0xffffff)
}

// ---- 语义色 ----
pub fn primary() -> Rgba {
    rgb(0x8776d5)
} // 品牌紫：焦点、选中条、拖动条
pub fn blue() -> Rgba {
    rgb(0x6ca0f5)
} // info
pub fn green() -> Rgba {
    rgb(0x7ecf2b)
} // success 文字
pub fn red() -> Rgba {
    rgb(0xe14b4b)
} // danger
pub fn orange() -> Rgba {
    rgb(0xe0a33e)
} // warning
pub fn link_blue() -> Rgba {
    rgb(0x6ca0f5)
}

/// JSON 语法色（截图：键绿、字符串黄、数字浅灰、常量紫）
pub fn json_key() -> Rgba {
    rgb(0x7ecf2b)
}
pub fn json_string() -> Rgba {
    rgb(0xf0e137)
}
pub fn json_num() -> Rgba {
    rgb(0xe0e0e0)
}
pub fn json_const() -> Rgba {
    rgb(0xa594fb)
}

/// HTTP 方法颜色（截图取色：GET 紫、POST 绿；其余按同族补）
pub fn method_color(method: &str) -> Rgba {
    match method.to_uppercase().as_str() {
        "GET" => rgb(0xa594fb),
        "POST" => rgb(0x7ecf2b),
        "PUT" => rgb(0xe0a33e),
        "PATCH" => rgb(0xf0e137),
        "DELETE" => rgb(0xe14b4b),
        _ => rgb(0x6ca0f5),
    }
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
