# typelessfree

开源、免费、本地优先的 AI 语音输入工具。按住快捷键说话 → 本地语音识别 → 可选 AI 清理 → 自动插入当前光标位置。

对标 [Typeless](https://typeless.com/)，核心承诺：

- 🔒 **本地优先**：默认音频不离开设备，云端清理仅在你明确开启后才发送文本
- 💰 **完全免费**：无订阅、无字数限制、无强制账号
- 🔍 **开源可审计**：MIT 许可
- 🖥 **跨平台**：Windows 10/11、macOS（Apple Silicon + Intel）、主流 Linux

## 功能一览

| 功能 | 说明 |
|------|------|
| 全局快捷键 | 按住说话（hold-to-talk）或按一次开始/结束（toggle），快捷键与模式均可配置 |
| 本地语音识别 | whisper.cpp（默认 base 模型），缺失时自动从 HuggingFace 下载并显示进度 |
| 实时预览 | 录音时光标旁浮动卡片，显示音量与实时 partial 识别结果 |
| AI 文本清理（可选） | 去填充词、整理标点与格式；支持 Ollama 本地或任意 OpenAI 兼容 API，默认关闭 |
| 自动插入 | 识别结果直接粘贴到当前焦点应用的光标位置，并自动恢复原剪贴板 |
| 语言支持 | 中文 / English / 自动检测 |
| 个人词典 / 热词 | 专有名词、人名、术语优先识别；支持导入导出备份 |

## 使用方法

1. 启动后应用驻留系统托盘（默认快捷键 **Ctrl+Shift+Space**）。
2. **按住说话模式**：按住快捷键开始录音，松开后自动识别并插入光标位置。
3. **切换模式**：按一次开始录音，再按一次结束。
4. 首次使用会自动下载所选的 whisper 模型（base 约 142 MB），可在设置中更换。

> 首次识别时模型加载约 1-2 秒，属正常现象。

## 设置

从系统托盘菜单打开 **设置** 窗口，所有修改保存后**立即生效**，无需重启：

| 分类 | 可配置项 |
|------|----------|
| 快捷键 | 修饰键（Ctrl / Shift / Alt / Win·Cmd）+ 主键（Space、Q、Z、D、V、F1-F4）；hold / toggle 模式 |
| 语音识别 | 模型（tiny / base / small / medium / large-v3-turbo，也支持填写本地模型绝对路径）；语言（自动检测 / 中文 / English） |
| AI 文本清理 | 开关、OpenAI 兼容端点、模型名、API Key、清理提示词 |
| 个人词典 / 热词 | 多行编辑；一键导出到 `hotwords.json`、从配置目录导入 |

### 热词导入 / 导出

- **导出**：点击「导出到 hotwords.json」，当前热词写入应用配置目录。
- **导入**：把 `hotwords.json`（JSON 数组或每行一词的纯文本）放入配置目录，点击「导入」，与现有热词合并去重后生效。

## 配置文件

配置位于系统配置目录（应用标识 `io.github.zzxyyds6.typelessfree`）：

| 平台 | 路径 |
|------|------|
| Windows | `%APPDATA%\io.github.zzxyyds6.typelessfree\` |
| macOS | `~/Library/Application Support/io.github.zzxyyds6.typelessfree/` |
| Linux | `~/.config/io.github.zzxyyds6.typelessfree/` |

目录内容：

- `config.json` — 全部设置
- `hotwords.json` — 热词导出/导入文件
- 模型缓存位于数据目录（Windows 同上目录，macOS / Linux 见 Tauri [路径解析](https://tauri.app/develop/configuration-files/)）下的 `models/`

`config.json` 示例（字段均为 camelCase）：

```json
{
  "hotkeyModifiers": "control+shift",
  "hotkeyKey": "space",
  "mode": "hold",
  "model": "base",
  "language": "auto",
  "cleanup": {
    "enabled": false,
    "endpoint": "http://localhost:11434/v1/chat/completions",
    "model": "llama3.2",
    "apiKey": "",
    "prompt": "你是语音转文字的编辑助手……"
  },
  "hotwords": ["张三", "Kubernetes", "whisper.cpp"]
}
```

- `model` 可填模型名或本地 `.bin` 文件绝对路径
- `language` 设为 `"auto"` 时由 whisper 自动检测
- `cleanup.enabled` 为 `false`（默认）时，识别文本**绝不离开本机**

## 隐私说明

- 音频始终只在本地处理，识别使用本地 whisper.cpp 模型
- AI 清理默认**关闭**；开启后仅将识别后的**文本**（不含音频）发送到你指定的端点
- 无账号、无遥测、无后台上报

## 从源码构建

### 前置要求

- [Node.js](https://nodejs.org/) 18+ 与 [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) 1.77+
- Windows：MSVC Build Tools；macOS：Xcode Command Line Tools；Linux：`libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`（详见 [Tauri 前置要求](https://tauri.app/start/prerequisites/)）

### 开发调试

```bash
pnpm install
pnpm tauri dev
```

### 构建发布包

```bash
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`（Windows: `.msi` / `.exe`；macOS: `.app` / `.dmg`；Linux: `.deb` / `.AppImage` / `.rpm`）。

## 常见问题

| 问题 | 处理办法 |
|------|----------|
| 提示「快捷键可能已被其他程序占用」 | 在设置中换一组快捷键；或关闭占用该组合键的其他应用 |
| 保存新快捷键提示注册失败 | 同上，注册失败时应用会自动恢复原快捷键，不影响继续使用 |
| 文本没有插入到目标应用 | macOS 需在「系统设置 → 隐私与安全性 → 辅助功能」中授权应用；Linux 需安装 `xdotool`（`libxdo`）；部分以管理员权限运行的窗口无法接收模拟输入 |
| 模型下载失败 | 检查到 huggingface.co 的网络；也可手动下载对应 `ggml-<模型名>.bin` 放入数据目录 `models/` |
| 识别不准 / 专有名词错字 | 在设置中添加热词；或换用更大模型（small / medium / large-v3-turbo） |
| Linux 托盘不显示 | 需要系统托盘支持（`libayatana-appindicator`）；GNOME 需安装 AppIndicator 扩展 |

## 路线图

- [x] MVP：快捷键 + 本地识别 + 预览 + 插入 + 可选清理 + 热词 + 设置界面
- [ ] 打包发布（v0.1.0 Release）
- [ ] GPU 加速增强（CUDA / Metal 显式配置）
- [ ] 按应用自动调整语气 / 风格
- [ ] 历史记录与搜索
- [ ] 选中文字后语音指令编辑、翻译模式
- [ ] 移动端键盘形态

## 文档

- [产品需求文档（PRD）](docs/PRD.md)
- [架构决策记录（ADR）](docs/adr/)
- [Agent 协作配置](AGENTS.md)

## 参考项目

- [Handy](https://github.com/cjpais/Handy)：主要参考架构
- [typeless-but-free](https://github.com/AgentGameLab/typeless-but-free)：功能与交互参考

## 许可

[MIT](LICENSE)