use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kv {
    pub key: String,
    pub value: String,
    /// 缺字段时按「启用」读：手工/外部工具写出的 yml 少个 enabled 不该让整条请求消失
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 行描述（Insomnia 的 "Toggle Description"），不参与请求发送。
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind {
    None,
    Json,
    /// text/plain; charset=utf-8
    Text,
    /// application/x-www-form-urlencoded
    Form,
    /// multipart/form-data；content 每行 `key=value` 或 `key=@文件路径`
    Multipart,
    /// 二进制文件作为请求体；content 为文件路径
    File,
    /// 原文，不自动加 Content-Type
    Raw,
}

/// 解析 multipart 文本表示：每行 `key=value` 或 `key=@文件路径`。
/// 空行与 `#` 开头的注释行跳过；按第一个 `=` 切分，value 可包含 `=`。
pub fn parse_multipart(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        out.push((key.to_string(), value.trim().to_string()));
    }
    out
}

/// 新建一个空 Kv（description 默认空）。
pub fn kv_new() -> Kv {
    Kv {
        key: String::new(),
        value: String::new(),
        enabled: true,
        description: String::new(),
    }
}

/// multipart/form-data 的一个字段；`is_file` 为 true 时 `value` 是文件路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    pub key: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_file: bool,
}

impl FormField {
    pub fn new() -> Self {
        Self {
            key: String::new(),
            value: String::new(),
            enabled: true,
            is_file: false,
        }
    }
}

impl Default for FormField {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub kind: BodyKind,
    #[serde(default)]
    pub content: String,
    /// multipart/form-data 的结构化字段；旧 YAML 的 content 行会自动迁移解析。
    #[serde(default)]
    pub form_data: Vec<FormField>,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            kind: BodyKind::None,
            content: String::new(),
            form_data: Vec::new(),
        }
    }
}

impl Body {
    /// 取 multipart 字段：优先结构化 `form_data`；为空时把旧版 content 行解析出来。
    pub fn multipart_fields(&self) -> Vec<FormField> {
        if !self.form_data.is_empty() {
            return self.form_data.clone();
        }
        parse_multipart(&self.content)
            .into_iter()
            .map(|(key, value)| {
                let is_file = value.starts_with('@');
                FormField {
                    key,
                    value: if is_file {
                        value[1..].trim().to_string()
                    } else {
                        value
                    },
                    enabled: true,
                    is_file,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestItem {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub params: Vec<Kv>,
    pub headers: Vec<Kv>,
    pub body: Body,
    /// Docs 页的请求说明（markdown 原文，不参与发送）。
    #[serde(default)]
    pub description: String,
    /// 请求区底部 DOCS 折叠区是否展开（按请求维度记忆，默认展开）。
    #[serde(default = "default_true")]
    pub docs_open: bool,
    /// 认证方式（模板，可写 `{{ 变量 }}`）；None = 不用认证（老文件没这字段也一样）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<crate::auth::Auth>,
}

/// serde 默认值：老文件里没有该字段时按 true（默认展开）。
pub fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub groups: Vec<Group>,
    #[serde(skip)]
    pub requests: Vec<RequestItem>,
}

/// 集合内分组（请求文件夹）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    /// 上级分组（None = 集合下的第一层）。盘上所有分组仍平铺在
    /// `collections/<cid>/groups/<gid>/`，层级只是元数据 —— 建/删/改路径都不用动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip)]
    pub requests: Vec<RequestItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub collections: Vec<Collection>,
    pub base_env: Environment,
    pub environments: Vec<Environment>,
}
