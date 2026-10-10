// 主题：light / dark / system。
// 结果写到 <html data-theme="light|dark">，全局样式与第三方样式都基于它切换。
export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "myapp.theme";

export function getStoredTheme(): ThemeMode {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    // localStorage 不可用（隐私模式等）时按跟随系统处理
  }
  return "system";
}

function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode !== "system") return mode;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** 只应用、不持久化（用于 system 模式下的实时跟随）。 */
export function applyTheme(mode: ThemeMode) {
  document.documentElement.dataset.theme = resolveTheme(mode);
}

export function setStoredTheme(mode: ThemeMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // 存不下就只在本次会话生效
  }
  applyTheme(mode);
}

/** 监听系统深浅色变化，返回取消订阅函数。 */
export function watchSystemTheme(onChange: () => void): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}
