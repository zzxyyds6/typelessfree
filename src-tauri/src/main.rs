mod audio;
mod cleanup;
mod config;
mod insert;
mod model;
mod transcribe;

use audio::AudioHandle;
use config::{HotkeyConfig, TriggerMode};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use global_hotkey::hotkey::HotKey;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
};
use tauri::WindowEvent;

/// 记录当前是否正在录音（两种模式共用）。
struct RecordingState(AtomicBool);

/// 音频服务持有者，供热键线程访问。
struct AudioHolder {
    audio: AudioHandle,
}

/// 在鼠标光标附近显示浮动预览卡（不抢焦点）。
fn show_preview(handle: &AppHandle) {
    if let Some(win) = handle.get_webview_window("preview") {
        if let Ok(pos) = handle.cursor_position() {
            let _ = win.set_position(PhysicalPosition::new(pos.x + 20.0, pos.y + 20.0));
        }
        let _ = win.show();
    }
}

fn hide_preview(handle: &AppHandle) {
    if let Some(win) = handle.get_webview_window("preview") {
        let _ = win.hide();
    }
}

fn start_recording(handle: &AppHandle, holder: &AudioHolder, state: &RecordingState) {
    match holder.audio.start() {
        Ok(()) => {
            state.0.store(true, Ordering::SeqCst);
            show_preview(handle);
            let _ = handle.emit("recording-started", ());

            // ---- 音量指示线程：150ms 一次，驱动预览卡音量条 ----
            let h_level = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let snap = match h_level.state::<AudioHolder>().audio.snapshot() {
                    Ok(s) => s,
                    Err(_) => break,
                };
                if !snap.recording {
                    break;
                }
                let _ = h_level.emit("recording-level", serde_json::json!({ "level": snap.level }));
            });

            // ---- 实时 partial 识别线程：每 1.8s 对已录音频做一次推断 ----
            let h_partial = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(1800));
                let snap = match h_partial.state::<AudioHolder>().audio.snapshot() {
                    Ok(s) => s,
                    Err(_) => break,
                };
                if !snap.recording {
                    break;
                }
                // 少于 1 秒的音频不值得推断
                if snap.samples.len() < 16000 {
                    continue;
                }
                let cfg: HotkeyConfig = config::load(&h_partial);
                let model_path = model::resolve_model_path(&h_partial, &cfg.model);
                // partial 失败静默忽略（比如模型还在下载），不干扰用户
                if let Ok(text) = transcribe::transcribe(&h_partial, &snap.samples, &cfg.language, &model_path) {
                    if !text.is_empty() {
                        let _ = h_partial.emit("partial-result", serde_json::json!({ "text": text }));
                    }
                }
            });
        }
        Err(e) => {
            let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
        }
    }
}

fn stop_and_transcribe(handle: &AppHandle, holder: &AudioHolder, state: &RecordingState) {
    state.0.store(false, Ordering::SeqCst);
    let samples = match holder.audio.stop() {
        Ok(s) => s,
        Err(e) => {
            hide_preview(handle);
            let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
            return;
        }
    };
    let _ = handle.emit("recording-stopped", ());
    let _ = handle.emit("partial-result", serde_json::json!({ "text": "" }));

    // 识别可能耗时数秒，放独立线程执行，不阻塞热键监听
    let handle = handle.clone();
    std::thread::spawn(move || {
        let cfg: HotkeyConfig = config::load(&handle);
        let model_path = model::resolve_model_path(&handle, &cfg.model);
        match transcribe::transcribe(&handle, &samples, &cfg.language, &model_path) {
            Ok(text) => {
                // 可选 AI 清理（默认关闭；关闭时文本不出本机）
                let text = if text.is_empty() {
                    text
                } else {
                    match cleanup::clean_text(&handle, &text, &cfg) {
                        Some(t) => t,
                        None => text,
                    }
                };
                // 识别完成后自动插入光标位置；插入失败不影响结果展示
                if !text.is_empty() {
                    if let Err(e) = insert::insert_text(&text) {
                        let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
                    }
                }
                let _ = handle.emit("transcription-result", serde_json::json!({ "text": text }));
            }
            Err(e) => {
                let _ = handle.emit("app-error", serde_json::json!({ "message": e }));
            }
        }
        hide_preview(&handle);
    });
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();

            // ---- 浮动预览窗口（frameless、置顶、不抢焦点，初始隐藏） ----
            WebviewWindowBuilder::new(
                app,
                "preview",
                WebviewUrl::App("index.html#preview".into()),
            )
            .title("typelessfree 预览")
            .inner_size(420.0, 180.0)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .resizable(false)
            .build()?;

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
            app.manage(AudioHolder {
                audio: AudioHandle::spawn(),
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
                    let holder = handle_thread.state::<AudioHolder>();
                    let state = handle_thread.state::<RecordingState>();
                    match event.state {
                        HotKeyState::Pressed => match mode {
                            TriggerMode::Hold => {
                                start_recording(&handle_thread, &holder, &state);
                            }
                            TriggerMode::Toggle => {
                                let recording = state.0.load(Ordering::SeqCst);
                                if recording {
                                    stop_and_transcribe(&handle_thread, &holder, &state);
                                } else {
                                    start_recording(&handle_thread, &holder, &state);
                                }
                            }
                        },
                        HotKeyState::Released => {
                            if mode == TriggerMode::Hold {
                                stop_and_transcribe(&handle_thread, &holder, &state);
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口=隐藏到托盘，不退出应用
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("typelessfree 启动失败");
}