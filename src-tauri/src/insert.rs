//! 文本插入：把识别结果粘贴到当前焦点应用的光标位置。
//! 策略：备份剪贴板 → 写入识别文本 → 模拟 Ctrl+V（macOS 为 Cmd+V）→ 恢复剪贴板。
//! 注意：macOS 需要在系统设置中授予“辅助功能”权限；Windows 某些高权限窗口可能拒绝模拟输入。

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

/// 将文本插入当前焦点应用光标位置。
pub fn insert_text(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("打开剪贴板失败: {e}"))?;

    // 1. 备份原剪贴板内容（可能为空/不可读，允许失败）
    let backup = clipboard.get_text().ok();

    // 2. 写入识别文本
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("写入剪贴板失败: {e}"))?;

    // 3. 模拟粘贴快捷键（macOS 用 Cmd+V）
    let (modifier, paste_key) = if cfg!(target_os = "macos") {
        (Key::Meta, Key::Unicode('v'))
    } else {
        (Key::Control, Key::Unicode('v'))
    };

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("初始化输入模拟失败: {e}"))?;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("按下修饰键失败: {e}"))?;
    // 给系统留一点反应时间
    std::thread::sleep(Duration::from_millis(60));
    enigo
        .key(paste_key, Direction::Click)
        .map_err(|e| format!("模拟 V 键失败: {e}"))?;
    std::thread::sleep(Duration::from_millis(60));
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("释放修饰键失败: {e}"))?;

    // 4. 恢复原剪贴板内容
    if let Some(original) = backup {
        let _ = clipboard.set_text(original);
    }

    Ok(())
}