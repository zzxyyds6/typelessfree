mod audio;
mod config;
mod model;
mod transcribe;

use audio::AudioRecorder;
use config::{HotkeyConfig, TriggerMode};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use global_hotkey::hotkey::HotKey;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

/// 记录 toggle 模式下的当前状态（true = 正在录音）。
struct RecordingState(AtomicBool);

/// 录音器持有者，供热键线程访问。
struct RecorderHolder {
    recorder: Mutex<AudioRecorder>,
}

fn start_recording(handle: &AppHandle, holder: &RecorderHolder) {
    let mut rec = holder.recorder.lock().unwrap();
    match rec.start() {
        Ok(()) => {
            let _ = handle.emit("recording-started", ());
        }
        Err(e) => {
            let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
        }
    }
}

fn stop_and_transcribe(handle: &AppHandle, holder: &RecorderHolder) {
    let samples = {
        let mut rec = holder.recorder.lock().unwrap();
        match rec.stop() {
            Ok(s) => s,
            Err(e) => {
                let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
                return;
            }
        }
    };
    let _ = handle.emit("recording-stopped", ());

    // 识别可能耗时数秒，放独立线程执行，不阻塞热键监听
    let handle = handle.clone();
    std::thread::spawn(move || {
        let cfg: HotkeyConfig = config::load(&handle);
        let model_path = model::resolve_model_path(&handle, &cfg.model);
        match transcribe::transcribe(&handle, &samples, &cfg.language, &model_path) {
            Ok(text) => {
                let _ = handle.emit("transcription-result", serde_json::json!({ "text": text }));
            }
            Err(e) => {
                let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
            }
        }
    });
}

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

            // ---- 共享状态 ----
            app.manage(RecorderHolder {
                recorder: Mutex::new(AudioRecorder::new()),
            });
            app.manage(RecordingState(AtomicBool::new(false)));

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

            // ---- 热键事件线程：驱动录音与识别 ----
            let hotkey_thread = hotkey;
            let handle_thread = handle.clone();
            std::thread::spawn(move || {
                for event in GlobalHotKeyEvent::receiver() {
                    if event.id != hotkey_thread.id() {
                        continue;
                    }
                    let holder = handle_thread.state::<RecorderHolder>();
                    let state = handle_thread.state::<RecordingState>();
                    match event.state {
                        HotKeyState::Pressed => match mode {
                            TriggerMode::Hold => start_recording(&handle_thread, &holder),
                            TriggerMode::Toggle => {
                                let recording = state.0.load(Ordering::SeqCst);
                                if recording {
                                    stop_and_transcribe(&handle_thread, &holder);
                                    state.0.store(false, Ordering::SeqCst);
                                } else {
                                    start_recording(&handle_thread, &holder);
                                    state.0.store(true, Ordering::SeqCst);
                                }
                            }
                        },
                        HotKeyState::Released => {
                            if mode == TriggerMode::Hold {
                                stop_and_transcribe(&handle_thread, &holder);
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("typelessfree 启动失败");
}

