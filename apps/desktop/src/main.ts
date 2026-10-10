import { createApp } from "vue";
import "./styles.css";
import { backend } from "./lib/backend";
import { markFrontendReady } from "./lib/startup";
import StartupGate from "./StartupGate.vue";

function renderStartupError(error: unknown) {
  const message = error instanceof Error ? [error.message, error.stack].filter(Boolean).join("\n") : String(error);
  console.error("[STARTUP] bootstrap failed", error);
  const loading = document.querySelector<HTMLDivElement>("#startup-loading");
  if (loading) loading.style.display = "none";
  const root = document.querySelector<HTMLDivElement>("#root");
  if (!root) return;
  const panel = document.createElement("div");
  panel.style.cssText =
    "display:flex;min-height:100vh;align-items:center;justify-content:center;background:#ffffff;color:#111827;padding:24px;font-family:ui-sans-serif,system-ui,sans-serif";
  const card = document.createElement("div");
  card.style.cssText =
    "max-width:760px;width:100%;border:1px solid #e5e7eb;border-radius:12px;padding:20px;box-shadow:0 10px 30px rgba(0,0,0,0.08);background:#fff";
  const title = document.createElement("h1");
  title.textContent = "MyApp 启动失败";
  title.style.cssText = "margin:0 0 12px;font-size:18px;font-weight:700";
  const text = document.createElement("p");
  text.textContent = "桌面界面在启动时崩溃，请复制下方错误信息并反馈。";
  text.style.cssText = "margin:0 0 12px;font-size:13px;line-height:1.5;color:#4b5563";
  const pre = document.createElement("pre");
  pre.textContent = message;
  pre.style.cssText =
    "margin:0;white-space:pre-wrap;word-break:break-word;font-size:12px;line-height:1.5;background:#f9fafb;border-radius:8px;padding:12px;overflow:auto";
  card.append(title, text, pre);
  panel.append(card);
  root.append(panel);
}

async function bootstrap() {
  console.log("[STARTUP] frontend bootstrap begin");

  // 桌面壳：等待后端存储就绪；Web 模式（pnpm dev:web）降级直接进入。
  if (typeof window.__TAURI_INTERNALS__ !== "undefined") {
    await backend.markFrontendReady().catch((error) => {
      console.warn("[STARTUP] mark_frontend_ready failed (safe to ignore in dev)", error);
    });
  }

  const app = createApp(StartupGate);
  app.mount("#root");

  const loading = document.querySelector<HTMLDivElement>("#startup-loading");
  if (loading) loading.style.display = "none";
  console.log("[STARTUP] vue mounted");
}

void bootstrap().catch(renderStartupError);
