//! 可选的 AI 文本清理。
//! 隐私约束：仅在用户明确开启 cleanup.enabled 时才发起网络请求；
//! 关闭时直接返回 None，文本绝不出本机。
//! 兼容任意 OpenAI chat/completions 端点（Ollama 本地 / 云端 BYOK）。

use serde_json::json;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 尝试用配置的 LLM 清理文本。
/// 返回 None 表示"未开启"或"清理失败"（失败时会发出 app-error 事件），调用方应回退到原文。
pub fn clean_text(app: &AppHandle, text: &str, cfg: &crate::config::HotkeyConfig) -> Option<String> {
    let c = &cfg.cleanup;

    // 未开启清理：直接短路，不发任何请求
    if !c.enabled {
        return None;
    }

    let endpoint = c.endpoint.as_ref()?;
    let model = c.model.as_ref()?;
    let prompt = c
        .prompt
        .as_deref()
        .unwrap_or(crate::config::DEFAULT_CLEANUP_PROMPT);

    let mut req = ureq::post(endpoint.as_str()).timeout(Duration::from_secs(20));
    if !c.api_key.as_deref().unwrap_or("").is_empty() {
        req = req.set(
            "Authorization",
            &format!("Bearer {}", c.api_key.as_deref().unwrap_or("")),
        );
    }

    let body = json!({
        "model": model,
        "temperature": 0,
        "messages": [
            { "role": "system", "content": prompt },
            { "role": "user", "content": text }
        ]
    });

    let resp = match req.send_json(body) {
        Ok(r) => r,
        Err(e) => {
            let _ = app.emit(
                "app-error",
                json!({ "message": format!("AI 清理请求失败（回退为原文）: {e}") }),
            );
            return None;
        }
    };

    let v: serde_json::Value = match resp.into_json() {
        Ok(v) => v,
        Err(e) => {
            let _ = app.emit(
                "app-error",
                json!({ "message": format!("AI 清理响应解析失败（回退为原文）: {e}") }),
            );
            return None;
        }
    };

    let cleaned = v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if cleaned.is_none() {
        let _ = app.emit(
            "app-error",
            json!({ "message": "AI 清理响应中没有可用内容（回退为原文）".to_string() }),
        );
    }
    cleaned
}