//! 长值弹框：Params/Headers/Form-Data 的 value 太长时，弹框里换行显示全文，并且**能直接改**。
//!
//! 单行输入框放不下长值（TextField 不折行，只横向裁），所以给个「展开」按钮看全文。
//! 上半部分只看：选中逻辑跟响应区共用一套（RespSel 行 + 字节列 / join_resp_selection 出文本），
//! 只是行集换成弹框里那几行；下半部分是多行编辑框，改一个字就实时写回请求（走 kv_set_any）。

use super::response_ui::join_resp_selection;
use super::*;

/// 长值弹框的状态。
pub struct KvZoom {
    pub title: String,
    /// 回写用的行内字段键（`P:{req}:{idx}:value` 等），kv_set_any 认这个
    pub field_key: String,
    /// 当前值：预览区 + 复制用，随编辑实时更新
    pub text: String,
    /// 编辑框（多行：值里带 \n 也能改）
    pub field: Entity<TextField>,
    pub sel: Option<RespSel>,
    pub(crate) drag: bool,
    /// 最近一次鼠标悬停的字节列（按下时拿它当起点）
    hover_col: Option<usize>,
    /// 预览区焦点：⌘C 复制
    pub focus: FocusHandle,
}

/// 按 \n 切行（预览区一行一个 InteractiveText）。
pub fn kv_zoom_lines(text: &str) -> Vec<String> {
    text.split('\n').map(|s| s.to_string()).collect()
}

impl AppModel {
    pub fn open_kv_zoom(
        &mut self,
        title: String,
        field_key: String,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        focus.focus(window);
        let handle = cx.entity();
        let field = TextField::new(
            text.clone().into(),
            SharedString::from(""),
            Arc::new(move |s, app| {
                handle.update(app, |this, cx| this.kv_zoom_edited(s.to_string(), cx));
            }),
            cx,
        );
        field.update(cx, |f, _cx| {
            f.plain = true;
            f.multiline = true;
            f.auto_grow = true;
            f.line_height = Some(px(crate::theme::line_h()));
        });
        self.kv_zoom = Some(KvZoom {
            title,
            field_key,
            text,
            field,
            sel: None,
            drag: false,
            hover_col: None,
            focus,
        });
        cx.notify();
    }

    pub fn close_kv_zoom(&mut self, cx: &mut Context<Self>) {
        if self.kv_zoom.take().is_some() {
            cx.notify();
        }
    }

    /// 编辑框每敲一下：写回请求 + 刷新预览。行内那个框缓存着旧文本，
    /// 删掉让它在下一帧按模型重建（不然用户再去行里编辑会用旧文本盖掉新值）。
    fn kv_zoom_edited(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(key) = self.kv_zoom.as_ref().map(|z| z.field_key.clone()) else {
            return;
        };
        if let Some(z) = self.kv_zoom.as_mut() {
            z.text = text.clone();
            // 值变了，旧选中范围已经对不上
            z.sel = None;
        }
        self.fields.kv.remove(&key);
        self.kv_set_any(&key, text, cx);
        cx.notify();
    }

    pub(crate) fn kv_zoom_begin(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(z) = self.kv_zoom.as_mut() else {
            return;
        };
        let col = z.hover_col.unwrap_or(0);
        z.sel = Some(RespSel {
            a_row: row,
            a_col: col,
            c_row: row,
            c_col: col,
        });
        z.drag = true;
        z.focus.clone().focus(window);
        cx.notify();
    }

    pub(crate) fn kv_zoom_drag_to(&mut self, row: usize, col: usize, cx: &mut Context<Self>) {
        let Some(z) = self.kv_zoom.as_mut() else {
            return;
        };
        z.hover_col = Some(col);
        if !z.drag {
            return;
        }
        if let Some(sel) = z.sel.as_mut()
            && (sel.c_row, sel.c_col) != (row, col)
        {
            sel.c_row = row;
            sel.c_col = col;
            cx.notify();
        }
    }

    pub(crate) fn kv_zoom_end(&mut self, cx: &mut Context<Self>) {
        let Some(z) = self.kv_zoom.as_mut() else {
            return;
        };
        if z.drag {
            z.drag = false;
            cx.notify();
        }
    }

    pub fn kv_zoom_selected_text(&self) -> Option<String> {
        let z = self.kv_zoom.as_ref()?;
        join_resp_selection(&kv_zoom_lines(&z.text), z.sel?)
    }

    /// ⌘C：选了就复制选中，没选就复制全文（弹框里全文才是常态）。
    pub(crate) fn kv_zoom_copy_shortcut(&mut self, cx: &mut Context<Self>) {
        match self.kv_zoom_selected_text() {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.toast(self.t("flash.copied_text").to_string(), cx);
            }
            None => self.kv_zoom_copy_all(cx),
        }
    }

    pub fn kv_zoom_copy_all(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.kv_zoom.as_ref().map(|z| z.text.clone()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast(self.t("flash.copied_all").to_string(), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::kv_zoom_lines;

    #[test]
    fn lines_split_keeps_empty_and_trailing() {
        assert_eq!(kv_zoom_lines("a\nb"), vec!["a", "b"]);
        assert_eq!(kv_zoom_lines(""), vec![""]);
        assert_eq!(kv_zoom_lines("a\n"), vec!["a", ""]);
        assert_eq!(kv_zoom_lines("中文\r\n次行"), vec!["中文\r", "次行"]);
    }
}
