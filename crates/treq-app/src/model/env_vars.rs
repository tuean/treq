//! 环境变量面板与环境编辑器（含回收站对话框）

use super::*;

impl AppModel {
    // ---- 变量面板 ----

    /// 合并后的变量（base + 当前环境），按名字排序；第三个字段是「当前环境里也定义了」。
    pub fn env_var_rows(&self) -> Vec<(String, String, bool)> {
        let active = self.active_env().map(|e| e.variables.clone());
        vars::merge_env(&self.workspace.base_env, self.active_env())
            .into_iter()
            .map(|(k, v)| {
                let overridden = active.as_ref().is_some_and(|a| a.contains_key(&k));
                (k, v, overridden)
            })
            .collect()
    }

    /// 当前请求用到、环境里却没有的变量（没选请求就是空）。
    pub fn missing_vars(&self) -> Vec<String> {
        let Some(req) = self.selected_request() else {
            return Vec::new();
        };
        vars::undefined_vars(
            req,
            &vars::merge_env(&self.workspace.base_env, self.active_env()),
        )
    }

    pub fn open_vars_panel(&mut self, cx: &mut Context<Self>) {
        self.vars_panel = true;
        self.popup.close();
        cx.notify();
    }

    pub fn close_vars_panel(&mut self, cx: &mut Context<Self>) {
        self.vars_panel = false;
        cx.notify();
    }

    pub fn toggle_vars_reveal(&mut self, cx: &mut Context<Self>) {
        self.vars_reveal = !self.vars_reveal;
        cx.notify();
    }

    /// 把缺失的变量补进当前环境（没有活动环境就补进 base），空值待填。
    pub fn add_var_to_env(&mut self, name: String, cx: &mut Context<Self>) {
        match self.settings.active_environment_id.clone() {
            Some(id) => {
                if let Some(env) = self.workspace.environments.iter_mut().find(|e| e.id == id) {
                    env.variables.entry(name).or_default();
                    let _ = self.store.save_environment(env);
                }
            }
            None => {
                self.workspace.base_env.variables.entry(name).or_default();
                let _ = self.store.save_environment(&self.workspace.base_env);
            }
        }
        cx.notify();
    }

    pub fn open_env_editor(&mut self, target: &str, cx: &mut Context<Self>) {
        // 上次未保存的编辑优先恢复（Esc 关掉不丢输入）
        if let Some(draft) = self.env_drafts.get(target).cloned() {
            self.rebuild_env_fields(target.to_string(), draft, cx);
            cx.notify();
            return;
        }
        let (name, vars) = if target == "base" {
            (
                self.workspace.base_env.name.clone(),
                self.workspace
                    .base_env
                    .variables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>(),
            )
        } else {
            match self.workspace.environments.iter().find(|e| e.id == target) {
                Some(e) => (
                    e.name.clone(),
                    e.variables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<Vec<_>>(),
                ),
                None => return,
            }
        };
        let handle = cx.entity();
        let target_owned = target.to_string();
        let mut fields = Vec::new();
        for (i, (k, v)) in vars.iter().enumerate() {
            let idx = i;
            let t = target_owned.clone();
            let h = handle.clone();
            let k_field = TextField::new(
                k.clone().into(),
                SharedString::from(""),
                Arc::new(move |s, app| {
                    h.update(app, |this, cx| {
                        this.env_set_key_at(t.clone(), idx, s.to_string(), cx)
                    });
                }),
                cx,
            );
            let idx2 = i;
            let t2 = target_owned.clone();
            let h2 = handle.clone();
            let v_field = TextField::new(
                v.clone().into(),
                SharedString::from(""),
                Arc::new(move |s, app| {
                    h2.update(app, |this, cx| {
                        this.env_set_value_at(t2.clone(), idx2, s.to_string(), cx)
                    });
                }),
                cx,
            );
            fields.push((k_field, v_field));
        }
        self.env_editor = Some(EnvEditor {
            target: target.to_string(),
            vars,
            fields,
        });
        let _ = name;
        cx.notify();
    }

    pub fn env_set_key_at(
        &mut self,
        target: String,
        index: usize,
        key: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(ed) = &mut self.env_editor {
            if ed.target != target {
                return;
            }
            if let Some((k, _)) = ed.vars.get_mut(index) {
                *k = key;
            }
        }
        cx.notify();
    }

    pub fn env_set_value_at(
        &mut self,
        target: String,
        index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(ed) = &mut self.env_editor {
            if ed.target != target {
                return;
            }
            if let Some((_, v)) = ed.vars.get_mut(index) {
                *v = value;
            }
        }
        cx.notify();
    }

    pub fn env_add_var(&mut self, cx: &mut Context<Self>) {
        if let Some(ed) = &mut self.env_editor {
            ed.vars.push((String::new(), String::new()));
            // 需要一个新行实体：直接重建编辑器（字段少，可接受）
            let target = ed.target.clone();
            let vars = ed.vars.clone();
            self.rebuild_env_fields(target, vars, cx);
        }
        cx.notify();
    }

    pub fn env_remove_var(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(ed) = &mut self.env_editor {
            if index < ed.vars.len() {
                ed.vars.remove(index);
            }
            let target = ed.target.clone();
            let vars = ed.vars.clone();
            self.rebuild_env_fields(target, vars, cx);
        }
        cx.notify();
    }

    pub fn open_trash_dialog(&mut self, cx: &mut Context<Self>) {
        self.trash_dialog = true;
        self.trash_confirm = false;
        cx.notify();
    }

    pub fn close_trash_dialog(&mut self, cx: &mut Context<Self>) {
        self.trash_dialog = false;
        self.trash_confirm = false;
        cx.notify();
    }

    /// 恢复回收站条目（集合 / 分组 / 请求）。
    pub fn trash_restore(
        &mut self,
        col_id: String,
        group_id: Option<String>,
        req_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let _ = self.trash_store.restore(
            self.store.root(),
            &col_id,
            group_id.as_deref(),
            req_id.as_deref(),
        );
        self.reload();
        cx.notify();
    }

    /// 清空回收站：第一次点只是待确认，再点一次才真清（不可恢复）。
    pub fn trash_empty(&mut self, cx: &mut Context<Self>) {
        if !self.trash_confirm {
            self.trash_confirm = true;
            cx.notify();
            return;
        }
        let _ = self.trash_store.empty();
        self.trash_confirm = false;
        cx.notify();
    }

    pub(crate) fn rebuild_env_fields(
        &mut self,
        target: String,
        vars: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        let handle = cx.entity();
        let target2 = target.clone();
        let mut fields = Vec::new();
        for (i, (k, v)) in vars.iter().enumerate() {
            let idx = i;
            let t = target2.clone();
            let h = handle.clone();
            let k_field = TextField::new(
                k.clone().into(),
                SharedString::from(""),
                Arc::new(move |s, app| {
                    h.update(app, |this, cx| {
                        this.env_set_key_at(t.clone(), idx, s.to_string(), cx)
                    });
                }),
                cx,
            );
            let idx2 = i;
            let t2 = target2.clone();
            let h2 = handle.clone();
            let v_field = TextField::new(
                v.clone().into(),
                SharedString::from(""),
                Arc::new(move |s, app| {
                    h2.update(app, |this, cx| {
                        this.env_set_value_at(t2.clone(), idx2, s.to_string(), cx)
                    });
                }),
                cx,
            );
            fields.push((k_field, v_field));
        }
        self.env_editor = Some(EnvEditor {
            target,
            vars,
            fields,
        });
    }

    pub fn env_save(&mut self, cx: &mut Context<Self>) {
        let Some(ed) = &self.env_editor else { return };
        let target = ed.target.clone();
        let vars: std::collections::BTreeMap<String, String> = ed
            .vars
            .iter()
            .filter(|(k, _)| !k.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if target == "base" {
            let mut env = self.workspace.base_env.clone();
            env.variables = vars;
            let _ = self.store.save_environment(&env);
            self.workspace.base_env = env;
        } else if let Some(e) = self
            .workspace
            .environments
            .iter_mut()
            .find(|e| e.id == target)
        {
            let mut env = e.clone();
            env.variables = vars;
            let _ = self.store.save_environment(&env);
            *e = env;
        }
        self.env_drafts.remove(&target);
        self.env_editor = None;
        self.update_preview(cx);
        cx.notify();
    }

    pub fn env_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(ed) = &self.env_editor {
            self.env_drafts.insert(ed.target.clone(), ed.vars.clone());
        }
        self.env_editor = None;
        cx.notify();
    }

    pub fn set_active_env(&mut self, env_id: &str, cx: &mut Context<Self>) {
        self.settings.active_environment_id = if env_id == "base" {
            None
        } else {
            Some(env_id.to_string())
        };
        settings::save(&self.settings).ok();
        self.update_preview(cx);
    }
}
