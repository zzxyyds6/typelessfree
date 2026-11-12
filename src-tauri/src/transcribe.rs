//! 语音识别：重采样 + whisper.cpp 推理。

use tauri::AppHandle;

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
    let _ = params.set_language(Some(language));
    params.set_no_context(true);
    params.set_suppress_blank(true);

    let mut state = ctx.create_state().map_err(|e| format!("创建推理状态失败: {e}"))?;
    state.full(params, samples).map_err(|e| format!("识别失败: {e}"))?;

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

