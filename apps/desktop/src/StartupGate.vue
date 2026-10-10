<script setup lang="ts">
// 启动门：进入业务 UI 前完成框架握手（关闭确认、二次启动文件/深链接订阅）。
import { onMounted } from "vue";
import App from "./App.vue";
import { backend } from "./lib/backend";
import { markFrontendReady, onAppCloseRequested, onAppOpenDeepLinks, onAppOpenFiles } from "./lib/startup";

const isTauri = typeof window.__TAURI_INTERNALS__ !== "undefined";

async function handleCloseRequested() {
  // 关闭行为：窗口关闭与托盘退出都直接退出进程（不驻留后台）。
  // 如需改为「关闭=最小化到托盘」，把这里换成 completeAppClose("hide") 即可。
  await backend.completeAppClose("quit").catch(console.error);
}

onMounted(async () => {
  if (!isTauri) return;
  await markFrontendReady().catch(() => {});
  await onAppCloseRequested(handleCloseRequested);
  await onAppOpenFiles((paths) => {
    console.log("[STARTUP] open files from second instance:", paths);
  });
  await onAppOpenDeepLinks((links) => {
    console.log("[STARTUP] deep links from second instance:", links);
  });
});
</script>

<template>
  <App />
</template>
