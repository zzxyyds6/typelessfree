//! 应用配置：快捷键与触发模式。
//! 配置文件位于系统配置目录（如 Windows: %APPDATA%/typelessfree/config.json），
//! 不存在或字段缺失时使用默认值。设置界面（F8）后续会负责写入此文件。

use serde::Deserialize;
use tauri::Manager;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    hotkey_modifiers: Option<String>,
    hotkey_key: Option<String>,
    /// "hold" = 按住说话，"toggle" = 按一次开始/再按结束
    mode: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Hold,
    Toggle,
}

impl TriggerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TriggerMode::Hold => "hold",
            TriggerMode::Toggle => "toggle",
        }
    }
}

pub struct HotkeyConfig {
    pub modifiers: global_hotkey::hotkey::Modifiers,
    pub code: global_hotkey::hotkey::Code,
    pub mode: TriggerMode,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifiers: global_hotkey::hotkey::Modifiers::CONTROL | global_hotkey::hotkey::Modifiers::SHIFT,
            code: global_hotkey::hotkey::Code::Space,
            mode: TriggerMode::Hold,
        }
    }
}

/// 解析修饰键字符串，如 "control+shift"、"alt+super"。
fn parse_modifiers(s: &str) -> Option<global_hotkey::hotkey::Modifiers> {
    let mut mods = global_hotkey::hotkey::Modifiers::empty();
    for part in s.split('+') {
        let m = match part.trim().to_ascii_lowercase().as_str() {
            "control" | "ctrl" => global_hotkey::hotkey::Modifiers::CONTROL,
            "shift" => global_hotkey::hotkey::Modifiers::SHIFT,
            "alt" => global_hotkey::hotkey::Modifiers::ALT,
            "super" | "meta" | "win" | "cmd" => global_hotkey::hotkey::Modifiers::META,
            _ => return None,
        };
        mods |= m;
    }
    if mods.is_empty() { None } else { Some(mods) }
}

/// 解析主键。MVP 支持常用键，后续在设置界面扩展。
fn parse_key(s: &str) -> Option<global_hotkey::hotkey::Code> {
    use global_hotkey::hotkey::Code;
    let k = s.trim().to_ascii_lowercase();
    Some(match k.as_str() {
        "space" => Code::Space,
        "q" => Code::KeyQ,
        "z" => Code::KeyZ,
        "d" => Code::KeyD,
        "v" => Code::KeyV,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        _ => return None,
    })
}

pub fn load(app: &tauri::AppHandle) -> HotkeyConfig {
    let mut cfg = HotkeyConfig::default();
    let Ok(dir) = app.path().app_config_dir() else {
        return cfg;
    };
    let Ok(text) = std::fs::read_to_string(dir.join("config.json")) else {
        return cfg;
    };
    let Ok(file) = serde_json::from_str::<ConfigFile>(&text) else {
        eprintln!("config.json 格式错误，使用默认配置");
        return cfg;
    };
    if let Some(s) = &file.hotkey_modifiers {
        if let Some(m) = parse_modifiers(s) {
            cfg.modifiers = m;
        }
    }
    if let Some(s) = &file.hotkey_key {
        if let Some(c) = parse_key(s) {
            cfg.code = c;
        }
    }
    if let Some(mode) = file.mode.as_deref() {
        cfg.mode = match mode {
            "toggle" => TriggerMode::Toggle,
            _ => TriggerMode::Hold,
        };
    }
    cfg
}

