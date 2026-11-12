//! 语音识别：重采样 + whisper.cpp 推理。

use tauri::{AppHandle, Emitter};

/// whisper 语言 token id → 语言代码（覆盖 MVP 常见语言，en/zh 优先验证）。
const LANG_IDS: &[(i32, &str)] = &[
    (0, "en"),
    (5079, "zh"),
    (5028, "de"),
    (5031, "es"),
    (5041, "fr"),
    (5056, "ja"),
    (5066, "ko"),
    (5061, "it"),
    (5075, "pt"),
    (5049, "ru"),
];

/// 线性插值重采样到 16kHz（whisper 输入要求）。
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    if from_rate == 16000 || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / 16000.0;
    let out_len = (samples.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(samples.len() - 1);
        let frac = (pos - i0 as f64) as f32;
        out.push(samples[i0] * (1.0 - frac) + samples[i1] * frac);
    }
    out
}

/// 运行 whisper 推理，返回识别文本。
/// language 为 "auto"/空时启用 whisper 自动语言检测，并通过 language-detected 事件广播结果。
pub fn transcribe(
    app: &AppHandle,
    samples: &[f32],
    language: &str,
    model_path: &std::path::Path,
) -> Result<String, String> {
    if samples.len() < 1600 {
        // 少于 0.1 秒的音频直接返回空
        return Ok(String::new());
    }
    crate::model::ensure_model(app, model_path)?;

    let path_str = model_path
        .to_str()
        .ok_or_else(|| "模型路径含非法字符".to_string())?
        .to_string();

    // whisper.cpp 模型加载约 1-2 秒；MVP 每次重新加载，后续可做上下文缓存
    let ctx = whisper_rs::WhisperContext::new_with_params(
        &path_str,
        whisper_rs::WhisperContextParameters::default(),
    )
    .map_err(|e| format!("加载模型失败: {e}"))?;

    let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy {
        best_of: 1,
    });
    let auto_detect = language.is_empty() || language.eq_ignore_ascii_case("auto");
    // None = whisper 内部自动检测语言
    let _ = params.set_language(if auto_detect { None } else { Some(language) });
    params.set_no_context(true);
    params.set_suppress_blank(true);

    let mut state = ctx.create_state().map_err(|e| format!("创建推理状态失败: {e}"))?;
    state.full(params, samples).map_err(|e| format!("识别失败: {e}"))?;

    // 自动模式下广播检测到的语言，供前端显示
    if auto_detect {
        if let Ok(lang_id) = state.full_lang_id() {
            if let Some((_, code)) = LANG_IDS.iter().find(|(id, _)| *id == lang_id) {
                let _ = app.emit("language-detected", serde_json::json!({ "code": code }));
            }
        }
    }

    let n = state
        .full_n_segments()
        .map_err(|e| format!("读取分段失败: {e}"))?;
    let mut text = String::new();
    for i in 0..n {
        let seg = state
            .full_get_segment_text(i)
            .map_err(|e| format!("读取文本失败: {e}"))?;
        text.push_str(&seg);
    }
    Ok(text.trim().to_string())
}