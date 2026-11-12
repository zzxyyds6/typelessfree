//! 模型管理：模型路径解析与下载（带进度事件）。
//! 模型来源：whisper.cpp 官方 ggml 模型（HuggingFace）。

use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

pub const AVAILABLE_MODELS: &[&str] = &["tiny", "base", "small", "medium", "large-v3-turbo"];

pub fn model_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("models")
}

/// 解析配置中的模型值：绝对路径直接使用；否则视为模型名。
pub fn resolve_model_path(app: &AppHandle, model: &str) -> PathBuf {
    let p = PathBuf::from(model);
    if p.is_absolute() {
        return p;
    }
    model_dir(app).join(format!("ggml-{model}.bin"))
}

fn model_url(name: &str) -> String {
    format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin")
}

/// 确保模型存在；缺失时下载并向前端广播进度。
pub fn ensure_model(app: &AppHandle, path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ggml-base")
        .trim_start_matches("ggml-")
        .to_string();
    let url = model_url(&name);
    let dir = path.parent().ok_or("模型路径无效")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建模型目录失败: {e}"))?;

    let _ = app.emit(
        "model-download-started",
        serde_json::json!({ "model": name, "url": url }),
    );
    let start = Instant::now();
    let resp = ureq::get(&url).call().map_err(|e| format!("模型下载失败: {e}"))?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = File::create(path).map_err(|e| format!("创建模型文件失败: {e}"))?;
    let mut buf = [0u8; 65536];
    let mut received: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("下载中断: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("写入模型失败: {e}"))?;
        received += n as u64;
        // 每 500ms 广播一次进度，避免事件风暴
        if last_emit.elapsed().as_millis() >= 500 {
            last_emit = Instant::now();
            let _ = app.emit(
                "model-download-progress",
                serde_json::json!({ "model": name, "received": received, "total": total }),
            );
        }
    }
    file.flush().ok();
    let _ = app.emit(
        "model-download-finished",
        serde_json::json!({ "model": name, "bytes": received, "seconds": start.elapsed().as_secs() }),
    );
    Ok(())
}

