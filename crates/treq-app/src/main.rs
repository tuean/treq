mod dialogs;
mod files;
mod fold;
mod i18n;
mod jsonview;
mod model;
mod nav;
mod panes;
mod search;
mod settings;
mod syntax;
mod theme;
mod widgets;

use gpui::*;
use model::{
    AppModel, BackupNow, ChooseWorkspace, DuplicateRequest, FocusResponseFilter, ImportFile,
    NavBack, NavForward, OpenPrefs, QuickClose, QuickDown, QuickSwitch, QuickUp, Quit, Reload,
    RestoreBackup, SendNow, ToggleLocale,
};

/// 崩溃时把 panic 信息写进 settings 同目录的 panic.log。
///
/// GUI 从 Dock 启动时 stderr 没人看（两次崩溃的报告里只有 SIGABRT，没有消息），
/// 最近按过的键（崩溃时跟 panic 一起写进 panic.log，用来定位是哪个键触发的）。
static RECENT_KEYS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
const RECENT_KEYS_MAX: usize = 12;
const PANIC_LOG_MAX_BYTES: u64 = 256 * 1024;

/// 记一个按键（只留最近 [`RECENT_KEYS_MAX`] 条）。
fn note_keystroke(k: &Keystroke) {
    let mut mods = String::new();
    for (on, name) in [
        (k.modifiers.platform, "cmd+"),
        (k.modifiers.control, "ctrl+"),
        (k.modifiers.alt, "alt+"),
        (k.modifiers.shift, "shift+"),
    ] {
        if on {
            mods.push_str(name);
        }
    }
    let desc = match &k.key_char {
        Some(c) => format!("{mods}{} [{c}]", k.key),
        None => format!("{mods}{}", k.key),
    };
    if let Ok(mut keys) = RECENT_KEYS.lock() {
        if keys.len() >= RECENT_KEYS_MAX {
            keys.remove(0);
        }
        keys.push(desc);
    }
}

/// 崩溃时把 panic 信息 + 最近的按键写进 settings 同目录的 panic.log。
///
/// GUI 从 Dock 启动时 stderr 没人看（崩溃报告里只有 SIGABRT），落一份文件才好查。
/// 追加而不是覆盖：真正的 panic 之后还会跟一条「cannot unwind」，两条都要留着。
fn install_panic_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let keys = RECENT_KEYS
            .lock()
            .map(|k| k.join(" | "))
            .unwrap_or_default();
        let log = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("com.treq.app")
            .join("panic.log");
        // 崩溃循环别把文件撑爆
        if std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0) > PANIC_LOG_MAX_BYTES {
            let _ = std::fs::remove_file(&log);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            use std::io::Write as _;
            let _ = writeln!(
                f,
                "=== {:?}\n{info}\n最近按键: {keys}\n{}\n",
                std::time::SystemTime::now(),
                std::backtrace::Backtrace::force_capture()
            );
        }
        default(info);
    }));
}

/// 开一个主窗口。`reuse` 给「点 Dock 图标重开」用：窗口没了但 model 还在，
/// 直接把它挂到新窗口上，菜单栏动作与状态都不丢。
fn open_main_window(cx: &mut App, reuse: Option<Entity<AppModel>>) -> Entity<AppModel> {
    let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("treq".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| match reuse {
                Some(view) => view,
                None => cx.new(AppModel::new),
            },
        )
        .expect("打开主窗口失败");
    window
        .update(cx, |_, _, cx| cx.entity())
        .expect("取主窗口 root view 失败")
}

// 当前 model（窗口关掉后依然活着，重开窗口时挂回去）
thread_local! {
    static REUSE: std::cell::RefCell<Option<Entity<AppModel>>> =
        const { std::cell::RefCell::new(None) };
}

fn main() {
    install_panic_log();
    let app = Application::new().with_assets(widgets::SvgAssets);
    // 关掉窗口后点 Dock 图标 / 双击 app：把同一个 model 挂回新窗口——不复用的话，
    // 菜单栏那些 on_action 还指着旧窗口里的它，点了没反应（表现为「只能退出重开」）
    app.on_reopen(|cx| {
        let live = REUSE.with(|c| c.borrow().clone());
        if let Some(view) = live {
            open_main_window(cx, Some(view));
        }
    });
    app.run(|cx: &mut App| {
        // 记住最近的按键（订阅要活到进程结束，故意泄漏）
        std::mem::forget(cx.observe_keystrokes(|e, _window, _cx| {
            note_keystroke(&e.keystroke);
        }));
        widgets::bind_textfield_keys(cx);
        cx.bind_keys([
            // 快速查找：⌘K / ⌃K 都认
            KeyBinding::new("ctrl-k", QuickSwitch, None),
            KeyBinding::new("cmd-k", QuickSwitch, None),
            // 重发（流式进行中＝停止）
            KeyBinding::new("cmd-r", SendNow, None),
            // 复制为新请求 / 响应过滤条 / 偏好菜单
            KeyBinding::new("cmd-d", DuplicateRequest, None),
            KeyBinding::new("cmd-f", FocusResponseFilter, None),
            KeyBinding::new("cmd-,", OpenPrefs, None),
            // 浏览器式前进/后退：⌘[ ⌘] 与 ⌥← ⌥→ 都可用
            KeyBinding::new("cmd-[", NavBack, None),
            KeyBinding::new("cmd-]", NavForward, None),
            KeyBinding::new("alt-left", NavBack, None),
            KeyBinding::new("alt-right", NavForward, None),
            // 发送/重发；流式进行中则停止（与 Send 按钮同一套语义）
            KeyBinding::new("cmd-enter", SendNow, None),
            KeyBinding::new("escape", QuickClose, None),
            KeyBinding::new("up", QuickUp, None),
            KeyBinding::new("down", QuickDown, None),
        ]);
        cx.activate(true);

        // 原生菜单栏（标签固定英文，应用内文案走 i18n）
        cx.set_menus(vec![
            Menu {
                name: "treq".into(),
                items: vec![MenuItem::action("Quit", Quit)],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Choose Workspace…", ChooseWorkspace),
                    MenuItem::action("Import (Postman / OpenAPI / HAR)…", ImportFile),
                    MenuItem::separator(),
                    MenuItem::action("Back Up Now", BackupNow),
                    MenuItem::action("Backups & Restore…", RestoreBackup),
                    MenuItem::separator(),
                    MenuItem::action("Reload", Reload),
                ],
            },
            Menu {
                name: "View".into(),
                items: vec![
                    MenuItem::action("Back", NavBack),
                    MenuItem::action("Forward", NavForward),
                    MenuItem::action("Language", ToggleLocale),
                ],
            },
        ]);

        let entity = open_main_window(cx, None);

        // 记下 model 供「点 Dock 图标重开」用（on_reopen 注册在 Application 上，先于这里）
        REUSE.with(|c| *c.borrow_mut() = Some(entity.clone()));

        let handle = entity.clone();
        cx.on_action(move |_: &Quit, cx| cx.quit());
        cx.on_action(move |_: &Reload, cx| {
            handle.update(cx, |m, cx| m.refresh(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &ToggleLocale, cx| {
            handle.update(cx, |m, cx| m.toggle_locale(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &ChooseWorkspace, cx| {
            handle.update(cx, |m, cx| m.choose_workspace(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &ImportFile, cx| {
            handle.update(cx, |m, cx| m.import_file(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &SendNow, cx| {
            handle.update(cx, |m, cx| m.send_or_stop(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &FocusResponseFilter, cx| {
            let Some(w) = cx.active_window() else {
                return;
            };
            w.update(cx, |_, window, cx| {
                handle.update(cx, |m, cx| m.focus_response_filter(window, cx));
            })
            .ok();
        });
        let handle = entity.clone();
        cx.on_action(move |_: &DuplicateRequest, cx| {
            handle.update(cx, |m, cx| m.duplicate_selected(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &OpenPrefs, cx| {
            let Some(w) = cx.active_window() else {
                return;
            };
            w.update(cx, |_, window, cx| {
                handle.update(cx, |m, cx| m.open_prefs_at_corner(window, cx));
            })
            .ok();
        });
        let handle = entity.clone();
        cx.on_action(move |_: &BackupNow, cx| {
            handle.update(cx, |m, cx| m.backup_now(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &RestoreBackup, cx| {
            handle.update(cx, |m, cx| m.open_restore_dialog(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &QuickSwitch, cx| {
            handle.update(cx, |m, cx| m.open_quick(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &QuickClose, cx| {
            handle.update(cx, |m, cx| {
                if m.move_dialog.is_some() {
                    m.close_move_dialog(cx);
                } else if m.url_dialog {
                    m.close_url_dialog(cx);
                } else if m.kv_zoom.is_some() {
                    m.close_kv_zoom(cx);
                } else if m.settings_page.is_some() {
                    m.close_settings_page(cx);
                } else {
                    m.close_quick(cx);
                }
            });
        });
        let handle = entity.clone();
        cx.on_action(move |_: &NavBack, cx| {
            handle.update(cx, |m, cx| m.nav_back(cx));
        });
        let handle = entity.clone();
        cx.on_action(move |_: &NavForward, cx| {
            handle.update(cx, |m, cx| m.nav_forward(cx));
        });

        cx.activate(true);
    });
}
