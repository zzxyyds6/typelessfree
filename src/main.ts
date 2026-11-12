import { listen } from "@tauri-apps/api/event";

const app = document.getElementById("app")!;
const statusEl = document.createElement("p");
app.appendChild(statusEl);

function setStatus(text: string, recording: boolean) {
  statusEl.textContent = text;
  statusEl.style.color = recording ? "#ef4444" : "#6b7280";
}

setStatus("空闲（按 Ctrl+Shift+Space 说话）", false);

listen("recording-started", () => setStatus("● 正在录音…", true));
listen("recording-stopped", () => setStatus("空闲", false));
