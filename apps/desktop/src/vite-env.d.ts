/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<{}, {}, any>;
  export default component;
}

interface Window {
  /** Tauri 注入的全局标记，用于区分桌面壳与纯 Web 模式。 */
  __TAURI_INTERNALS__?: unknown;
}
