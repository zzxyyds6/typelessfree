mod config;

use config::{HotkeyConfig, TriggerMode};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKey, HotKeyState,
};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

/// 记录 toggle 模式下的当前状态（true = 正在录音）。
struct RecordingState(AtomicBool);

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();

            // ---- 加载配置 ----
            let cfg: HotkeyConfig = config::load(&handle);
            let mode = cfg.mode;

            // ---- 注册全局热键 ----
            let manager = GlobalHotKeyManager::new().expect("无法创建全局热键管理器");
            let hotkey = HotKey::new(Some(cfg.modifiers), cfg.code);
            manager
                .register(hotkey)
                .expect("全局热键注册失败：快捷键可能已被其他程序占用");
            // manager 需要存活到进程结束；放进程级静态生命周期
            std::mem::forget(manager);

            // ---- 系统托盘 ----
            let settings_item =
                MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            let mode_label = match mode {
                TriggerMode::Hold => "按住说话",
                TriggerMode::Toggle => "切换模式",
            };
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().expect("缺少应用图标").clone())
                .tooltip(format!("typelessfree（{}）", mode_label))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // ---- 热键事件线程：向前端广播录音开始/停止 ----
            let state = std::sync::Arc::new(RecordingState(AtomicBool::new(false)));
            let state_thread = state.clone();
            let hotkey_thread = hotkey;
            std::thread::spawn(move || {
                for event in GlobalHotKeyEvent::receiver() {
                    if event.hotkey != hotkey_thread {
                        continue;
                    }
                    let should_emit = match event.state {
                        HotKeyState::Pressed => match mode {
                            TriggerMode::Hold => true,
                            TriggerMode::Toggle => {
                                let now = !state_thread.0.load(Ordering::SeqCst);
                                state_thread.0.store(now, Ordering::SeqCst);
                                now
                            }
                        },
                        HotKeyState::Released => match mode {
                            TriggerMode::Hold => true,
                            // toggle 模式下松开不产生事件
                            TriggerMode::Toggle => false,
                        },
                    };
                    match (event.state, should_emit) {
                        (HotKeyState::Pressed, true) => {
                            let _ = handle.emit("recording-started", ());
                        }
                        (HotKeyState::Released, true) => {
                            let _ = handle.emit("recording-stopped", ());
                        }
                        _ => {}
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("typelessfree 启动失败");
}
