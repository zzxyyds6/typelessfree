import { listen } from "@tauri-apps/api/event";

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
listen("transcription-result", (e) => {
  const { text } = e.payload as { text: string };
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
