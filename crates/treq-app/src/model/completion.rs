//! 输入框里的 `{{ 变量 }}` 补全：同步候选、按光标算候选项、浮层渲染与接受

use super::*;
use crate::widgets::SuggestKind;

/// Header key 的兜底候选：工作区还空着、或想不起来头名时也能提示。
const COMMON_HEADERS: &[&str] = &[
    "Accept",
    "Accept-Encoding",
    "Accept-Language",
    "Authorization",
    "Cache-Control",
    "Content-Type",
    "Cookie",
    "Origin",
    "Referer",
    "User-Agent",
    "X-Requested-With",
    "X-Api-Key",
];

/// 往候选里塞一个 key：跳过空串，按小写去重，保序。
fn push_candidate(out: &mut Vec<String>, seen: &mut Vec<String>, key: &str) {
    let key = key.trim();
    if key.is_empty() {
        return;
    }
    let lower = key.to_lowercase();
    if seen.contains(&lower) {
        return;
    }
    seen.push(lower);
    out.push(key.to_string());
}

/// 取一个请求里某张表的全部 key（含未勾选的行，仍然算「现有 key」）。
fn take_keys(which: KvWhich, r: &RequestItem, out: &mut Vec<String>, seen: &mut Vec<String>) {
    let keys: Vec<&str> = match which {
        KvWhich::Params => r.params.iter().map(|k| k.key.as_str()).collect(),
        KvWhich::Headers => r.headers.iter().map(|k| k.key.as_str()).collect(),
        KvWhich::FormData => r.body.form_data.iter().map(|f| f.key.as_str()).collect(),
    };
    for k in keys {
        push_candidate(out, seen, k);
    }
}

/// 取一个请求里某张表中 key 匹配的 value（用于 value 框联想历史值）。
fn take_values(
    which: KvWhich,
    r: &RequestItem,
    key: &str,
    out: &mut Vec<String>,
    seen: &mut Vec<String>,
) {
    let pairs: Vec<(&str, &str)> = match which {
        KvWhich::Params => r
            .params
            .iter()
            .map(|k| (k.key.as_str(), k.value.as_str()))
            .collect(),
        KvWhich::Headers => r
            .headers
            .iter()
            .map(|k| (k.key.as_str(), k.value.as_str()))
            .collect(),
        KvWhich::FormData => r
            .body
            .form_data
            .iter()
            .map(|f| (f.key.as_str(), f.value.as_str()))
            .collect(),
    };
    for (k, v) in pairs {
        if k.eq_ignore_ascii_case(key) {
            push_candidate(out, seen, v);
        }
    }
}

/// 常见 key 的兜底 value（按小写 key 匹配）。
fn common_values(key: &str) -> &'static [&'static str] {
    match key.trim().to_ascii_lowercase().as_str() {
        "content-type" => &[
            "application/json",
            "application/x-www-form-urlencoded",
            "multipart/form-data",
            "text/plain",
            "application/xml",
        ],
        "accept" => &["application/json", "text/html", "text/plain", "*/*"],
        "accept-encoding" => &["gzip, deflate, br", "gzip, deflate"],
        "cache-control" => &["no-cache", "no-store", "max-age=0"],
        "authorization" => &["Bearer ", "Basic "],
        _ => &[],
    }
}

impl AppModel {
    // ---- 输入框里的 `{{ 变量 }}` 补全 ----

    /// 把当前环境的变量名同步给输入框（只在变化时写，避免每帧无谓更新）。
    /// 覆盖 URL、Body、认证、KV 各行 —— 凡是能写模板的地方都能补全。
    pub fn sync_field_vars(&mut self, cx: &mut Context<Self>) {
        let names: Vec<String> = vars::merge_env(&self.workspace.base_env, self.active_env())
            .into_keys()
            .collect();
        let mut fields: Vec<Entity<TextField>> = Vec::new();
        if let Some(f) = &self.fields.url {
            fields.push(f.clone());
        }
        if let Some(f) = &self.fields.body {
            fields.push(f.clone());
        }
        if let Some(f) = &self.fields.body_file {
            fields.push(f.clone());
        }
        fields.extend(self.fields.auth.values().cloned());
        fields.extend(self.fields.kv.values().cloned());
        for f in fields {
            f.update(cx, |f, _cx| {
                if f.var_names != names {
                    f.var_names = names.clone();
                }
            });
        }
    }

    /// KV 表某个 key 框的候选：当前请求 → 最近历史 → 工作区其他请求（同类表）。
    /// 只提供数据；输入框再按用户敲的前缀过滤。设上限，避免大工作区里候选太多。
    fn kv_key_candidates_live(&self, which: KvWhich) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        // 1) 当前请求自己的 key，优先级最高
        if let Some(r) = self.selected_request() {
            take_keys(which, r, &mut out, &mut seen);
        }
        // 2) 最近发送过的历史（recent_all 已是倒序）
        for h in &self.recent_all {
            take_keys(which, &h.request, &mut out, &mut seen);
            if out.len() >= 80 {
                return out;
            }
        }
        // 3) 工作区里其他请求的 key
        for c in &self.workspace.collections {
            for r in c
                .requests
                .iter()
                .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
            {
                take_keys(which, r, &mut out, &mut seen);
                if out.len() >= 80 {
                    return out;
                }
            }
        }
        // 4) Header 再补一批常见头名，空工作区也能用
        if which == KvWhich::Headers {
            for k in COMMON_HEADERS {
                push_candidate(&mut out, &mut seen, k);
            }
        }
        out
    }

    /// 对外入口：在「当前请求 / 历史 / 其他请求」之外，再并上会话内见过的 key。
    /// 这样把某个 key 清空重敲时，它不会因为当前模型里没了就提示不出来。
    pub(crate) fn kv_key_candidates(&mut self, which: KvWhich) -> Vec<String> {
        let mut out = self.kv_key_candidates_live(which);
        let slot = match which {
            KvWhich::Params => 0,
            KvWhich::Headers => 1,
            KvWhich::FormData => 2,
        };
        let cache = &mut self.kv_seen_keys[slot];
        for v in cache.iter() {
            if out.iter().any(|e| e.eq_ignore_ascii_case(v)) == false {
                out.push(v.clone());
            }
        }
        for v in out.iter() {
            if cache.iter().any(|e| e.eq_ignore_ascii_case(v)) == false {
                cache.push(v.clone());
            }
        }
        if cache.len() > 200 {
            cache.drain(0..cache.len() - 200);
        }
        out
    }

    /// KV value 框候选：同一个 key 下用过的值（当前请求 → 历史 → 其他请求），
    /// 再补一批常见 key 的默认值（Content-Type: application/json ...）。
    fn kv_value_candidates_live(&self, which: KvWhich, key: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        if key.trim().is_empty() {
            return out;
        }
        if let Some(r) = self.selected_request() {
            take_values(which, r, key, &mut out, &mut seen);
        }
        for h in &self.recent_all {
            take_values(which, &h.request, key, &mut out, &mut seen);
            if out.len() >= 60 {
                return out;
            }
        }
        for c in &self.workspace.collections {
            for r in c
                .requests
                .iter()
                .chain(c.groups.iter().flat_map(|g| g.requests.iter()))
            {
                take_values(which, r, key, &mut out, &mut seen);
                if out.len() >= 60 {
                    return out;
                }
            }
        }
        for v in common_values(key) {
            push_candidate(&mut out, &mut seen, v);
        }
        out
    }

    /// 对外入口：value 候选同样并上会话内见过的值（清空重敲不丢）。
    pub(crate) fn kv_value_candidates(&mut self, which: KvWhich, key: &str) -> Vec<String> {
        let mut out = self.kv_value_candidates_live(which, key);
        if key.trim().is_empty() {
            return out;
        }
        let ck = String::from(crate::panes::kv_prefix(which)) + "|" + &key.trim().to_lowercase();
        let cache = self.kv_seen_values.entry(ck).or_default();
        for v in cache.iter() {
            if out.iter().any(|e| e.eq_ignore_ascii_case(v)) == false {
                out.push(v.clone());
            }
        }
        for v in out.iter() {
            if cache.iter().any(|e| e.eq_ignore_ascii_case(v)) == false {
                cache.push(v.clone());
            }
        }
        if cache.len() > 200 {
            cache.drain(0..cache.len() - 200);
        }
        out
    }

    /// 取当前请求里某张表第 index 行的 key（value 框据此联想历史值）。
    fn kv_key_at(&self, which: KvWhich, index: usize) -> Option<String> {
        let r = self.selected_request()?;
        match which {
            KvWhich::Params => r.params.get(index).map(|k| k.key.clone()),
            KvWhich::Headers => r.headers.get(index).map(|k| k.key.clone()),
            KvWhich::FormData => r.body.form_data.get(index).map(|f| f.key.clone()),
        }
    }

    /// 把 KV 的 key / value 框候选同步给输入框（只在变化时写，避免每帧无谓更新）。
    pub fn sync_kv_suggest(&mut self, cx: &mut Context<Self>) {
        let req_id = match &self.selection {
            Some(Selection::Request(id)) => id.clone(),
            _ => String::new(),
        };
        // key 候选按表算一次；value 候选按「表 + key」算一次，同名列共用
        let mut key_cache: [Option<Vec<String>>; 3] = [None, None, None];
        let mut value_cache: Vec<(String, Vec<String>)> = Vec::new();
        let fields: Vec<(String, Entity<TextField>)> = self
            .fields
            .kv
            .iter()
            .map(|(k, f)| (k.clone(), f.clone()))
            .collect();
        for (field_key, field) in fields {
            let parts: Vec<&str> = field_key.split(':').collect();
            if parts.len() != 4 {
                continue;
            }
            let which = match parts[0] {
                "P" => KvWhich::Params,
                "FD" => KvWhich::FormData,
                _ => KvWhich::Headers,
            };
            let slot = match which {
                KvWhich::Params => 0,
                KvWhich::Headers => 1,
                KvWhich::FormData => 2,
            };
            let index: usize = parts[2].parse().unwrap_or(0);
            let cands: Vec<String> = match parts[3] {
                "key" => {
                    if key_cache[slot].is_none() {
                        key_cache[slot] = Some(self.kv_key_candidates(which));
                    }
                    key_cache[slot].clone().unwrap_or_default()
                }
                "value" => {
                    // 只在当前请求里联想；key 还没填就没法猜是什么值
                    let mut c: Vec<String> = Vec::new();
                    if parts[1] == req_id.as_str()
                        && let Some(k) = self.kv_key_at(which, index)
                        && k.trim().len() > 0
                    {
                        let ck = parts[0].to_string() + "|" + &k.trim().to_lowercase();
                        if let Some(pos) = value_cache.iter().position(|(n, _)| *n == ck) {
                            c = value_cache[pos].1.clone();
                        } else {
                            c = self.kv_value_candidates(which, &k);
                            value_cache.push((ck, c.clone()));
                        }
                    }
                    c
                }
                _ => continue,
            };
            field.update(cx, |f, _cx| {
                if f.plain_candidates != cands {
                    f.plain_candidates = cands;
                }
            });
        }
    }

    /// 所有可能补全的输入框（URL / Body / KV / 认证），顺序即优先级。
    fn suggest_fields(&self) -> Vec<Entity<TextField>> {
        let mut fields: Vec<Entity<TextField>> = Vec::new();
        if let Some(f) = &self.fields.url {
            fields.push(f.clone());
        }
        if let Some(f) = &self.fields.body {
            fields.push(f.clone());
        }
        fields.extend(self.fields.kv.values().cloned());
        fields.extend(self.fields.auth.values().cloned());
        fields
    }

    /// 当前正在补全、且仍聚焦的输入框。切页 / 点到别处后旧框不再重绘，
    /// 光看 suggest 会残留浮层，所以这里必须再确认焦点。
    pub(crate) fn suggest_target(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<TextField>> {
        self.suggest_fields().into_iter().find(|f| {
            let f = f.read(cx);
            f.suggest.is_some() && f.focus_handle.is_focused(window)
        })
    }

    /// 补全浮层：贴着输入框下沿弹出（根节点末尾绘制，天然在最上层）。
    pub(crate) fn suggest_overlay(
        &self,
        field: Entity<TextField>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (items, selected, bounds, kind) = {
            let f = field.read(cx);
            let Some(sg) = &f.suggest else {
                return div().into_any();
            };
            (sg.items.clone(), sg.selected, f.last_bounds, sg.kind)
        };
        let Some(bounds) = bounds else {
            return div().into_any();
        };
        let scale = crate::settings::suggest_scale(&self.settings);
        let vw = window.viewport_size().width.to_f64() as f32;
        let vh = window.viewport_size().height.to_f64() as f32;
        let w = 240.0_f32 * scale;
        let row_h = 26.0_f32 * scale;
        let h = items.len() as f32 * row_h + row_h;
        let x = (bounds.origin.x.to_f64() as f32).min(vw - w - 6.0).max(6.0);
        let y = (bounds.origin.y.to_f64() as f32 + bounds.size.height.to_f64() as f32 + 2.0)
            .min(vh - h - 6.0)
            .max(6.0);
        let mut menu = div()
            .id("suggest")
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .flex()
            .flex_col()
            .bg(theme::bg_popup())
            .border_1()
            .border_color(theme::border_strong())
            .rounded(px(4.))
            .py(theme::sp1())
            .shadow_lg();
        for (i, name) in items.iter().enumerate() {
            let active = i == selected;
            let field_for_click = field.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("suggest/{}", name)))
                    .px(theme::sp3())
                    .h(px(row_h))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .font(theme::mono())
                    .text_size(px(theme::mono_size() * scale))
                    .when(active, |d| d.bg(theme::bg_hover()))
                    .text_color(if active {
                        theme::fg_bright()
                    } else {
                        theme::fg_normal()
                    })
                    .truncate()
                    .child(SharedString::from(name.clone()))
                    .on_mouse_down(
                        MouseButton::Left,
                        move |_e: &MouseDownEvent, _w: &mut Window, app: &mut App| {
                            field_for_click.update(app, |f, cx| {
                                if let Some(sg) = &mut f.suggest {
                                    sg.selected = i;
                                }
                                f.accept_suggest(cx);
                            });
                        },
                    ),
            );
        }
        menu.child(
            div()
                .px(theme::sp3())
                .h(px(row_h))
                .flex()
                .items_center()
                .text_size(px((theme::font_small() - 1.) * scale))
                .text_color(theme::fg_dark())
                .child(self.t(if kind == SuggestKind::Plain {
                    "kv.complete_hint"
                } else {
                    "vars.complete_hint"
                })),
        )
        .into_any()
    }

    /// Esc：关闭最上层浮层。草稿（导入文本 / 环境变量 / 搜索词）在关闭前存回 model，
    /// 再次打开时原样恢复，误按 Esc 不会丢输入。
    pub fn close_overlay(&mut self, cx: &mut Context<Self>) {
        if let Some(qu) = &self.quick {
            self.quick_query = qu.query.clone();
            self.quick = None;
        } else if self.suggest_fields().iter().any(|f| f.read(cx).suggest.is_some()) {
            // 输入框里的补全优先关：所有框上的浮层都关掉
            for f in self.suggest_fields() {
                f.update(cx, |f, cx| f.dismiss_suggest(cx));
            }
            return;
        } else if self.popup.open {
            self.popup.close();
        } else if self.save_var_dialog.is_some() {
            self.close_save_var_dialog(cx);
            return;
        } else if self.import_dialog.is_some() {
            self.close_import_dialog(cx);
            return;
        } else if self.vars_panel {
            self.close_vars_panel(cx);
            return;
        } else if self.env_editor.is_some() {
            self.env_cancel(cx);
            return;
        } else if self.trash_dialog {
            self.trash_dialog = false;
        } else if self.codegen.is_some() {
            self.codegen = None;
            self.codegen_pin = None;
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn close_quick(&mut self, cx: &mut Context<Self>) {
        self.close_overlay(cx);
    }

    pub fn quick_set_query(&mut self, q: String, cx: &mut Context<Self>) {
        if let Some(qu) = &mut self.quick {
            qu.query = q;
            qu.selected = 0;
        }
        self.quick_rebuild();
        cx.notify();
    }

    /// 重建面板列表：历史（带状态码）在前，请求在后；按关键字过滤。
    pub(crate) fn quick_rebuild(&mut self) {
        let q = self
            .quick
            .as_ref()
            .map(|q| q.query.trim().to_lowercase())
            .unwrap_or_default();
        let mut list: Vec<(QuickTarget, String)> = vec![];
        for h in &self.recent_all {
            if !q.is_empty()
                && !h.request.name.to_lowercase().contains(&q)
                && !h.request.url.to_lowercase().contains(&q)
                && !h.request.method.to_lowercase().contains(&q)
            {
                continue;
            }
            let status = h
                .status
                .map(|s| s.to_string())
                .or_else(|| h.error.as_ref().map(|_| "✗".to_string()))
                .unwrap_or_default();
            let label = format!(
                "{}  {}  —  {}  ...  {}",
                h.request.method,
                h.request.name,
                truncate_url(&h.request.url, 50),
                status
            );
            list.push((QuickTarget::History(h.id), label));
            if list.len() >= 30 {
                break;
            }
        }
        if list.len() < 60 {
            let current = self.workspace_path();
            // 跨工作区命中：当前区优先（逻辑在 search::rank_hits，有单测）
            let ranked = crate::search::rank_hits(&self.cross_index, &current, &q, 60 - list.len());
            for i in ranked {
                let hit = &self.cross_index[i];
                let r = &hit.request;
                let scope = if hit.workspace == current {
                    hit.container.clone()
                } else {
                    format!("{} › {}", hit.workspace_name, hit.container)
                };
                let label = format!(
                    "{}  {}  —  {}  ({})",
                    r.method,
                    r.name,
                    truncate_url(&r.url, 44),
                    scope
                );
                list.push((
                    QuickTarget::CrossRequest {
                        workspace: hit.workspace.clone(),
                        request: r.id.clone(),
                    },
                    label,
                ));
            }
        }
        if let Some(qu) = &mut self.quick {
            qu.list = list;
            qu.selected = qu.selected.min(qu.list.len().saturating_sub(1));
        }
    }

    /// 指定 id 的请求是否仍存在（历史跳转用）。
    pub(crate) fn request_exists(&self, id: &str) -> bool {
        self.workspace.collections.iter().any(|c| {
            c.requests.iter().any(|r| r.id == id)
                || c.groups
                    .iter()
                    .any(|g| g.requests.iter().any(|r| r.id == id))
        })
    }

    /// 扁平的 (集合名, &请求, 分组名 Option) 列表。
    pub(crate) fn all_requests(&self) -> Vec<(&str, &RequestItem, Option<&str>)> {
        let mut out = vec![];
        for c in &self.workspace.collections {
            for r in &c.requests {
                out.push((c.name.as_str(), r, None));
            }
            for g in &c.groups {
                for r in &g.requests {
                    out.push((c.name.as_str(), r, Some(g.name.as_str())));
                }
            }
        }
        out
    }

    pub fn quick_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.quick.is_none() && self.move_dialog.is_some() {
            self.move_move(delta, cx);
            return;
        }
        if let Some(qu) = &mut self.quick
            && !qu.list.is_empty()
        {
            let n = qu.list.len() as isize;
            qu.selected = ((qu.selected as isize + delta).rem_euclid(n)) as usize;
        }
        cx.notify();
    }

    pub fn quick_pick_selected(&mut self, cx: &mut Context<Self>) {
        let Some((target, _)) = self
            .quick
            .as_ref()
            .and_then(|q| q.list.get(q.selected).cloned())
        else {
            return;
        };
        self.quick = None;
        match target {
            QuickTarget::CrossRequest { workspace, request } => {
                if workspace != self.settings.workspace_root {
                    self.settings.set_active_workspace(workspace);
                    settings::save(&self.settings).ok();
                    self.store = WorkspaceStore::new(self.settings.workspace_root.clone());
                    self.fields = EditorFields::new();
                    self.reload();
                }
                self.reveal_request(request, cx);
            }
            QuickTarget::History(hid) => {
                if let Some(h) = self.recent_all.iter().find(|h| h.id == hid).cloned() {
                    // 命令面板是导航面板：优先跳到历史对应的原请求，不要覆盖当前请求；
                    // 原请求已删除时用快照新建一个请求，同样不覆盖当前选中的请求。
                    if self.request_exists(&h.request.id) {
                        self.reveal_request(h.request.id.clone(), cx);
                    } else {
                        // 原请求已删除：新建一个请求，也不覆盖当前选中的请求
                        self.create_request_from_snapshot(&h.request);
                    }
                }
            }
        }
        cx.notify();
    }

    pub fn quick_set_selected(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(qu) = &mut self.quick {
            qu.selected = i;
        }
        cx.notify();
    }

    /// 渲染命令面板（覆盖在根节点上层）。
    pub(crate) fn quick_render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(qu) = &self.quick else {
            return div().into_any();
        };
        let field = qu.field.clone();
        // 打开后自动聚焦输入框（渲染阶段做，actor 没有 window）
        if !field.read(cx).focus_handle.is_focused(window) {
            field.update(cx, |f, _| f.focus_handle.focus(window));
        }
        let list = qu.list.clone();
        let selected = qu.selected;
        let w = window.bounds().size.width.to_f64() as f32;
        let panel_w = 560f32.min(w - 80.0);
        let panel_x = ((w - panel_w) / 2.0).max(20.0);
        let bar_quick_rows = self.scroll_handle("quick-rows");
        let mut rows = div()
            .id("quick-rows")
            .flex_col()
            .py(theme::sp2())
            .overflow_scroll()
            .track_scroll(&bar_quick_rows)
            .max_h(px(360.));
        for (i, (_, label)) in list.iter().enumerate() {
            let i_clone = i;
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("quick/{}", i)))
                    .h(theme::row_h())
                    .flex()
                    .items_center()
                    .px(theme::sp4())
                    .cursor_pointer()
                    .text_size(px(theme::font_body()))
                    .font(theme::mono())
                    .when(i == selected, |d| {
                        d.relative()
                            .bg(theme::bg_selected_accent())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.))
                                    .top(px(0.))
                                    .h_full()
                                    .w(px(2.))
                                    .bg(theme::primary()),
                            )
                    })
                    .when(i != selected, |d| d.hover(|d| d.bg(theme::bg_hover())))
                    .text_color(if i == selected {
                        theme::fg_white()
                    } else {
                        theme::fg_normal()
                    })
                    .truncate()
                    .child(SharedString::from(label))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _w, cx| {
                            this.quick_set_selected(i_clone, cx);
                            this.quick_pick_selected(cx);
                        }),
                    ),
            );
        }
        if list.is_empty() {
            rows = rows.child(
                div()
                    .px(theme::sp4())
                    .py(theme::sp3())
                    .text_size(px(theme::font_body()))
                    .text_color(theme::fg_dark())
                    .child(self.t("quick.empty")),
            );
        }
        div()
            .absolute()
            .left(px(panel_x))
            .top(px(80.))
            .w(px(panel_w))
            .bg(theme::bg_popup())
            .border_1()
            .border_color(theme::border_strong())
            .rounded_lg()
            .shadow_lg()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(theme::sp4())
                    .pt(theme::sp3())
                    .pb(theme::sp2())
                    .child(field.clone()),
            )
            .child(
                div()
                    .px(theme::sp4())
                    .pb(theme::sp2())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::fg_dark())
                    .child(self.t("quick.hint_all_ws")),
            )
            .child(div().border_t_1().border_color(theme::border()).child(rows))
            .into_any()
    }
}
