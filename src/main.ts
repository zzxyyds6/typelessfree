import { listen } from "@tauri-apps/api/event";

type LevelPayload = { level: number };
type TextPayload = { text: string };

// ---------- 浮动预览卡窗口（URL hash 为 #preview） ----------
if (location.hash === "#preview") {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <div class="preview-card">
      <div class="preview-header">
        <span class="mic-dot"></span>
        <span class="preview-title">正在聆听…</span>
      </div>
      <div class="level-track"><div class="level-bar" id="level-bar"></div></div>
      <div class="partial" id="partial">开始说话…</div>
    </div>
  `;

  const levelBar = document.getElementById("level-bar")!;
  const partial = document.getElementById("partial")!;
  const title = app.querySelector(".preview-title")!;

  listen("recording-started", () => {
    title.textContent = "正在聆听…";
    partial.textContent = "开始说话…";
    levelBar.style.width = "0%";
  });

  listen("recording-level", (e) => {
    const { level } = e.payload as LevelPayload;
    levelBar.style.width = `${Math.round(level * 100)}%`;
  });

  listen("language-detected", (e) => {
    const { code } = e.payload as { code: string };
    title.textContent = `正在聆听…（${code.toUpperCase()}）`;
  });

  listen("partial-result", (e) => {
    const { text } = e.payload as TextPayload;
    if (text) {
      partial.textContent = text;
    }
  });

  listen("recording-stopped", () => {
    title.textContent = "识别中…";
    partial.textContent = "正在整理文字…";
  });

  listen("app-error", (e) => {
    const { message } = e.payload as { message: string };
    title.textContent = "出错了";
    partial.textContent = message;
  });
} else {
  // ---------- 主窗口（调试/状态面板，后续替换为设置界面） ----------
  const app = document.getElementById("app")!;
  const statusEl = document.createElement("p");
  const resultEl = document.createElement("pre");
  const progressEl = document.createElement("p");
  app.appendChild(statusEl);
  app.appendChild(resultEl);
  app.appendChild(progressEl);
  resultEl.style.whiteSpace = "pre-wrap";
  progressEl.style.fontSize = "12px";

  function setStatus(text: string, recording: boolean) {
    statusEl.textContent = text;
    statusEl.style.color = recording ? "#ef4444" : "#6b7280";
  }

  setStatus("空闲（按 Ctrl+Shift+Space 说话）", false);

  listen("recording-started", () => setStatus("● 正在录音…", true));
  listen("recording-stopped", () => setStatus("识别中…", false));
  listen("language-detected", (e) => {
    const { code } = e.payload as { code: string };
    title.textContent = `正在聆听…（${code.toUpperCase()}）`;
  });

  listen("partial-result", (e) => {
    const { text } = e.payload as TextPayload;
    if (text) resultEl.textContent = text;
  });
  listen("transcription-result", (e) => {
    const { text } = e.payload as TextPayload;
    resultEl.textContent = text || "（未识别到内容）";
    setStatus("空闲", false);
  });
  listen("app-error", (e) => {
    const { message } = e.payload as { message: string };
    setStatus("出错：" + message, false);
  });
  listen("model-download-progress", (e) => {
    const { received, total } = e.payload as { received: number; total: number };
    const mb = (n: number) => (n / 1024 / 1024).toFixed(1);
    progressEl.textContent =
      total > 0 ? `模型下载：${mb(received)} / ${mb(total)} MB` : `模型下载：${mb(received)}`;
  });
  listen("model-download-finished", () => {
    progressEl.textContent = "模型下载完成";
  });
}