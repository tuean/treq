//! 认证表单：类型选择 + 两个字段（含义随类型变）+ API Key 位置

use super::*;

impl AppModel {
    /// 认证表单：类型选择 + 两个字段（含义随类型变）+ API Key 位置。
    pub(crate) fn auth_form(&mut self, req: &RequestItem, cx: &mut Context<Self>) -> AnyElement {
        use treq_core::auth::{Auth, AuthField, KeyIn};
        let auth = req.auth.clone().unwrap_or(Auth::None);
        let kind = auth.kind_id().to_string();
        if self.fields.auth_kind.as_deref() != Some(kind.as_str()) {
            self.fields.auth.clear();
            self.fields.auth_kind = Some(kind.clone());
        }
        let label = |t: String| {
            div()
                .w(px(96.))
                .flex_none()
                .text_size(px(theme::font_small()))
                .text_color(theme::fg_dim())
                .child(SharedString::from(t))
        };
        let field = |this: &mut Self,
                     slot: &str,
                     value: String,
                     placeholder: &str,
                     label_text: String,
                     cx: &mut Context<Self>| {
            let key = format!("{}:{}", kind, slot);
            let f = match this.fields.auth.get(&key) {
                Some(f) => f.clone(),
                None => {
                    let handle = cx.entity();
                    let slot_owned = slot.to_string();
                    let f = TextField::new(
                        value.into(),
                        SharedString::from(placeholder.to_string()),
                        Arc::new(move |s, app| {
                            handle.update(app, |this, cx| {
                                this.auth_set(&slot_owned, s.to_string(), cx);
                            });
                        }),
                        cx,
                    );
                    f.update(cx, |f, _cx| f.plain = true);
                    this.fields.auth.insert(key, f.clone());
                    f
                }
            };
            div()
                .flex()
                .items_center()
                .gap(theme::sp3())
                .child(label(label_text))
                .child(div().flex_1().min_w_0().child(f))
                .into_any()
        };

        let mut rows = div().flex().flex_col().gap(theme::sp3()).child(
            div()
                .flex()
                .items_center()
                .gap(theme::sp2())
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::fg_dim())
                        .child(self.t("auth.kind")),
                )
                .children(["none", "bearer", "basic", "apikey"].into_iter().map(|k| {
                    let active = kind == k;
                    let id = format!("auth-kind-{}", k);
                    let text: SharedString = self.t(auth_kind_key(k)).into();
                    div()
                        .id(SharedString::from(id))
                        .h(theme::control_h())
                        .flex()
                        .items_center()
                        .px(theme::sp3())
                        .text_size(px(theme::font_body()))
                        .rounded_md()
                        .cursor_pointer()
                        .when(active, |d| {
                            d.bg(theme::bg_accent()).text_color(theme::fg_on_accent())
                        })
                        .when(!active, |d| {
                            d.text_color(theme::fg_dim())
                                .hover(|d| d.text_color(theme::fg_normal()))
                        })
                        .child(text)
                        .on_click(cx.listener(move |this, _, _w, cx| {
                            this.set_auth_kind(k, cx);
                        }))
                })),
        );

        match &auth {
            Auth::None => {}
            Auth::Bearer { .. } => {
                rows = rows
                    .child(field(
                        self,
                        "a",
                        auth.field(AuthField::A).to_string(),
                        "{{ token }}",
                        self.t("auth.token").to_string(),
                        cx,
                    ))
                    .child(field(
                        self,
                        "b",
                        auth.field(AuthField::B).to_string(),
                        "Bearer",
                        self.t("auth.prefix").to_string(),
                        cx,
                    ));
            }
            Auth::Basic { .. } => {
                rows = rows
                    .child(field(
                        self,
                        "a",
                        auth.field(AuthField::A).to_string(),
                        "user",
                        self.t("auth.username").to_string(),
                        cx,
                    ))
                    .child(field(
                        self,
                        "b",
                        auth.field(AuthField::B).to_string(),
                        "password",
                        self.t("auth.password").to_string(),
                        cx,
                    ));
            }
            Auth::ApiKey { location, .. } => {
                rows = rows
                    .child(field(
                        self,
                        "a",
                        auth.field(AuthField::A).to_string(),
                        "X-Api-Key",
                        self.t("auth.keyname").to_string(),
                        cx,
                    ))
                    .child(field(
                        self,
                        "b",
                        auth.field(AuthField::B).to_string(),
                        "{{ api_key }}",
                        self.t("auth.keyvalue").to_string(),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::sp2())
                            .child(label(self.t("auth.in").to_string()))
                            .children([KeyIn::Header, KeyIn::Query].into_iter().map(|loc| {
                                let active = *location == loc;
                                let (id, text) = match loc {
                                    KeyIn::Header => ("auth-in-header", self.t("auth.in_header")),
                                    KeyIn::Query => ("auth-in-query", self.t("auth.in_query")),
                                };
                                div()
                                    .id(id)
                                    .h(theme::control_h())
                                    .flex()
                                    .items_center()
                                    .px(theme::sp3())
                                    .text_size(px(theme::font_body()))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .when(active, |d| {
                                        d.bg(theme::bg_accent()).text_color(theme::fg_on_accent())
                                    })
                                    .when(!active, |d| {
                                        d.text_color(theme::fg_dim())
                                            .hover(|d| d.text_color(theme::fg_normal()))
                                    })
                                    .child(SharedString::from(text.to_string()))
                                    .on_click(cx.listener(move |this, _, _w, cx| {
                                        this.set_auth_key_in(loc, cx);
                                    }))
                            })),
                    );
            }
        }

        let mut card = div().flex().flex_col().gap(theme::sp3()).child(rows).child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::fg_dim())
                .child(self.t("auth.hint")),
        );
        if !auth.is_none() {
            card = card.child(
                div()
                    .font(theme::mono())
                    .text_size(px(theme::font_small()))
                    .text_color(theme::json_key())
                    .child(SharedString::from(format!(
                        "{}{}",
                        self.t("auth.summary"),
                        auth.summary()
                    ))),
            );
        }
        if self.has_manual_auth_header() {
            card = card.child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::orange())
                    .child(self.t("auth.manual_wins")),
            );
        }
        div()
            .p(theme::sp4())
            .flex()
            .flex_col()
            .gap(theme::sp3())
            .child(widgets::section_label(self.t("editor.tab.auth")))
            .child(card)
            .into_any()
    }
}
