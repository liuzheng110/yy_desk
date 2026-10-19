import { createApp, nextTick } from "vue";
import { createPinia } from "pinia";
import "./styles.css";
import { toErrorMessage } from "./lib/errors";
import { connectStartupEvents, markFrontendReady } from "./lib/startup";
import { APP_NAME } from "./lib/identity";
import { toastError } from "./lib/toast";
import router from "./router";
import { useSettingsStore } from "./stores/settings";
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
  title.textContent = `${APP_NAME} 启动失败`;
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

  const isTauri = typeof window.__TAURI_INTERNALS__ !== "undefined";
  const app = createApp(StartupGate);
  let startupComplete = false;
  let mountError: unknown = null;
  app.use(createPinia());
  app.use(router);
  // 全局兜底：组件外抛出的错误统一打日志并弹 Toast，避免静默失败。
  app.config.errorHandler = (error, _instance, info) => {
    if (!startupComplete) mountError = error;
    console.error("[UI] global error", error, info);
    toastError(toErrorMessage(error));
  };
  // 尽早应用主题，避免首帧闪白
  useSettingsStore().initTheme();
  await router.isReady();
  const disconnect = isTauri ? await connectStartupEvents() : () => {};
  app.onUnmount(disconnect);
  try {
    app.mount("#root");
    await nextTick();
    if (mountError) throw mountError;
    if (isTauri) await markFrontendReady();
    startupComplete = true;
  } catch (error) {
    disconnect();
    app.unmount();
    throw error;
  }

  const loading = document.querySelector<HTMLDivElement>("#startup-loading");
  if (loading) loading.style.display = "none";
  console.log("[STARTUP] vue mounted");
}

void bootstrap().catch(renderStartupError);
