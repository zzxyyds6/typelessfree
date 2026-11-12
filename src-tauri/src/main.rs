mod audio;
mod cleanup;
mod hotwords;
mod config;
mod insert;
mod model;
mod transcribe;

use audio::AudioHandle;
use config::{HotkeyConfig, TriggerMode};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use global_hotkey::hotkey::HotKey;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// 全局热键管理器与当前热键（保存设置时支持运行时重注册）。
struct HotkeyHolder(std::sync::Mutex<(GlobalHotKeyManager, HotKey)>);

/// 事件线程读取的当前热键与触发模式（与 HotkeyHolder 保持一致）。
struct HotkeyState(std::sync::Mutex<(HotKey, TriggerMode)>);

/// Tauri 命令：从配置目录的 hotwords.json 导入热词词典。
#[tauri::command]
fn import_hotwords(app: AppHandle) -> Result<usize, String> {
    hotwords::import_from_file(&app)
}

/// Tauri 命令：导出热词词典到配置目录的 hotwords.json。
#[tauri::command]
fn export_hotwords(app: AppHandle) -> Result<usize, String> {
    hotwords::export_to_file(&app)
}

/// Tauri 命令：读取当前设置（camelCase，供设置界面填充表单）。
#[tauri::command]
fn get_settings(app: AppHandle) -> serde_json::Value {
    let cfg: HotkeyConfig = config::load(&app);
    serde_json::json!({
        "hotkeyModifiers": config::modifiers_to_string(cfg.modifiers),
        "hotkeyKey": config::key_to_string(cfg.code),
        "mode": cfg.mode.as_str(),
        "model": cfg.model,
        "language": cfg.language,
        "cleanup": {
            "enabled": cfg.cleanup.enabled,
            "endpoint": cfg.cleanup.endpoint,
            "model": cfg.cleanup.model,
            "apiKey": cfg.cleanup.api_key,
            "prompt": cfg.cleanup.prompt,
        },
        "hotwords": cfg.hotwords,
    })
}

/// Tauri 命令：保存设置并即时生效（重注册全局热键、更新托盘提示）。
#[tauri::command]
fn save_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), String> {
    config::validate(&settings)?;
    let new_hotkey = HotKey::new(
        config::parse_modifiers(
            settings.get("hotkeyModifiers").and_then(|v| v.as_str()).unwrap_or("control+shift"),
        )
        .ok_or_else(|| "不支持的修饰键".to_string())?,
        config::parse_key(
            settings.get("hotkeyKey").and_then(|v| v.as_str()).unwrap_or("space"),
        )
        .ok_or_else(|| "不支持的主键".to_string())?,
    );

    // 先尝试重注册（失败时恢复旧热键，不打断现有使用）
    {
        let holder = app.state::<HotkeyHolder>();
        let mut guard = holder
            .0
            .lock()
            .map_err(|_| "热键状态锁获取失败".to_string())?;
        let old = guard.1;
        let _ = guard.0.unregister(old);
        if let Err(e) = guard.0.register(new_hotkey) {
            let _ = guard.0.register(old);
            return Err(format!("全局热键注册失败：可能已被其他程序占用：{e}"));
        }
        guard.1 = new_hotkey;
    }

    // 写配置文件
    config::save(&app, &settings)?;

    // 同步事件线程使用的当前热键与触发模式
    let mode_str = settings
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("hold");
    let new_mode = if mode_str == "toggle" { TriggerMode::Toggle } else { TriggerMode::Hold };
    {
        let st = app.state::<HotkeyState>();
        let mut g = st.0.lock().map_err(|_| "热键状态锁获取失败".to_string())?;
        *g = (new_hotkey, new_mode);
    }

    // 更新托盘提示
    let label = if mode_str == "toggle" { "切换模式" } else { "按住说话" };
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(format!("typelessfree（{label}）")));
    }
    Ok(())
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
                let prompt = hotwords::build_initial_prompt(&cfg.hotwords);
                if let Ok(text) = transcribe::transcribe(&h_partial, &snap.samples, &cfg.language, &prompt, &model_path) {
                    if !text.is_empty() {
                        let fixed = hotwords::correct(&text, &cfg.hotwords);
                        let _ = h_partial.emit("partial-result", serde_json::json!({ "text": fixed }));
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
        let prompt = hotwords::build_initial_prompt(&cfg.hotwords);
        match transcribe::transcribe(&handle, &samples, &cfg.language, &prompt, &model_path) {
            Ok(text) => {
                // 热词后处理修正（在 AI 清理之前，保证词典词形优先）
                let text = hotwords::correct(&text, &cfg.hotwords);
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

            // ---- 注册全局热键（manager 托管，支持设置界面重注册） ----
            let manager = GlobalHotKeyManager::new().expect("无法创建全局热键管理器");
            let hotkey = HotKey::new(Some(cfg.modifiers), cfg.code);
            manager
                .register(hotkey)
                .expect("全局热键注册失败：快捷键可能已被其他程序占用");
            app.manage(HotkeyHolder(std::sync::Mutex::new((manager, hotkey))));
            app.manage(HotkeyState(std::sync::Mutex::new((hotkey, mode))));

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
            let handle_thread = handle.clone();
            std::thread::spawn(move || {
                for event in GlobalHotKeyEvent::receiver() {
                    let (current_hotkey, current_mode) = {
                        let g = handle_thread
                            .state::<HotkeyState>()
                            .0
                            .lock()
                            .expect("热键状态锁获取失败");
                        (*g.0, g.1)
                    };
                    if event.id != current_hotkey.id() {
                        continue;
                    }
                    let holder = handle_thread.state::<AudioHolder>();
                    let state = handle_thread.state::<RecordingState>();
                    match event.state {
                        HotKeyState::Pressed => match current_mode {
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
                            if current_mode == TriggerMode::Hold {
                                stop_and_transcribe(&handle_thread, &holder, &state);
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![import_hotwords, export_hotwords, get_settings, save_settings])
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