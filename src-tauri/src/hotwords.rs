//! 个人词典 / 热词支持（F6）。
//!
//! 两层机制提升专有名词识别准确率：
//! 1. 识别阶段：把热词列表拼成 whisper initial_prompt，引导模型输出正确词形；
//! 2. 后处理阶段：对识别结果做模糊修正（编辑距离），兜底纠正常见错字。
//!
//! 词典可独立存放在配置目录的 hotwords.json（JSON 数组或每行一词的纯文本），
//! 便于导入 / 导出与备份。

/// 把热词列表拼成 whisper initial_prompt。
/// 列表为空时返回空字符串（不设置 prompt）。
pub fn build_initial_prompt(hotwords: &[String]) -> String {
    let words: Vec<&String> = hotwords
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return String::new();
    }
    format!(
        "以下是用户自定义的专有名词与术语，请在转写结果中准确使用：{}",
        words.join("、")
    )
}

/// Levenshtein 编辑距离（小字符串专用，O(n*m) 足够）。
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// 热词模糊匹配的容忍编辑距离阈值：词越长容忍越多错字。
fn tolerance(len: usize) -> usize {
    match len {
        0..=2 => 0,
        3..=5 => 1,
        6..=11 => 2,
        _ => 3,
    }
}

/// 对识别结果做热词后处理修正。
/// - 拉丁词（字母/数字）：按非字母数字分词，大小写不敏感、编辑距离容忍匹配；
/// - 中文等其他连续文字：等长窗口替换，编辑距离容忍（只纠错字，不吞词）。
/// 热词较短（<=2 字符）时只做精确替换，避免误伤正常文本。
pub fn correct(text: &str, hotwords: &[String]) -> String {
    let mut result = text.to_string();
    for hw in hotwords {
        let word = hw.trim();
        if word.is_empty() || result.contains(word) {
            continue;
        }
        let chars: Vec<char> = word.chars().collect();
        if chars.iter().all(|c| c.is_ascii_alphanumeric()) {
            result = correct_latin(&result, word);
        } else {
            result = correct_cjk(&result, &chars);
        }
    }
    result
}

/// 拉丁词修正：按非字母数字分词后逐词比较（大小写不敏感 + 编辑距离）。
fn correct_latin(text: &str, word: &str) -> String {
    let target_lower: Vec<char> = word.to_lowercase().chars().collect();
    let tol = tolerance(target_lower.len());
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            token.push(ch);
        } else {
            if !token.is_empty() {
                out.push_str(&maybe_fix(&token, &target_lower, tol, word));
                token.clear();
            }
            out.push(ch);
        }
    }
    if !token.is_empty() {
        out.push_str(&maybe_fix(&token, &target_lower, tol, word));
    }
    out
}

/// 单个拉丁词：与热词足够接近（编辑距离 <= 容忍值）则替换为热词。
fn maybe_fix(token: &str, target_lower: &[char], tol: usize, word: &str) -> String {
    let lower: Vec<char> = token.to_lowercase().chars().collect();
    // 长度差超过 1 时不替换，避免把短词误替换成长热词（如 lang -> Golang）
    if lower.len().abs_diff(target_lower.len()) > 1 {
        return token.to_string();
    }
    if lower == target_lower || edit_distance(&lower, target_lower) <= tol {
        return word.to_string();
    }
    token.to_string()
}

/// 中文等连续文字修正：只做等长窗口替换（仅纠错字，绝不吞掉或增删词前后文字）。
/// 仅对 >=3 字的热词启用（短词模糊替换误伤风险过高，靠 initial_prompt 兜底）。
fn correct_cjk(text: &str, target: &[char]) -> String {
    if target.len() < 3 {
        return text.to_string();
    }
    let tol = if target.len() >= 8 { 2 } else { 1 };
    let len = target.len();
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i + len <= chars.len() {
        if edit_distance(&chars[i..i + len], target) <= tol {
            let fixed: String = target.iter().collect();
            out.push_str(&fixed);
            i += len;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    while i < chars.len() {
        out.push(chars[i]);
        i += 1;
    }
    out
}

use tauri::Manager;

/// 词典文件名（位于配置目录，与 config.json 同级）。
const DICT_FILE: &str = "hotwords.json";

/// 导出当前热词到配置目录下的 hotwords.json（JSON 数组），返回导出词数。
/// 用户可复制该文件备份或带到其他设备。
pub fn export_to_file(app: &tauri::AppHandle) -> Result<usize, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "无法获取配置目录".to_string())?;
    let cfg = crate::config::load(app);
    let words: Vec<&str> = cfg
        .hotwords
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();
    let json = serde_json::to_string_pretty(&words).map_err(|e| format!("序列化热词失败: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    std::fs::write(dir.join(DICT_FILE), json).map_err(|e| format!("写入词典失败: {e}"))?;
    Ok(words.len())
}

/// 从配置目录的 hotwords.json 导入热词（JSON 数组或每行一词的纯文本），
/// 与现有热词合并去重后写回 config.json；返回新增词数，下次录音即生效。
pub fn import_from_file(app: &tauri::AppHandle) -> Result<usize, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "无法获取配置目录".to_string())?;
    let text = std::fs::read_to_string(dir.join(DICT_FILE)).map_err(|_| {
        "未找到 hotwords.json（请将其放入应用配置目录后重试）".to_string()
    })?;
    let incoming = parse_dict(&text)?;

    // 读原始 config.json（可能不存在），保留其他字段，只更新 hotwords
    let cfg_path = dir.join("config.json");
    let mut root: serde_json::Value = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !root.is_object() {
        // config.json 内容不是对象时（异常情况）退回空配置，避免索引 panic
        root = serde_json::json!({});
    }
    let mut merged: Vec<String> = root
        .get("hotwords")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let mut added = 0usize;
    for w in incoming {
        let w = w.trim().to_string();
        if w.is_empty() || merged.iter().any(|e| e.eq_ignore_ascii_case(&w)) {
            continue;
        }
        merged.push(w);
        added += 1;
    }
    root["hotwords"] =
        serde_json::Value::Array(merged.iter().map(|w| serde_json::Value::String(w.clone())).collect());
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let out = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化配置失败: {e}"))?;
    std::fs::write(&cfg_path, out).map_err(|e| format!("写入配置失败: {e}"))?;
    Ok(added)
}

/// 解析词典文件内容：JSON 数组，或每行一词的纯文本。
fn parse_dict(text: &str) -> Result<Vec<String>, String> {
    if let Ok(v) = serde_json::from_str::<Vec<String>>(text) {
        return Ok(v);
    }
    Ok(text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_为空_当无热词() {
        assert_eq!(build_initial_prompt(&[]), "");
        assert_eq!(build_initial_prompt(&["  ".to_string()]), "");
    }

    #[test]
    fn prompt_包含热词列表() {
        let p = build_initial_prompt(&["张三".to_string(), "Kubernetes".to_string()]);
        assert!(p.contains("张三") && p.contains("Kubernetes"));
    }

    #[test]
    fn 拉丁词_大小写不敏感精确替换() {
        assert_eq!(correct("i use kubernetes daily", &["Kubernetes".into()]), "i use Kubernetes daily");
    }

    #[test]
    fn 拉丁词_编辑距离修正() {
        assert_eq!(correct("kubeernetes is great", &["kubernetes".into()]), "kubernetes is great");
        assert_eq!(correct("Kuberntes rocks", &["Kubernetes".into()]), "Kubernetes rocks");
    }

    #[test]
    fn 拉丁词_不影响正常词() {
        assert_eq!(correct("the cat sat", &["Kubernetes".into()]), "the cat sat");
    }

    #[test]
    fn 短热词只精确替换() {
        assert_eq!(correct("rust is fun", &["Rust".into()]), "Rust is fun");
        // 2 字符词不允许模糊修正，避免误伤
        assert_eq!(correct("go lang", &["Golang".into()]), "go lang");
    }

    #[test]
    fn 中文_三字词_单字错误修正() {
        assert_eq!(correct("联系欧阳三", &["欧阳晓".into()]), "联系欧阳晓");
    }

    #[test]
    fn 中文_短词不模糊替换() {
        assert_eq!(correct("这是张山的项目", &["张三".into()]), "这是张山的项目");
    }

    #[test]
    fn 解析_JSON数组词典() {
        let v = parse_dict("[\"张三\", \"Kubernetes\"]").unwrap();
        assert_eq!(v, vec!["张三".to_string(), "Kubernetes".to_string()]);
    }

    #[test]
    fn 解析_每行一词词典() {
        let v = parse_dict("张三\nKubernetes\n\n  \n李四").unwrap();
        assert_eq!(v, vec!["张三".to_string(), "Kubernetes".to_string(), "李四".to_string()]);
    }

    #[test]
    fn 解析_空内容() {
        let v = parse_dict("").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn 中文_正确词不改动() {
        assert_eq!(correct("张三来了", &["张三".into()]), "张三来了");
    }

    #[test]
    fn 中文_多字术语容错() {
        // 等长错字替换（4 字术语，tol=1）；词前后文字不受影响
        assert_eq!(correct("部署到库伯内提士了", &["库伯内提斯".into()]), "部署到库伯内提斯了");
    }
}