# treq 基础功能设计

日期：2026-07-21 | 状态：已批准（用户确认 B 范围 + Yaak 式存储 + macOS + 中英双语默认中文）

## 目标

用 Rust + GPUI 构建类 Yaak 的接口请求工具，第一版范围：

- 集合树 + 请求编辑器（方法/URL/Params/Headers/Body）+ 响应查看器 + 保存加载
- 环境变量（Base + 覆盖环境，`{{ var }}` 语法）

## 架构

Cargo workspace，两个 crate：

- **treq-core**：无 GPUI 依赖。数据模型、YAML 持久化、环境变量解析、HTTP 执行（reqwest/rustls）、JSON 美化。可独立单元测试。
- **treq-app**：GPUI 界面。三栏布局、状态管理、菜单、文件对话框、i18n 消息表。

## 数据模型与存储

Yaak 式目录结构，工作区根目录由用户选择（默认 `~/Documents/treq/`，首次启动自动创建）：

```
workspace_root/
├── collections/
│   └── <collection_id>/
│       ├── collection.yml    # id, name
│       └── requests/
│           └── <request_id>.yml
└── environments/
    ├── base.yml              # 兜底变量
    └── dev.yml / prod.yml    # 可选，叠加在 base 之上
```

规则：

- 文件以 uuid 命名，name 存文件内（重命名不破坏 Git 历史）。
- YAML schema 仿 Yaak：
  - `collection.yml`: `id`, `name`
  - `request.yml`: `id`, `name`, `method`, `url`, `params` (key/value/enabled), `headers` (key/value/enabled), `body` (`kind: none|json|form|raw`, `content`)
  - `environment.yml`: `id`, `name`, `variables: {name: value}`
- 应用设置（workspace_root、locale、激活环境）存 `~/Library/Application Support/com.treq.app/settings.toml`。
- v1 不做实时文件监听，工具栏提供手动刷新按钮。

## 环境变量

- 语法 `{{ varName }}`，在 URL/params/headers/body 中解析，解析发生在发送时。
- 激活环境叠加在 base 之上（base 兜底）。
- 编辑器 URL 下方显示解析后 URL 预览。

## UI（Yaak 风格三栏）

- **左栏**：集合树（集合 > 请求），右键菜单新建请求/新建集合/重命名/删除；底部环境切换下拉 + 刷新按钮。
- **中栏**：方法下拉 + URL 输入 + 发送按钮；Params/Headers 为 key-value 表格（每行可勾选启用、可删除）；Body 四个类型选择（none/json/form/raw），raw/json 均原样编辑，不做格式化按钮。
- **右栏**：响应面板——状态码/耗时/大小标题栏；Headers 列表；Body 视图（JSON 自动美化 + 简易语法着色，raw 原样）。
- **i18n**：内置消息表 `(zh/en)`，默认 zh，设置菜单切换，不引第三方库。

## HTTP 执行

- reqwest + rustls，GPUI background executor 异步发送。
- 错误（超时/连接失败/DNS）在响应面板以错误状态展示。
- 非 2xx 视为正常响应展示。

## v1 明确不做

认证辅助 · 请求历史 · 导入导出 · 实时文件监听 · 变量自动补全 · 流式响应 · 多项目 · 主题系统。

## 测试

- treq-core 单元测试：env 解析、YAML 存取（往返）、JSON 美化、变量替换。
- 端到端：本地 std TcpListener mock server，验证真实 HTTP 请求/响应解析。
- UI 手动验证。