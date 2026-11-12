import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type LevelPayload = { level: number };
type TextPayload = { text: string };

type CleanupSettings = {
  enabled: boolean;
  endpoint: string | null;
  model: string | null;
  apiKey: string | null;
  prompt: string | null;
};

type Settings = {
  hotkeyModifiers: string;
  hotkeyKey: string;
  mode: string;
  model: string;
  language: string;
  cleanup: CleanupSettings;
  hotwords: string[];
};

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
  // ---------- 主窗口：设置界面 ----------
  const $ = (id: string) => document.getElementById(id)!;

  const saveTip = $("save-tip")!;
  const statusEl = $("status")!;
  const resultEl = $("result")!;
  const progressEl = $("progress")!;
  resultEl.style.whiteSpace = "pre-wrap";

  function tip(text: string, isError = false) {
    saveTip.textContent = text;
    saveTip.style.color = isError ? "#ef4444" : "#22c55e";
    if (!isError) {
      window.setTimeout(() => {
        if (saveTip.textContent === text) saveTip.textContent = "";
      }, 4000);
    }
  }

  async function loadSettings() {
    try {
      const s = await invoke<Settings>("get_settings");
      const mods = (s.hotkeyModifiers || "").split("+");
      for (const m of ["control", "shift", "alt", "super"]) {
        ($(`mod-${m}`) as HTMLInputElement).checked = mods.includes(m);
      }
      ($("hotkey-key") as HTMLSelectElement).value = s.hotkeyKey || "space";
      ($(`mode-${s.mode === "toggle" ? "toggle" : "hold"}`) as HTMLInputElement).checked = true;
      ($("model") as HTMLSelectElement).value = s.model || "base";
      ($("language") as HTMLSelectElement).value = s.language || "auto";
      ($("cleanup-enabled") as HTMLInputElement).checked = !!s.cleanup?.enabled;
      ($("cleanup-endpoint") as HTMLInputElement).value = s.cleanup?.endpoint || "";
      ($("cleanup-model") as HTMLInputElement).value = s.cleanup?.model || "";
      ($("cleanup-api-key") as HTMLInputElement).value = s.cleanup?.apiKey || "";
      ($("cleanup-prompt") as HTMLTextAreaElement).value = s.cleanup?.prompt || "";
      ($("hotwords") as HTMLTextAreaElement).value = (s.hotwords || []).join("\n");
    } catch (e) {
      tip("读取设置失败：" + String(e), true);
    }
  }

  function collectSettings(): Settings {
    const mods = ["control", "shift", "alt", "super"].filter(
      (m) => ($(`mod-${m}`) as HTMLInputElement).checked
    );
    const mode = ($("mode-toggle") as HTMLInputElement).checked ? "toggle" : "hold";
    return {
      hotkeyModifiers: mods.length > 0 ? mods.join("+") : "control",
      hotkeyKey: ($("hotkey-key") as HTMLSelectElement).value,
      mode,
      model: ($("model") as HTMLSelectElement).value,
      language: ($("language") as HTMLSelectElement).value,
      cleanup: {
        enabled: ($("cleanup-enabled") as HTMLInputElement).checked,
        endpoint: ($("cleanup-endpoint") as HTMLInputElement).value.trim(),
        model: ($("cleanup-model") as HTMLInputElement).value.trim(),
        apiKey: ($("cleanup-api-key") as HTMLInputElement).value,
        prompt: ($("cleanup-prompt") as HTMLTextAreaElement).value,
      },
      hotwords: ($("hotwords") as HTMLTextAreaElement).value.split("\n"),
    };
  }

  $("save").addEventListener("click", async () => {
    const btn = $("save") as HTMLButtonElement;
    btn.disabled = true;
    tip("保存中…");
    try {
      await invoke("save_settings", { settings: collectSettings() });
      tip("已保存，快捷键与模式立即生效");
    } catch (e) {
      tip(String(e), true);
    } finally {
      btn.disabled = false;
    }
  });

  $("hotwords-import").addEventListener("click", async () => {
    try {
      const n = await invoke<number>("import_hotwords");
      await loadSettings();
      tip(`导入完成，新增 ${n} 个热词`);
    } catch (e) {
      tip(String(e), true);
    }
  });

  $("hotwords-export").addEventListener("click", async () => {
    try {
      // 先保存当前表单里的热词，再导出
      await invoke("save_settings", { settings: collectSettings() });
      const n = await invoke<number>("export_hotwords");
      tip(`已导出 ${n} 个热词到配置目录 hotwords.json`);
    } catch (e) {
      tip(String(e), true);
    }
  });

  loadSettings();

  // ---- 录音状态与识别结果（轻量展示） ----
  function setStatus(text: string, recording: boolean) {
    statusEl.textContent = text;
    statusEl.style.color = recording ? "#ef4444" : "#6b7280";
  }

  setStatus("空闲", false);

  listen("recording-started", () => setStatus("● 正在录音…", true));
  listen("recording-stopped", () => setStatus("识别中…", false));
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