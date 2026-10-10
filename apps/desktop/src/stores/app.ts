import { defineStore } from "pinia";
import { ref } from "vue";
import { backend } from "@/lib/backend";

/** 应用基础信息（版本 / 平台）。全局只拉取一次。 */
export const useAppStore = defineStore("app", () => {
  const version = ref("");
  const platform = ref("");

  let pending: Promise<void> | null = null;

  /** 幂等初始化：并发调用复用同一个 Promise。 */
  function init(): Promise<void> {
    if (!pending) {
      pending = (async () => {
        version.value = await backend.getAppVersion().catch(() => "?");
        platform.value = await backend.getPlatform().catch(() => "?");
      })();
    }
    return pending;
  }

  return { version, platform, init };
});
