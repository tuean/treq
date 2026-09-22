#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    Zh,
    En,
}

impl Locale {
    pub fn from_str(s: &str) -> Self {
        if s == "en" { Locale::En } else { Locale::Zh }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Locale::Zh => "zh",
            Locale::En => "en",
        }
    }
}

/// 全部 UI 文案。新增文案必须加到两个分支（`i18n::tests` 会检查两份表键集合一致，
/// 以及代码里 `t("…")` 用到的键都真的存在 —— 拼错的键在界面上会显示成 `?`）。
pub fn tr(locale: Locale, key: &str) -> &'static str {
    match (locale, key) {
        (Locale::Zh, "app.name") => "treq",
        (Locale::Zh, "menu.file") => "文件",
        (Locale::Zh, "action.choose_workspace") => "切换工作区…",
        (Locale::Zh, "action.reload") => "刷新",
        (Locale::Zh, "action.language") => "语言：English",
        (Locale::Zh, "side.search") => "Filter",
        (Locale::Zh, "side.base_environment") => "Base Environment",
        (Locale::Zh, "side.env_vars") => "环境变量",
        (Locale::Zh, "action.quick_panel") => "命令面板",
        (Locale::Zh, "workspace.add") => "添加工作区（项目）…",
        (Locale::Zh, "workspace.remove") => "从列表移除当前工作区",
        (Locale::Zh, "quick.hint_all_ws") => "跨工作区搜索：输入名字 / URL / 方法",
        (Locale::Zh, "action.preferences") => "Preferences",
        (Locale::Zh, "editor.tab.query") => "Query",
        (Locale::Zh, "editor.section.url_preview") => "URL PREVIEW",
        (Locale::Zh, "editor.section.docs") => "DOCS",
        (Locale::Zh, "editor.body.form_hint") => "表单体：a=1&b=2",
        (Locale::Zh, "editor.kv.add") => "Add",
        (Locale::Zh, "editor.kv.delete_all") => "Delete All",
        (Locale::Zh, "editor.kv.toggle_desc") => "Toggle Description",
        (Locale::Zh, "editor.footer.import") => "从 cURL 导入",
        (Locale::Zh, "editor.footer.codegen") => "生成代码",
        (Locale::Zh, "docs.placeholder") => "写点请求说明（Markdown）…",
        (Locale::Zh, "response.tab.preview") => "Preview",
        (Locale::Zh, "response.tab.cookies") => "Cookies",
        (Locale::Zh, "response.tab.timeline") => "Timeline",
        (Locale::Zh, "response.view.pretty") => "格式化",
        (Locale::Zh, "response.view.raw") => "原始",
        (Locale::Zh, "response.none") => "还没有响应，按 Send 发送",
        (Locale::Zh, "response.cookies_empty") => "响应没有 Set-Cookie",
        (Locale::Zh, "response.status") => "状态",
        (Locale::Zh, "response.timeline.total") => "总耗时",
        (Locale::Zh, "response.filter.placeholder") => "$.store.books[*].author",
        (Locale::Zh, "response.filter.not_json") => "响应不是 JSON，无法按路径过滤",
        (Locale::Zh, "response.filter.bad") => "路径不合法",
        (Locale::Zh, "response.filter.empty") => "路径没有匹配到任何值",
        (Locale::Zh, "history.min_ago") => " 分钟前",
        (Locale::Zh, "history.hour_ago") => " 小时前",
        (Locale::Zh, "history.day_ago") => " 天前",
        (Locale::Zh, "response.saved") => "已保存到：",
        (Locale::Zh, "response.save_failed") => "保存失败：",
        (Locale::Zh, "response.copied") => "已复制响应正文 · ",
        (Locale::Zh, "response.truncated") => "响应过大，界面只保留前 ",
        (Locale::Zh, "response.too_big") => "响应过大（超过 2 MB），已跳过渲染",
        (Locale::Zh, "response.too_big_hint") => "点右上「完整」加载",
        (Locale::Zh, "response.view.full") => "完整",
        (Locale::Zh, "backup.title") => "备份与恢复",
        (Locale::Zh, "backup.dir") => "备份目录",
        (Locale::Zh, "backup.now") => "立即备份",
        (Locale::Zh, "backup.empty") => "还没有备份",
        (Locale::Zh, "backup.list_hint") => "备份文件（新 → 旧），点选后恢复",
        (Locale::Zh, "backup.restore_merge") => "合并到当前",
        (Locale::Zh, "backup.restore_overwrite") => "用备份覆盖",
        (Locale::Zh, "backup.merge_hint") => "合并：只补回缺失或改动过的文件，现有接口保留",
        (Locale::Zh, "backup.overwrite_hint") => {
            "覆盖：先用备份完全替换当前工作区（会先自动留一份现场）"
        }
        (Locale::Zh, "backup.safety") => "现场备份",
        (Locale::Zh, "backup.collections") => "集合",
        (Locale::Zh, "backup.requests") => "接口",
        (Locale::Zh, "backup.done_now") => "已备份：",
        (Locale::Zh, "backup.done_merge") => "已合并：",
        (Locale::Zh, "backup.done_overwrite") => "已覆盖：",
        (Locale::Zh, "backup.failed") => "备份失败：",
        (Locale::Zh, "backup.restore_failed") => "恢复失败：",
        (Locale::Zh, "app.tagline") => "treq — 本地优先的 API 客户端",
        (Locale::Zh, "quick.placeholder") => "搜索请求或历史… (Enter 选择 / Esc 关闭)",
        (Locale::Zh, "quick.empty") => "没有匹配的请求或历史",
        (Locale::Zh, "side.environment") => "环境",
        (Locale::Zh, "side.no_collections") => "（空）右键新建集合",
        (Locale::Zh, "action.new_collection") => "新建集合",
        (Locale::Zh, "action.new_request") => "新建请求",
        (Locale::Zh, "action.rename") => "重命名",
        (Locale::Zh, "action.delete") => "删除",
        (Locale::Zh, "action.new_group") => "新建分组",
        (Locale::Zh, "action.plugin") => "生成代码…",
        (Locale::Zh, "codegen.hint") => "切换语言 / 库后代码即时更新",
        (Locale::Zh, "action.copy") => "复制",
        (Locale::Zh, "action.send") => "发送",
        (Locale::Zh, "action.sending") => "发送中…",
        (Locale::Zh, "action.stop") => "停止",
        (Locale::Zh, "sse.live") => "事件流",
        (Locale::Zh, "sse.events") => "事件",
        (Locale::Zh, "sse.stopped") => "事件流已停止",
        (Locale::Zh, "sse.truncated") => "事件流过长，界面只保留前 2MB",
        (Locale::Zh, "sse.time") => "时间",
        (Locale::Zh, "settings.font") => "字体",
        (Locale::Zh, "settings.font.ui") => "界面字体",
        (Locale::Zh, "settings.font.ui_size") => "界面字号",
        (Locale::Zh, "settings.font.mono") => "代码字体",
        (Locale::Zh, "settings.font.mono_size") => "代码字号",
        (Locale::Zh, "settings.font.system") => "系统默认",
        (Locale::Zh, "settings.font.hint") => "字体家族留空＝系统默认；界面字号 / 代码字号范围",
        (Locale::Zh, "settings.editor_line_h") => "内容行高（px）",
        (Locale::Zh, "settings.editor_line_h.hint") => "只作用于 JSON/正文内容：请求体 JSON、Docs、响应正文、响应头与 Cookies/Timeline 列表；Query/Headers 等表格组件高度固定，不受影响。范围 12–32，改完立刻生效",
        (Locale::Zh, "editor.tab.headers") => "Headers",
        (Locale::Zh, "editor.tab.body") => "Body",
        (Locale::Zh, "editor.tab.none") => "无",
        (Locale::Zh, "editor.tab.json") => "JSON",
        (Locale::Zh, "editor.tab.text") => "文本",
        (Locale::Zh, "editor.tab.form") => "表单",
        (Locale::Zh, "editor.tab.multipart") => "Form-Data",
        (Locale::Zh, "editor.tab.file") => "文件",
        (Locale::Zh, "editor.tab.raw") => "原始",
        (Locale::Zh, "editor.choose_file") => "选择文件…",
        (Locale::Zh, "editor.body.none_hint") => "无请求体",
        (Locale::Zh, "editor.body.file_hint") => "请求体为该文件的二进制内容",
        (Locale::Zh, "editor.form.text") => "文本",
        (Locale::Zh, "editor.form.file") => "文件",
        (Locale::Zh, "editor.form.add") => "+ 字段",
        (Locale::Zh, "editor.form.choose_file") => "选择文件…",
        (Locale::Zh, "editor.select_hint") => "从左侧选择一个请求",
        (Locale::Zh, "response.title") => "响应",
        (Locale::Zh, "response.error") => "错误",
        (Locale::Zh, "response.size") => "大小",
        (Locale::Zh, "response.tab.headers") => "Headers",
        (Locale::Zh, "action.import_curl") => "从 cURL 导入…",
        (Locale::Zh, "action.import") => "导入",
        (Locale::Zh, "action.cancel") => "取消",
        (Locale::Zh, "action.close") => "关闭",
        (Locale::Zh, "url.title") => "完整 URL",
        (Locale::Zh, "url.empty") => "（未填写 URL）",
        (Locale::Zh, "url.edit_label") => "请求 URL（可直接改，支持 {{变量}}）",
        (Locale::Zh, "action.save") => "保存",
        (Locale::Zh, "net.title") => "网络设置",
        (Locale::Zh, "editor.tab.auth") => "认证",
        (Locale::Zh, "auth.kind") => "方式",
        (Locale::Zh, "auth.kind.none") => "无",
        (Locale::Zh, "auth.kind.bearer") => "Bearer",
        (Locale::Zh, "auth.kind.basic") => "Basic",
        (Locale::Zh, "auth.kind.apikey") => "API Key",
        (Locale::Zh, "auth.token") => "Token",
        (Locale::Zh, "auth.prefix") => "前缀",
        (Locale::Zh, "auth.username") => "用户名",
        (Locale::Zh, "auth.password") => "密码",
        (Locale::Zh, "auth.keyname") => "参数名",
        (Locale::Zh, "auth.keyvalue") => "参数值",
        (Locale::Zh, "auth.in") => "位置",
        (Locale::Zh, "auth.in_header") => "请求头",
        (Locale::Zh, "auth.in_query") => "Query",
        (Locale::Zh, "auth.hint") => {
            "值里可以写 {{ 变量 }}，发送时按当前环境解析；Token 放环境里就不会进仓库。"
        }
        (Locale::Zh, "auth.summary") => "实际发出：",
        (Locale::Zh, "auth.manual_wins") => "这个请求已手写 Authorization 头，认证设置不会覆盖它。",
        (Locale::Zh, "cookies.title") => "Cookie 罐",
        (Locale::Zh, "cookies.count_prefix") => "（",
        (Locale::Zh, "cookies.count_suffix") => " 条）",
        (Locale::Zh, "cookies.hint") => {
            "响应里的 Set-Cookie 会存在这儿，之后对同域请求自动带上（自己写了 Cookie 头就不插手）。清掉后立刻生效。"
        }
        (Locale::Zh, "cookies.empty") => "罐子是空的 —— 发一个带 Set-Cookie 的请求就有了",
        (Locale::Zh, "cookies.clear_host") => "清空本站",
        (Locale::Zh, "cookies.clear_all") => "全部清空",
        (Locale::Zh, "cookies.cleared_host") => "已清掉本站 ",
        (Locale::Zh, "cookies.cleared_all") => "已清掉全部 ",
        (Locale::Zh, "cookies.unit") => " 条 cookie",
        (Locale::Zh, "net.timeout_hint") => {
            "单请求超时（秒，1–600）。留空 = 默认 30。超过就报超时；事件流只拿它当「连接 + 等首包」的上限，之后可以一直流。"
        }
        (Locale::Zh, "net.timeout_short") => "超时 ",
        (Locale::Zh, "net.saved") => "已保存网络设置 · ",
        (Locale::Zh, "proxy.hint") => {
            "给所有请求走代理（Clash 之类）。留空 = 直连；支持 http:// 与 socks5://，只填 127.0.0.1:7897 也行。"
        }
        (Locale::Zh, "proxy.current") => "当前：",
        (Locale::Zh, "proxy.none") => "直连（未设代理）",
        (Locale::Zh, "proxy.clear") => "清空并直连",
        (Locale::Zh, "proxy.saved") => "已启用代理 · ",
        (Locale::Zh, "proxy.cleared") => "已关闭代理（直连）",
        (Locale::Zh, "action.edit_env") => "编辑环境…",
        (Locale::Zh, "trash.title") => "回收站",
        (Locale::Zh, "trash.empty") => "回收站是空的",
        (Locale::Zh, "trash.restore") => "恢复",
        (Locale::Zh, "trash.empty_all") => "清空",
        (Locale::Zh, "trash.empty_confirm") => "确认清空（不可恢复）",
        (Locale::Zh, "import.hint") => "粘贴一条 curl 命令（支持 -X/-H/-d/--json/-u 等）",
        (Locale::Zh, "response.copy_line") => "复制这一行",
        (Locale::En, "response.copy_line") => "Copy line",
        (Locale::Zh, "response.copy_value") => "复制值",
        (Locale::En, "response.copy_value") => "Copy value",
        (Locale::Zh, "response.copy_path") => "复制 JSON 路径",
        (Locale::En, "response.copy_path") => "Copy JSON path",
        (Locale::Zh, "flash.copied_line") => "已复制这一行",
        (Locale::En, "flash.copied_line") => "Line copied",
        (Locale::Zh, "flash.copied_value") => "已复制该行的值",
        (Locale::En, "flash.copied_value") => "Value copied",
        (Locale::Zh, "flash.copied_path") => "已复制路径：",
        (Locale::En, "flash.copied_path") => "Path copied: ",
        (Locale::Zh, "flash.path_not_found") => {
            "没能定位到这行的路径：响应太大、不是 JSON，或值不唯一"
        }
        (Locale::En, "flash.path_not_found") => {
            "Could not locate this line's path (too big, not JSON, or value not unique)"
        }
        (Locale::Zh, "response.fold_all") => "折叠全部",
        (Locale::En, "response.fold_all") => "Fold all",
        (Locale::Zh, "response.unfold_all") => "展开全部",
        (Locale::En, "response.unfold_all") => "Expand all",
        (Locale::Zh, "flash.no_response") => "还没有响应可以过滤：先发一次请求",
        (Locale::En, "flash.no_response") => "No response to filter yet — send a request first",
        (Locale::Zh, "flash.copied_url") => "已复制 URL：",
        (Locale::Zh, "flash.copied_curl") => "已复制 cURL 到剪贴板",
        (Locale::Zh, "flash.duplicated") => "已复制为新请求：",
        (Locale::Zh, "flash.duplicate_failed") => "复制失败：",
        (Locale::Zh, "flash.copy_suffix") => " 副本",
        (Locale::Zh, "flash.moved") => "已移动到：",
        (Locale::Zh, "flash.copied_code") => "已复制代码到剪贴板",
        (Locale::Zh, "flash.copied_text") => "已复制选中文本",
        (Locale::Zh, "flash.beautified") => "已美化 JSON 正文",
        (Locale::Zh, "action.beautify") => "美化",
        (Locale::Zh, "kv.chars") => "字符",
        (Locale::Zh, "flash.copied_all") => "已复制全文",
        (Locale::Zh, "kv.edit_hint") => "可直接编辑，实时写回",
        (Locale::Zh, "editor.body.json_error") => {
            "JSON 格式错误：第 {line} 行第 {col} 列 · {msg}"
        }
        (Locale::Zh, "flash.move_failed") => "移动失败：",
        (Locale::En, "flash.copied_url") => "URL copied: ",
        (Locale::En, "flash.copied_curl") => "cURL copied to clipboard",
        (Locale::En, "flash.duplicated") => "Duplicated as: ",
        (Locale::En, "flash.duplicate_failed") => "Duplicate failed: ",
        (Locale::En, "flash.copy_suffix") => " copy",
        (Locale::En, "flash.moved") => "Moved to: ",
        (Locale::En, "flash.copied_code") => "Code copied to clipboard",
        (Locale::En, "flash.copied_text") => "Selected text copied",
        (Locale::En, "flash.beautified") => "JSON formatted",
        (Locale::En, "action.beautify") => "Format",
        (Locale::En, "kv.chars") => "chars",
        (Locale::En, "flash.copied_all") => "Full value copied",
        (Locale::En, "kv.edit_hint") => "Edit here — applied live",
        (Locale::En, "editor.body.json_error") => {
            "Invalid JSON: line {line}, column {col} · {msg}"
        }
        (Locale::En, "flash.move_failed") => "Move failed: ",
        (Locale::Zh, "action.copy_url") => "复制 URL",
        (Locale::Zh, "action.copy_curl") => "复制为 cURL",
        (Locale::Zh, "action.duplicate") => "复制为新请求",
        (Locale::Zh, "action.move_to") => "移动到…",
        (Locale::Zh, "action.collapse_others") => "折叠所有（除此节点）",
        (Locale::Zh, "action.move_root") => "（集合根目录）",
        (Locale::Zh, "move.title") => "移动到…",
        (Locale::Zh, "move.moving") => "正在移动：",
        (Locale::Zh, "move.placeholder") => "输入集合 / 分组名过滤…",
        (Locale::Zh, "move.empty") => "没有匹配的目标",
        (Locale::Zh, "move.hint") => "↑↓ 选择 · Enter 移动 · Esc 取消",
        (Locale::En, "action.copy_url") => "Copy URL",
        (Locale::En, "action.copy_curl") => "Copy as cURL",
        (Locale::En, "action.duplicate") => "Duplicate request",
        (Locale::En, "action.move_to") => "Move to…",
        (Locale::En, "action.collapse_others") => "Collapse All Others",
        (Locale::En, "action.move_root") => " (collection root)",
        (Locale::En, "move.title") => "Move to…",
        (Locale::En, "move.moving") => "Moving: ",
        (Locale::En, "move.placeholder") => "Filter collections / groups…",
        (Locale::En, "move.empty") => "No matching destination",
        (Locale::En, "move.hint") => "↑↓ select · Enter move · Esc cancel",
        (Locale::Zh, "vars.save_title") => "把响应值存成环境变量",
        (Locale::Zh, "vars.save_btn") => "存为变量",
        (Locale::Zh, "vars.save_hint") => {
            "取响应里的一个值（比如登录返回的 token）存进环境变量，后面的请求用 {{ 变量名 }} 就能引用。"
        }
        (Locale::Zh, "vars.source_body") => "响应正文",
        (Locale::Zh, "vars.source_header") => "响应头",
        (Locale::Zh, "vars.path_hint") => {
            "JSON 路径（$.data.token 这种，过滤条里写的路径会自动带过来）"
        }
        (Locale::Zh, "vars.header_hint") => "响应头名字（大小写不敏感）",
        (Locale::Zh, "vars.preview") => "取到的值",
        (Locale::Zh, "vars.matched_first") => " 个命中，取了第一个",
        (Locale::Zh, "vars.name") => "变量名（写进环境，别带空格）",
        (Locale::Zh, "vars.target_env") => "将写入：",
        (Locale::Zh, "vars.will_overwrite") => "同名变量已存在，保存会覆盖",
        (Locale::Zh, "vars.saved") => "已保存 ",
        (Locale::Zh, "vars.saved_to") => " 到 ",
        (Locale::Zh, "vars.no_response") => "还没有响应",
        (Locale::Zh, "vars.save_empty") => "没有可存的值",
        (Locale::En, "vars.save_title") => "Save response value as a variable",
        (Locale::En, "vars.save_btn") => "Save as var",
        (Locale::En, "vars.save_hint") => {
            "Pick a value from the response (e.g. a login token) and store it in an environment variable; later requests can reference it with {{ name }}."
        }
        (Locale::En, "vars.source_body") => "Body",
        (Locale::En, "vars.source_header") => "Header",
        (Locale::En, "vars.path_hint") => {
            "JSON path (e.g. $.data.token; the filter bar path is prefilled)"
        }
        (Locale::En, "vars.header_hint") => "Response header name (case-insensitive)",
        (Locale::En, "vars.preview") => "Value",
        (Locale::En, "vars.matched_first") => " matches, using the first",
        (Locale::En, "vars.name") => "Variable name (no spaces)",
        (Locale::En, "vars.target_env") => "Will write to: ",
        (Locale::En, "vars.will_overwrite") => {
            "A variable with this name exists; saving overwrites it"
        }
        (Locale::En, "vars.saved") => "Saved ",
        (Locale::En, "vars.saved_to") => " to ",
        (Locale::En, "vars.no_response") => "No response yet",
        (Locale::En, "vars.save_empty") => "Nothing to save",
        (Locale::Zh, "vars.complete_hint") => "Tab / Enter 补齐 · Esc 关闭",
        (Locale::Zh, "vars.title") => "变量",
        (Locale::Zh, "vars.hint") => {
            "发送时把 {{ 变量 }} 换成这里的值；点「补进环境」可把缺失的补上。"
        }
        (Locale::Zh, "vars.missing") => "未定义变量 ",
        (Locale::Zh, "vars.undefined") => "未定义 · 发送时会原样带出",
        (Locale::Zh, "vars.add") => "补进环境",
        (Locale::Zh, "vars.empty") => "环境里还没有变量",
        (Locale::Zh, "vars.env") => "当前环境",
        (Locale::Zh, "vars.masked_hint") => "敏感名默认打码",
        (Locale::Zh, "vars.reveal") => "显示明文",
        (Locale::Zh, "vars.hide") => "隐藏明文",
        (Locale::Zh, "env.none") => "无（仅 base）",
        (Locale::Zh, "action.import_file") => "从文件导入（Postman / OpenAPI / HAR）…",
        (Locale::Zh, "import.done") => "已导入：",
        (Locale::Zh, "import.requests") => "个接口",
        (Locale::Zh, "import.groups") => "个分组",
        (Locale::Zh, "import.read_failed") => "读文件失败：",
        (Locale::Zh, "import.failed") => "导入失败：",
        (Locale::Zh, "response.tab.history") => "历史",
        (Locale::Zh, "response.history_empty") => "还没有发送记录",
        (Locale::Zh, "history.just_now") => "刚刚",
        (Locale::Zh, "history.ago") => "前",
        (Locale::Zh, "ws.missing") => "工作区不存在，请通过菜单选择目录",
        (Locale::En, "app.name") => "treq",
        (Locale::En, "menu.file") => "File",
        (Locale::En, "action.choose_workspace") => "Choose Workspace…",
        (Locale::En, "action.reload") => "Reload",
        (Locale::En, "action.language") => "Language: 中文",
        (Locale::En, "side.search") => "Filter",
        (Locale::En, "side.base_environment") => "Base Environment",
        (Locale::En, "side.env_vars") => "Environment Variables",
        (Locale::En, "action.quick_panel") => "Command Palette",
        (Locale::En, "workspace.add") => "Add workspace (project)…",
        (Locale::En, "workspace.remove") => "Remove current workspace from list",
        (Locale::En, "quick.hint_all_ws") => "Search across workspaces: name / URL / method",
        (Locale::En, "action.preferences") => "Preferences",
        (Locale::En, "editor.tab.query") => "Query",
        (Locale::En, "editor.section.url_preview") => "URL PREVIEW",
        (Locale::En, "editor.section.docs") => "DOCS",
        (Locale::En, "editor.body.form_hint") => "Form body: a=1&b=2",
        (Locale::En, "editor.kv.add") => "Add",
        (Locale::En, "editor.kv.delete_all") => "Delete All",
        (Locale::En, "editor.kv.toggle_desc") => "Toggle Description",
        (Locale::En, "editor.footer.import") => "Import from cURL",
        (Locale::En, "editor.footer.codegen") => "Generate Code",
        (Locale::En, "docs.placeholder") => "Describe this request (Markdown)…",
        (Locale::En, "response.tab.preview") => "Preview",
        (Locale::En, "response.tab.cookies") => "Cookies",
        (Locale::En, "response.tab.timeline") => "Timeline",
        (Locale::En, "response.view.pretty") => "Prettified",
        (Locale::En, "response.view.raw") => "Raw",
        (Locale::En, "response.none") => "No response yet — hit Send",
        (Locale::En, "response.cookies_empty") => "Response has no Set-Cookie",
        (Locale::En, "response.status") => "Status",
        (Locale::En, "response.timeline.total") => "Total time",
        (Locale::En, "response.filter.placeholder") => "$.store.books[*].author",
        (Locale::En, "response.filter.not_json") => "Response is not JSON, cannot filter by path",
        (Locale::En, "response.filter.bad") => "Invalid path",
        (Locale::En, "response.filter.empty") => "Path matched nothing",
        (Locale::En, "history.min_ago") => "m ago",
        (Locale::En, "history.hour_ago") => "h ago",
        (Locale::En, "history.day_ago") => "d ago",
        (Locale::En, "response.saved") => "Saved to: ",
        (Locale::En, "response.save_failed") => "Save failed: ",
        (Locale::En, "response.copied") => "Response body copied · ",
        (Locale::En, "response.truncated") => "Response too large — UI keeps the first ",
        (Locale::En, "response.too_big") => "Response too large (over 2 MB) — rendering skipped",
        (Locale::En, "response.too_big_hint") => "click “Full” to load",
        (Locale::En, "response.view.full") => "Full",
        (Locale::En, "backup.title") => "Backups",
        (Locale::En, "backup.dir") => "Backup folder",
        (Locale::En, "backup.now") => "Back up now",
        (Locale::En, "backup.empty") => "No backups yet",
        (Locale::En, "backup.list_hint") => "Backups (newest first) — pick one to restore",
        (Locale::En, "backup.restore_merge") => "Merge into current",
        (Locale::En, "backup.restore_overwrite") => "Replace with backup",
        (Locale::En, "backup.merge_hint") => {
            "Merge: only missing/changed files come back; current requests stay"
        }
        (Locale::En, "backup.overwrite_hint") => {
            "Replace: swap the whole workspace for this backup (a safety backup is taken first)"
        }
        (Locale::En, "backup.safety") => "safety copy",
        (Locale::En, "backup.collections") => "collections",
        (Locale::En, "backup.requests") => "requests",
        (Locale::En, "backup.done_now") => "Backed up: ",
        (Locale::En, "backup.done_merge") => "Merged: ",
        (Locale::En, "backup.done_overwrite") => "Replaced with: ",
        (Locale::En, "backup.failed") => "Backup failed: ",
        (Locale::En, "backup.restore_failed") => "Restore failed: ",
        (Locale::En, "app.tagline") => "treq — a local-first API client",
        (Locale::En, "quick.placeholder") => {
            "Search requests or history… (Enter to pick / Esc to close)"
        }
        (Locale::En, "quick.empty") => "No matching requests or history",
        (Locale::En, "side.environment") => "Environment",
        (Locale::En, "side.no_collections") => "(empty) right-click to create",
        (Locale::En, "action.new_collection") => "New Collection",
        (Locale::En, "action.new_request") => "New Request",
        (Locale::En, "action.rename") => "Rename",
        (Locale::En, "action.delete") => "Delete",
        (Locale::En, "action.new_group") => "New Group",
        (Locale::En, "action.plugin") => "Generate Code…",
        (Locale::En, "codegen.hint") => "Pick a language / library — code updates instantly",
        (Locale::En, "action.copy") => "Copy",
        (Locale::En, "action.send") => "Send",
        (Locale::En, "action.sending") => "Sending…",
        (Locale::En, "action.stop") => "Stop",
        (Locale::En, "sse.live") => "Event stream",
        (Locale::En, "sse.events") => "events",
        (Locale::En, "sse.stopped") => "Event stream stopped",
        (Locale::En, "sse.truncated") => "Stream too long — UI keeps the first 2MB",
        (Locale::En, "sse.time") => "Time",
        (Locale::En, "settings.font") => "Fonts",
        (Locale::En, "settings.font.ui") => "UI font",
        (Locale::En, "settings.font.ui_size") => "UI size",
        (Locale::En, "settings.font.mono") => "Code font",
        (Locale::En, "settings.font.mono_size") => "Code size",
        (Locale::En, "settings.font.system") => "System default",
        (Locale::En, "settings.font.hint") => "Empty family = system default. Size ranges (UI / code)",
        (Locale::En, "settings.editor_line_h") => "Content line height (px)",
        (Locale::En, "settings.editor_line_h.hint") => "Applies to JSON/content only: body JSON, docs, response body, response headers & Cookies/Timeline lists. Query/Headers tables keep a fixed row height · 12–32 · applies instantly",
        (Locale::En, "editor.tab.headers") => "Headers",
        (Locale::En, "editor.tab.body") => "Body",
        (Locale::En, "editor.tab.none") => "None",
        (Locale::En, "editor.tab.json") => "JSON",
        (Locale::En, "editor.tab.text") => "Text",
        (Locale::En, "editor.tab.form") => "Form",
        (Locale::En, "editor.tab.multipart") => "Form-Data",
        (Locale::En, "editor.tab.file") => "File",
        (Locale::En, "editor.tab.raw") => "Raw",
        (Locale::En, "editor.choose_file") => "Choose File…",
        (Locale::En, "editor.body.none_hint") => "No request body",
        (Locale::En, "editor.body.file_hint") => "Body is the raw bytes of this file",
        (Locale::En, "editor.form.text") => "Text",
        (Locale::En, "editor.form.file") => "File",
        (Locale::En, "editor.form.add") => "+ Field",
        (Locale::En, "editor.form.choose_file") => "Choose File…",
        (Locale::En, "editor.select_hint") => "Select a request on the left",
        (Locale::En, "response.title") => "Response",
        (Locale::En, "response.error") => "Error",
        (Locale::En, "response.size") => "Size",
        (Locale::En, "response.tab.headers") => "Headers",
        (Locale::En, "action.import_curl") => "Import from cURL…",
        (Locale::En, "action.import") => "Import",
        (Locale::En, "action.cancel") => "Cancel",
        (Locale::En, "action.close") => "Close",
        (Locale::En, "url.title") => "Full URL",
        (Locale::En, "url.empty") => "(no URL yet)",
        (Locale::En, "url.edit_label") => "Request URL (editable, supports {{variables}})",
        (Locale::En, "action.save") => "Save",
        (Locale::En, "net.title") => "Network Settings",
        (Locale::En, "editor.tab.auth") => "Auth",
        (Locale::En, "auth.kind") => "Type",
        (Locale::En, "auth.kind.none") => "None",
        (Locale::En, "auth.kind.bearer") => "Bearer",
        (Locale::En, "auth.kind.basic") => "Basic",
        (Locale::En, "auth.kind.apikey") => "API Key",
        (Locale::En, "auth.token") => "Token",
        (Locale::En, "auth.prefix") => "Prefix",
        (Locale::En, "auth.username") => "Username",
        (Locale::En, "auth.password") => "Password",
        (Locale::En, "auth.keyname") => "Name",
        (Locale::En, "auth.keyvalue") => "Value",
        (Locale::En, "auth.in") => "In",
        (Locale::En, "auth.in_header") => "Header",
        (Locale::En, "auth.in_query") => "Query",
        (Locale::En, "auth.hint") => {
            "Values may use {{ var }} — resolved against the active environment at send time, so tokens stay out of the repo."
        }
        (Locale::En, "auth.summary") => "Sends: ",
        (Locale::En, "auth.manual_wins") => {
            "This request already sets an Authorization header by hand; auth settings will not override it."
        }
        (Locale::En, "cookies.title") => "Cookie Jar",
        (Locale::En, "cookies.count_prefix") => " (",
        (Locale::En, "cookies.count_suffix") => ")",
        (Locale::En, "cookies.hint") => {
            "Set-Cookie from responses is kept here and sent back to the same domain automatically (unless you wrote a Cookie header yourself). Clearing takes effect immediately."
        }
        (Locale::En, "cookies.empty") => "Jar is empty — send a request that returns Set-Cookie",
        (Locale::En, "cookies.clear_host") => "Clear site",
        (Locale::En, "cookies.clear_all") => "Clear all",
        (Locale::En, "cookies.cleared_host") => "Cleared ",
        (Locale::En, "cookies.cleared_all") => "Cleared all ",
        (Locale::En, "cookies.unit") => " cookies",
        (Locale::En, "net.timeout_hint") => {
            "Per-request timeout in seconds (1-600). Empty = 30s. Event streams only use it for connect + first bytes, then keep streaming."
        }
        (Locale::En, "net.timeout_short") => "timeout ",
        (Locale::En, "net.saved") => "Network settings saved · ",
        (Locale::En, "proxy.hint") => {
            "Route every request through a proxy (Clash etc.). Empty = direct. http:// and socks5:// supported; bare 127.0.0.1:7897 works too."
        }
        (Locale::En, "proxy.current") => "Current: ",
        (Locale::En, "proxy.none") => "Direct (no proxy)",
        (Locale::En, "proxy.clear") => "Clear & go direct",
        (Locale::En, "proxy.saved") => "Proxy enabled · ",
        (Locale::En, "proxy.cleared") => "Proxy disabled (direct)",
        (Locale::En, "action.edit_env") => "Edit Environment…",
        (Locale::En, "trash.title") => "Trash",
        (Locale::En, "trash.empty") => "Trash is empty",
        (Locale::En, "trash.restore") => "Restore",
        (Locale::En, "trash.empty_all") => "Empty",
        (Locale::En, "trash.empty_confirm") => "Confirm empty (permanent)",
        (Locale::En, "import.hint") => "Paste a curl command (-X/-H/-d/--json/-u supported)",
        (Locale::En, "vars.complete_hint") => "Tab / Enter to complete · Esc to dismiss",
        (Locale::En, "vars.title") => "Variables",
        (Locale::En, "vars.hint") => {
            "{{ var }} is replaced with these values on send; use “Add to env” for missing ones."
        }
        (Locale::En, "vars.missing") => "Undefined variables: ",
        (Locale::En, "vars.undefined") => "undefined · sent literally",
        (Locale::En, "vars.add") => "Add to env",
        (Locale::En, "vars.empty") => "No variables yet",
        (Locale::En, "vars.env") => "Environment",
        (Locale::En, "vars.masked_hint") => "secret-looking names are masked",
        (Locale::En, "vars.reveal") => "Reveal",
        (Locale::En, "vars.hide") => "Hide",
        (Locale::En, "env.none") => "none (base only)",
        (Locale::En, "action.import_file") => "Import from file (Postman / OpenAPI / HAR)…",
        (Locale::En, "import.done") => "Imported: ",
        (Locale::En, "import.requests") => " requests",
        (Locale::En, "import.groups") => " groups",
        (Locale::En, "import.read_failed") => "Cannot read file: ",
        (Locale::En, "import.failed") => "Import failed: ",
        (Locale::En, "response.tab.history") => "History",
        (Locale::En, "response.history_empty") => "No requests sent yet",
        (Locale::En, "history.just_now") => "now",
        (Locale::En, "history.ago") => " ago",
        (Locale::En, "ws.missing") => "Workspace missing, choose a folder from the menu",
        (Locale::Zh, "settings.title") => "设置",
        (Locale::Zh, "settings.close") => "关闭",
        (Locale::Zh, "settings.tab.style") => "样式",
        (Locale::Zh, "settings.tab.general") => "通用",
        (Locale::Zh, "settings.tab.net") => "网络",
        (Locale::Zh, "settings.tab.backup") => "备份",
        (Locale::Zh, "settings.style.hint") => {
            "下拉框样式：点行选中即生效并记入配置；点“样例”可看这一款的打开态"
        }
        (Locale::Zh, "settings.general.language") => "语言",
        (Locale::Zh, "settings.general.workspace") => "工作区",
        (Locale::Zh, "settings.general.choose_dir") => "选择目录…",
        (Locale::Zh, "settings.backup.interval") => "自动备份间隔",
        (Locale::Zh, "settings.backup.off") => "关闭",
        (Locale::Zh, "settings.backup.minutes") => " 分钟",
        (Locale::Zh, "settings.backup.day") => " 天",
        (Locale::Zh, "settings.backup.keep") => "备份保留份数",
        (Locale::Zh, "settings.backup.files") => " 份",
        (Locale::Zh, "settings.backup.hint") =>
            "默认一天一次，且只在工作区有改动时备份；超过保留份数的旧备份会自动清掉（改动最多 30 秒后生效）",
        (Locale::Zh, "settings.backup.restore") => "从备份恢复…",
        (Locale::Zh, "style.classic") => "经典描边",
        (Locale::Zh, "style.classic.desc") => "原样式：粗描边 + 文字箭头",
        (Locale::Zh, "style.ghost") => "幽灵无框",
        (Locale::Zh, "style.ghost.desc") => "无边框，悬停才浮出浅底",
        (Locale::Zh, "style.outlined") => "强化描边（推荐）",
        (Locale::Zh, "style.outlined.desc") => "细描边；悬停/打开时描边与箭头染品牌紫",
        (Locale::Zh, "style.underline") => "下划线页签",
        (Locale::Zh, "style.underline.desc") => "无盒，品牌紫下划线标记开合",
        (Locale::Zh, "style.pill") => "胶囊",
        (Locale::Zh, "style.pill.desc") => "胶囊底；打开时紫底白字",
        (Locale::Zh, "style.split") => "分离箭头",
        (Locale::Zh, "style.split.desc") => "原生弹钮：箭头区带分隔线，悬停独立高亮",
        (Locale::En, "settings.title") => "Settings",
        (Locale::En, "settings.close") => "Close",
        (Locale::En, "settings.tab.style") => "Styles",
        (Locale::En, "settings.tab.general") => "General",
        (Locale::En, "settings.tab.net") => "Network",
        (Locale::En, "settings.tab.backup") => "Backups",
        (Locale::En, "settings.style.hint") => {
            "Dropdown style — click a row to apply; click a sample to preview its open state"
        }
        (Locale::En, "settings.general.language") => "Language",
        (Locale::En, "settings.general.workspace") => "Workspace",
        (Locale::En, "settings.general.choose_dir") => "Choose folder…",
        (Locale::En, "settings.backup.interval") => "Auto-backup",
        (Locale::En, "settings.backup.off") => "Off",
        (Locale::En, "settings.backup.minutes") => " min",
        (Locale::En, "settings.backup.day") => " day",
        (Locale::En, "settings.backup.keep") => "Kept backups",
        (Locale::En, "settings.backup.files") => " copies",
        (Locale::En, "settings.backup.hint") => {
            "Daily by default, and only when something changed; anything beyond the kept count is pruned (changes apply within 30s)"
        }
        (Locale::En, "settings.backup.restore") => "Restore from backup…",
        (Locale::En, "style.classic") => "Classic outline",
        (Locale::En, "style.classic.desc") => "Original: heavy border + text caret",
        (Locale::En, "style.ghost") => "Ghost",
        (Locale::En, "style.ghost.desc") => "No box; surface shows up on hover",
        (Locale::En, "style.outlined") => "Outlined (recommended)",
        (Locale::En, "style.outlined.desc") => {
            "Fine border; border and caret turn brand purple on hover/open"
        }
        (Locale::En, "style.underline") => "Underline",
        (Locale::En, "style.underline.desc") => "No box; brand-purple underline marks open state",
        (Locale::En, "style.pill") => "Pill",
        (Locale::En, "style.pill.desc") => {
            "Pill background; turns purple with white text when open"
        }
        (Locale::En, "style.split") => "Split caret",
        (Locale::En, "style.split.desc") => "Native popup button: divider before the caret zone",
        (_, _) => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 从 i18n 表里抠出某个 locale 的全部键：`(Locale::Zh, "键") => "文案"`
    fn table_keys(src: &str, locale: &str) -> BTreeSet<String> {
        let head = format!("(Locale::{locale}, \"");
        src.lines()
            .filter_map(|l| {
                let rest = l.trim().strip_prefix(&head)?;
                Some(rest.split('"').next()?.to_string())
            })
            .collect()
    }

    /// 代码里的 i18n 键：`t("键")` 与 `tr(Locale::X, "键")`；动态键（含 `{}`）跳过。
    fn used_keys(src: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for pat in [".t(\"", "tr(Locale::Zh, \"", "tr(Locale::En, \""] {
            let mut rest = src;
            while let Some(i) = rest.find(pat) {
                rest = &rest[i + pat.len()..];
                let Some(end) = rest.find('"') else { break };
                let key = &rest[..end];
                if !key.is_empty() && !key.contains('{') {
                    out.insert(key.to_string());
                }
                rest = &rest[end..];
            }
        }
        out
    }

    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rust_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    #[test]
    fn zh_and_en_tables_have_the_same_keys() {
        let src = include_str!("i18n.rs");
        let zh = table_keys(src, "Zh");
        let en = table_keys(src, "En");
        assert!(zh.len() > 100, "解析出的键太少，可能解析坏了：{}", zh.len());
        let missing_en: Vec<_> = zh.difference(&en).cloned().collect();
        let missing_zh: Vec<_> = en.difference(&zh).cloned().collect();
        assert!(missing_en.is_empty(), "英文表缺这些键：{missing_en:?}");
        assert!(missing_zh.is_empty(), "中文表缺这些键：{missing_zh:?}");
    }

    #[test]
    fn every_key_used_in_code_exists_in_the_table() {
        let mut files = Vec::new();
        rust_files(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let table = table_keys(include_str!("i18n.rs"), "Zh");
        let mut bad = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            // 只看测试代码之前的部分：测试里的键（含故意写错的样例）不算
            let prod = src.split("#[cfg(test)]").next().unwrap_or("").to_string();
            for key in used_keys(&prod) {
                if !table.contains(&key) {
                    bad.push(format!("{} → {key}", f.display()));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "代码里用了、但表里没有的键（界面上会显示成 ?）：\n{}",
            bad.join("\n")
        );
    }

    #[test]
    fn unknown_key_falls_back_to_question_mark() {
        assert_eq!(tr(Locale::Zh, "根本没有这个键"), "?");
        assert_eq!(tr(Locale::En, "根本没有这个键"), "?");
    }
}
