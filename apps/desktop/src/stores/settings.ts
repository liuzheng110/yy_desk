import { defineStore } from "pinia";
import { ref } from "vue";
import { backend } from "@/lib/backend";
import { applyTheme, getStoredTheme, setStoredTheme, watchSystemTheme, type ThemeMode } from "@/lib/theme";

/** 桌面设置（开机启动、主题等）。按需加载，避免每次启动都读一遍。 */
export const useSettingsStore = defineStore("settings", () => {
  const autostart = ref(false);
  const autostartBusy = ref(false);
  const theme = ref<ThemeMode>(getStoredTheme());

  let loaded = false;
  let unwatchSystem: (() => void) | null = null;

  async function load() {
    if (loaded) return;
    loaded = true;
    autostart.value = await backend.getAutostartEnabled().catch(() => false);
  }

  /** 切换开机启动；失败时回滚到原值并抛错，由调用方提示。 */
  async function setAutostart(enabled: boolean) {
    const previous = autostart.value;
    autostartBusy.value = true;
    autostart.value = enabled;
    try {
      await backend.setAutostartEnabled(enabled);
    } catch (error) {
      autostart.value = previous;
      throw error;
    } finally {
      autostartBusy.value = false;
    }
  }

  /** 应用当前主题并开始跟随系统深浅色（幂等）。 */
  function initTheme() {
    applyTheme(theme.value);
    unwatchSystem?.();
    unwatchSystem = watchSystemTheme(() => {
      if (theme.value === "system") applyTheme("system");
    });
  }

  function setTheme(next: ThemeMode) {
    theme.value = next;
    setStoredTheme(next);
  }

  return { autostart, autostartBusy, theme, load, setAutostart, initTheme, setTheme };
});
