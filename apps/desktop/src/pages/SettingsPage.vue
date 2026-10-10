<script setup lang="ts">
// 设置页：开机启动、外观主题、版本信息、手动检查更新。
import { onMounted } from "vue";
import BaseToggle from "@/components/ui/BaseToggle.vue";
import { toErrorMessage } from "@/lib/errors";
import type { ThemeMode } from "@/lib/theme";
import { toastError } from "@/lib/toast";
import { useAppStore } from "@/stores/app";
import { useSettingsStore } from "@/stores/settings";
import { useUpdaterStore } from "@/stores/updater";

const app = useAppStore();
const settings = useSettingsStore();
const updater = useUpdaterStore();

onMounted(() => {
  void settings.load();
});

async function onAutostart(enabled: boolean) {
  try {
    await settings.setAutostart(enabled);
  } catch (error) {
    console.error("[SETTINGS] set autostart failed", error);
    toastError(`设置开机启动失败：${toErrorMessage(error)}`);
  }
}

function onThemeChange(event: Event) {
  settings.setTheme((event.target as HTMLSelectElement).value as ThemeMode);
}
</script>

<template>
  <section class="page">
    <h1>设置</h1>
    <div class="settings-card">
      <div class="setting-row">
        <span>开机启动</span>
        <BaseToggle
          :model-value="settings.autostart"
          :busy="settings.autostartBusy"
          aria-label="开机启动"
          @update:model-value="onAutostart"
        />
      </div>
      <div class="setting-row">
        <span>外观主题</span>
        <select class="theme-select" :value="settings.theme" aria-label="外观主题" @change="onThemeChange">
          <option value="system">跟随系统</option>
          <option value="light">浅色</option>
          <option value="dark">深色</option>
        </select>
      </div>
      <div class="setting-row static">
        <span>应用版本</span>
        <span class="setting-value">{{ app.version }}</span>
      </div>
      <div class="setting-row static">
        <span>运行平台</span>
        <span class="setting-value">{{ app.platform }}</span>
      </div>
      <div class="setting-row static">
        <span>检查更新</span>
        <button class="update-btn" :disabled="updater.state === 'checking'" @click="updater.check(true)">
          {{ updater.title() }}
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.theme-select {
  font: inherit;
  font-size: 12.5px;
  padding: 4px 8px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-app);
  color: var(--text-strong);
  cursor: pointer;
}

.theme-select:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 2px var(--focus-ring);
}
</style>
