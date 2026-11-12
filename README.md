# typelessfree

开源、免费、本地优先的 AI 语音输入工具。按住快捷键说话 → 本地语音识别 → 可选 AI 清理 → 自动插入当前光标位置。

对标 [Typeless](https://typeless.com/)，核心承诺：

- 🔒 **本地优先**：默认音频不离开设备，云端清理仅在你明确开启后才发送文本
- 💰 **完全免费**：无订阅、无字数限制、无强制账号
- 🔍 **开源可审计**：MIT 许可

## 功能

- 全局快捷键（按住说话 / 切换模式）
- 本地语音识别（whisper.cpp，支持 GPU 加速与 CPU 回退）
- 录音浮动预览卡与实时识别结果
- 可选 AI 文本清理（Ollama 或任意 OpenAI 兼容 API）
- 识别结果自动插入光标位置，恢复原剪贴板
- 中英文支持，个人词典 / 热词

## 安装与开发

### 前置要求

- [Node.js](https://nodejs.org/) 18+ 与 [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) 1.77+
- Windows：MSVC Build Tools；Linux：`libwebkit2gtk-4.1-dev` 等（见 [Tauri 文档](https://tauri.app/start/prerequisites/)）；macOS：Xcode Command Line Tools

### 本地开发

```bash
pnpm install
pnpm tauri dev
```

### 构建发布包

```bash
pnpm tauri build
```

## 文档

- [产品需求文档（PRD）](docs/PRD.md)
- [架构决策记录（ADR）](docs/adr/)
- [Agent 协作配置](AGENTS.md)

## 参考项目

- [Handy](https://github.com/cjpais/Handy)：主要参考架构
- [typeless-but-free](https://github.com/AgentGameLab/typeless-but-free)：功能与交互参考

## 许可

[MIT](LICENSE)
