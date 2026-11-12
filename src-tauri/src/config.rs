//! 应用配置：快捷键、触发模式、模型、语言与 AI 清理。
//! 配置文件位于系统配置目录（如 Windows: %APPDATA%/typelessfree/config.json），
//! 不存在或字段缺失时使用默认值。设置界面（F8）后续会负责写入此文件。

use serde::Deserialize;
use tauri::Manager;

/// 默认清理提示词（中文口语 → 干净书面文字）。
pub const DEFAULT_CLEANUP_PROMPT: &str = "你是语音转文字的编辑助手。请把用户发来的语音识别文本整理成通顺自然的书面文字：去除语气填充词（嗯、啊、就是、那个等）、口误和自我修正的重复内容，补上合适的标点，必要时整理成列表。只输出整理后的文字，不要任何解释。保持原意和原文语言。";

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    hotkey_modifiers: Option<String>,
    hotkey_key: Option<String>,
    /// "hold" = 按住说话，"toggle" = 按一次开始/再按结束
    mode: Option<String>,
    /// 模型名（tiny/base/small/medium/large-v3-turbo）或模型文件绝对路径
    model: Option<String>,
    /// "auto" = 自动检测，或 "zh" / "en" 等语言代码
    language: Option<String>,
    cleanup: Option<CleanupConfig>,
    /// 个人词典 / 热词列表
    #[serde(default)]
    hotwords: Option<Vec<String>>,
}

/// AI 清理配置。默认关闭：关闭时文本绝不出本机。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupConfig {
    #[serde(default)]
    pub enabled: bool,
    /// OpenAI 兼容 chat/completions 端点（Ollama 本地或其他自建服务）
    pub endpoint: Option<String>,
    pub model: Option<String>,
    /// 云端 API 时填写；Ollama 本地可留空
    pub api_key: Option<String>,
    pub prompt: Option<String>,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: Some("http://localhost:11434/v1/chat/completions".to_string()),
            model: Some("llama3.2".to_string()),
            api_key: Some(String::new()),
            prompt: Some(DEFAULT_CLEANUP_PROMPT.to_string()),
        }
    }
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
    pub model: String,
    pub language: String,
    pub cleanup: CleanupConfig,
    /// 个人词典 / 热词（识别 prompt 注入 + 结果后处理修正）
    pub hotwords: Vec<String>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifiers: global_hotkey::hotkey::Modifiers::CONTROL
                | global_hotkey::hotkey::Modifiers::SHIFT,
            code: global_hotkey::hotkey::Code::Space,
            mode: TriggerMode::Hold,
            model: "base".to_string(),
            language: "auto".to_string(),
            cleanup: CleanupConfig::default(),
            hotwords: Vec::new(),
        }
    }
}

/// 解析修饰键字符串，如 "control+shift"、"alt+super"。
pub fn parse_modifiers(s: &str) -> Option<global_hotkey::hotkey::Modifiers> {
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
pub fn parse_key(s: &str) -> Option<global_hotkey::hotkey::Code> {
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
    if let Some(m) = file.model {
        cfg.model = m;
    }
    if let Some(l) = file.language {
        cfg.language = l;
    }
    if let Some(h) = file.hotwords {
        cfg.hotwords = h.into_iter().filter(|w| !w.trim().is_empty()).collect();
    }
    if let Some(c) = file.cleanup {
        cfg.cleanup.enabled = c.enabled;
        if let Some(e) = c.endpoint {
            cfg.cleanup.endpoint = Some(e);
        }
        if let Some(m) = c.model {
            cfg.cleanup.model = Some(m);
        }
        if let Some(k) = c.api_key {
            cfg.cleanup.api_key = Some(k);
        }
        if let Some(p) = c.prompt {
            cfg.cleanup.prompt = Some(p);
        }
    }
    cfg
}
/// 把修饰键位标志转回配置字符串（如 "control+shift"）。
pub fn modifiers_to_string(m: global_hotkey::hotkey::Modifiers) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if m.contains(global_hotkey::hotkey::Modifiers::CONTROL) {
        parts.push("control");
    }
    if m.contains(global_hotkey::hotkey::Modifiers::SHIFT) {
        parts.push("shift");
    }
    if m.contains(global_hotkey::hotkey::Modifiers::ALT) {
        parts.push("alt");
    }
    if m.contains(global_hotkey::hotkey::Modifiers::META) {
        parts.push("super");
    }
    if parts.is_empty() {
        parts.push("none");
    }
    parts.join("+")
}

/// 把主键转回配置字符串。
pub fn key_to_string(code: global_hotkey::hotkey::Code) -> String {
    use global_hotkey::hotkey::Code;
    match code {
        Code::Space => "space".to_string(),
        Code::KeyQ => "q".to_string(),
        Code::KeyZ => "z".to_string(),
        Code::KeyD => "d".to_string(),
        Code::KeyV => "v".to_string(),
        Code::F1 => "f1".to_string(),
        Code::F2 => "f2".to_string(),
        Code::F3 => "f3".to_string(),
        Code::F4 => "f4".to_string(),
        _ => "space".to_string(),
    }
}

/// 校验设置界面提交的配置（camelCase 字段），不合法时返回错误说明。
pub fn validate(settings: &serde_json::Value) -> Result<(), String> {
    let file: ConfigFile = serde_json::from_value(settings.clone())
        .map_err(|e| format!("配置格式错误: {e}"))?;
    if let Some(m) = &file.hotkey_modifiers {
        if parse_modifiers(m).is_none() {
            return Err(format!("不支持的修饰键：{m}（可选 control/shift/alt/super）"));
        }
    }
    if let Some(k) = &file.hotkey_key {
        if parse_key(k).is_none() {
            return Err(format!("不支持的主键：{k}（可选 space/q/z/d/v/f1-f4）"));
        }
    }
    if let Some(mode) = file.mode.as_deref() {
        if mode != "hold" && mode != "toggle" {
            return Err(format!("不支持的触发模式：{mode}（可选 hold/toggle）"));
        }
    }
    if let Some(model) = &file.model {
        if model.trim().is_empty() {
            return Err("模型名不能为空".to_string());
        }
    }
    Ok(())
}

/// 保存设置到 config.json。校验 + 规范化热词后整体写入，保留未提交字段。
pub fn save(app: &tauri::AppHandle, settings: &serde_json::Value) -> Result<(), String> {
    validate(settings)?;
    let mut out = settings.clone();
    // 规范化热词：去首尾空白、去空行、大小写不敏感去重
    if let Some(arr) = out.get_mut("hotwords") {
        if let Some(list) = arr.as_array_mut() {
            let mut seen: Vec<String> = Vec::new();
            let mut cleaned: Vec<serde_json::Value> = Vec::new();
            for item in list {
                if let Some(w) = item.as_str() {
                    let w = w.trim().to_string();
                    if w.is_empty() || seen.iter().any(|e| e.eq_ignore_ascii_case(&w)) {
                        continue;
                    }
                    seen.push(w.clone());
                    cleaned.push(serde_json::Value::String(w));
                }
            }
            *arr = serde_json::Value::Array(cleaned);
        }
    }
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "无法获取配置目录".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let text = serde_json::to_string_pretty(&out).map_err(|e| format!("序列化配置失败: {e}"))?;
    std::fs::write(dir.join("config.json"), text).map_err(|e| format!("写入配置失败: {e}"))?;
    Ok(())
}
