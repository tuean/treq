//! 把响应里的值存成环境变量（请求链）

use super::*;

impl AppModel {
    // ---- 把响应值存成环境变量 ----

    /// 打开「存为变量」：路径默认用响应过滤条里已经写好的 JSONPath（顺手），
    /// 变量名按路径最后一段猜。
    pub fn open_save_var_dialog(&mut self, cx: &mut Context<Self>) {
        self.open_save_var_dialog_at(None, cx);
    }

    /// 同上，但可以指定路径（响应行右键带过来）。
    pub fn open_save_var_dialog_at(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        let default_path = path.unwrap_or_else(|| {
            if self.resp_filter.trim().is_empty() {
                "$".to_string()
            } else {
                self.resp_filter.trim().to_string()
            }
        });
        let default_name = treq_core::vars::var_name_from_path(&default_path);
        let path = self.var_field(default_path, "$.data.token", false, cx);
        let name = self.var_field(default_name, "token", true, cx);
        self.save_var_dialog = Some(SaveVarDialog {
            source: VarSource::Body,
            path,
            name,
            error: None,
        });
        cx.notify();
    }

    /// 对话框里的输入框（回车 = 保存）。
    pub(crate) fn var_field(
        &self,
        text: String,
        placeholder: &str,
        is_name: bool,
        cx: &mut Context<Self>,
    ) -> Entity<TextField> {
        let handle = cx.entity();
        let n = 0;
        let f = TextField::new(
            text.into(),
            SharedString::from(placeholder.to_string()),
            Arc::new(move |_, app| {
                // 输入即重算预览
                handle.update(app, |this, cx| {
                    if let Some(dlg) = &mut this.save_var_dialog {
                        dlg.error = None;
                        cx.notify();
                    }
                });
            }),
            cx,
        );
        f.update(cx, |f, _cx| f.plain = true);
        let handle2 = cx.entity();
        f.update(cx, |f, _cx| {
            f.on_submit = Some(Arc::new(move |_w, app| {
                // confirm_save_var 要读这两个输入框 → 不能在它们自己的 update 里读，推迟一帧
                let handle = handle2.clone();
                app.defer(move |app| {
                    handle.update(app, |this, cx| this.confirm_save_var(cx));
                });
            }));
        });
        let _ = (is_name, n);
        f
    }

    pub fn set_var_source(&mut self, source: VarSource, cx: &mut Context<Self>) {
        if let Some(dlg) = &mut self.save_var_dialog {
            dlg.source = source;
            dlg.error = None;
        }
        cx.notify();
    }

    /// 当前设置的取值结果（界面预览与保存共用；不命中就是错误信息）。
    pub fn save_var_preview(&self, cx: &mut Context<Self>) -> Result<(String, usize), String> {
        let Some(dlg) = &self.save_var_dialog else {
            return Err(String::new());
        };
        let path = dlg.path.read(cx).content.to_string();
        let resp = self
            .response
            .as_ref()
            .ok_or_else(|| self.t("vars.no_response").to_string())?;
        match dlg.source {
            VarSource::Body => treq_core::vars::value_from_json(&resp.body, &path),
            VarSource::Header => {
                treq_core::vars::value_from_header(&resp.headers, &path).map(|v| (v, 1))
            }
        }
    }

    /// 校验变量名：非空、不含空白与花括号（`{{ x }}` 里带空格会解析不出来）。
    pub(crate) fn check_var_name(name: &str) -> Result<(), String> {
        let n = name.trim();
        if n.is_empty() {
            return Err("变量名不能为空".into());
        }
        if n.contains(char::is_whitespace) || n.contains('{') || n.contains('}') {
            return Err("变量名不能带空格或花括号".into());
        }
        Ok(())
    }

    /// 写进环境：有活动环境写活动环境，否则写 Base（与「补进环境」一致）。
    pub(crate) fn set_env_var(&mut self, name: &str, value: &str) -> String {
        match self.settings.active_environment_id.clone() {
            Some(id) => {
                if let Some(env) = self.workspace.environments.iter_mut().find(|e| e.id == id) {
                    env.variables.insert(name.to_string(), value.to_string());
                    let _ = self.store.save_environment(env);
                    return env.name.clone();
                }
                self.workspace
                    .base_env
                    .variables
                    .insert(name.to_string(), value.to_string());
                let _ = self.store.save_environment(&self.workspace.base_env);
                self.workspace.base_env.name.clone()
            }
            None => {
                self.workspace
                    .base_env
                    .variables
                    .insert(name.to_string(), value.to_string());
                let _ = self.store.save_environment(&self.workspace.base_env);
                self.workspace.base_env.name.clone()
            }
        }
    }

    /// 目标环境名（对话框里显示「将写入：X」）。
    pub fn save_var_target(&self) -> String {
        match self.settings.active_environment_id.clone() {
            Some(id) => self
                .workspace
                .environments
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| self.workspace.base_env.name.clone()),
            None => self.workspace.base_env.name.clone(),
        }
    }

    /// 这个名字在当前环境里已经有了吗（提醒会覆盖）。
    pub fn save_var_exists(&self, name: &str) -> bool {
        let vars = vars::merge_env(&self.workspace.base_env, self.active_env());
        vars.contains_key(name)
    }

    pub fn close_save_var_dialog(&mut self, cx: &mut Context<Self>) {
        self.save_var_dialog = None;
        cx.notify();
    }

    pub fn confirm_save_var(&mut self, cx: &mut Context<Self>) {
        let Some(dlg) = &self.save_var_dialog else {
            return;
        };
        let name = dlg.name.read(cx).content.to_string();
        let name = name.trim().to_string();
        if let Err(e) = Self::check_var_name(&name) {
            if let Some(dlg) = &mut self.save_var_dialog {
                dlg.error = Some(e);
            }
            cx.notify();
            return;
        }
        let value = match self.save_var_preview(cx) {
            Ok((v, _)) => v,
            Err(e) => {
                if let Some(dlg) = &mut self.save_var_dialog {
                    dlg.error = Some(e);
                }
                cx.notify();
                return;
            }
        };
        let env_name = self.set_env_var(&name, &value);
        self.toast(
            format!(
                "{}{}{}{}",
                self.t("vars.saved"),
                name,
                self.t("vars.saved_to"),
                env_name
            ),
            cx,
        );
        self.save_var_dialog = None;
        cx.notify();
    }
}

#[cfg(test)]
mod var_name_tests {
    use crate::model::AppModel;

    #[test]
    fn rejects_empty_whitespace_and_braces() {
        assert!(AppModel::check_var_name("token").is_ok());
        assert!(AppModel::check_var_name("  api_key_2  ").is_ok(), "两侧空格会被 trim");
        assert!(AppModel::check_var_name("").is_err());
        assert!(AppModel::check_var_name("   ").is_err());
        assert!(AppModel::check_var_name("has space").is_err());
        assert!(AppModel::check_var_name("{{x}}").is_err());
        assert!(AppModel::check_var_name("x}").is_err());
        assert!(AppModel::check_var_name("中 文").is_err());
        assert!(AppModel::check_var_name("中文变量").is_ok(), "中文名本身可以用");
    }
}
