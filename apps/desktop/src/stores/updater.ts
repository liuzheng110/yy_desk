import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { backend, UPDATE_DOWNLOAD_PROGRESS_EVENT } from "@/lib/backend";
import type { DownloadedUpdate, UpdateDownloadProgress, UpdateInfo } from "@/lib/backend";
import { useAppStore } from "./app";
import { APP_NAME } from "@/lib/identity";

export type UpdateState = "idle" | "checking" | "available" | "downloading" | "downloaded" | "none";

// 启动后延迟自动检查，避免与首帧渲染争抢资源
const AUTO_CHECK_DELAY_MS = 3000;

const isTauri = typeof window.__TAURI_INTERNALS__ !== "undefined";

/**
 * 自动更新状态机：检查 → 提示条 → 一键下载 → 安装并重启。
 * 生命周期由 App.vue 驱动（start/stop），标题栏与设置页都能读写同一份状态。
 */
export const useUpdaterStore = defineStore("updater", () => {
  const app = useAppStore();

  const info = ref<UpdateInfo | null>(null);
  const cached = ref<DownloadedUpdate | null>(null);
  const state = ref<UpdateState>("idle");
  const dismissed = ref(false);
  const error = ref("");
  const progress = ref<{ downloaded: number; total: number | null } | null>(null);
  const installing = ref(false);
  const initializing = ref(false);
  // 手动检查的结果反馈（已是最新 / 检查失败），避免“点了没反应”
  const notice = ref<{ kind: "info" | "error"; text: string } | null>(null);

  let unlistenProgress: UnlistenFn | null = null;
  let autoCheckTimer: number | null = null;
  let noticeTimer: number | null = null;
  let started = false;
  let lifecycle = 0;
  let activeAttempt: { id: string; version: string } | null = null;

  const bannerVisible = computed(
    () => !dismissed.value && ["available", "downloading", "downloaded"].includes(state.value),
  );
  const actionable = computed(() => state.value === "available" || state.value === "downloaded");
  const progressPercent = computed(() => {
    const current = progress.value;
    if (!current || !current.total) return null;
    return Math.min(100, Math.round((current.downloaded / current.total) * 100));
  });

  function showNotice(kind: "info" | "error", text: string) {
    notice.value = { kind, text };
    if (noticeTimer !== null) window.clearTimeout(noticeTimer);
    noticeTimer = window.setTimeout(
      () => {
        notice.value = null;
        noticeTimer = null;
      },
      kind === "error" ? 8000 : 3500,
    );
  }

  function dismissNotice() {
    notice.value = null;
    if (noticeTimer !== null) {
      window.clearTimeout(noticeTimer);
      noticeTimer = null;
    }
  }

  function attemptId(): string {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
      return crypto.randomUUID();
    }
    return `attempt-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  async function check(manual = false) {
    if (initializing.value || installing.value || state.value === "checking" || state.value === "downloading") return;
    if (cached.value) {
      state.value = "downloaded";
      if (manual) {
        dismissed.value = false;
        showNotice("info", `版本 ${cached.value.version} 已下载，可点击更新按钮继续安装。`);
      }
      return;
    }
    state.value = "checking";
    error.value = "";
    if (manual) dismissNotice();
    try {
      const result = await backend.checkForUpdates();
      info.value = result;
      state.value = result.update_available ? "available" : "none";
      if (manual) dismissed.value = false;
      // 自动检查时静默，只有用户主动点“检查更新”才提示“已是最新”
      if (!result.update_available && manual) {
        showNotice("info", `已是最新版本（v${result.current_version}）`);
      }
    } catch (err) {
      console.error("[UPDATE] check failed", err);
      error.value = String(err);
      state.value = "idle";
      if (manual) showNotice("error", `检查更新失败：${String(err)}`);
    }
  }

  // 启动时先看缓存里是否已有校验通过的更新包，避免重复下载。
  async function restore(generation = lifecycle): Promise<boolean> {
    try {
      const found = await backend.getDownloadedUpdate();
      if (generation !== lifecycle) return false;
      cached.value = found;
      if (!found) return false;
      info.value = {
        current_version: app.version,
        latest_version: found.version,
        update_available: true,
        portable_mode: found.portable_mode,
        manual_update_only: false,
        release_name: `${APP_NAME} v${found.version}`,
        release_url: found.release_url,
        release_notes: found.release_notes,
      };
      state.value = "downloaded";
      return true;
    } catch (err) {
      console.error("[UPDATE] restore cached update failed", err);
      return false;
    }
  }

  async function install() {
    const target = cached.value;
    if (!target || installing.value || initializing.value) return;
    installing.value = true;
    error.value = "";
    try {
      // 安装成功会重启应用，这里的 Promise 通常不会 resolve。
      await backend.installDownloadedUpdate(target.cache_id, target.version);
    } catch (err) {
      console.error("[UPDATE] install failed", err);
      error.value = String(err);
      // A failed revalidation can evict the backend cache. Reconcile before
      // offering retry so the UI cannot stay stuck on a nonexistent package.
      if (!(await restore())) {
        cached.value = null;
        state.value = info.value?.update_available ? "available" : "idle";
      }
    } finally {
      installing.value = false;
    }
  }

  async function downloadAndInstall() {
    if (initializing.value || installing.value || state.value !== "available") return;
    if (cached.value) {
      state.value = "downloaded";
      await install();
      return;
    }
    const target = info.value;
    if (!target) return;
    state.value = "downloading";
    error.value = "";
    progress.value = { downloaded: 0, total: null };
    dismissed.value = false;
    activeAttempt = { id: attemptId(), version: target.latest_version.replace(/^v/, "") };
    try {
      const downloaded = await backend.downloadUpdate(
        target.latest_version,
        activeAttempt.id,
        target.release_notes || "",
      );
      cached.value = downloaded;
      state.value = "downloaded";
      await install();
    } catch (err) {
      console.error("[UPDATE] download failed", err);
      error.value = String(err);
      state.value = "available";
    } finally {
      activeAttempt = null;
    }
  }

  function primaryAction() {
    if (state.value === "downloaded") {
      void install();
    } else {
      void downloadAndInstall();
    }
  }

  async function onClick() {
    if (initializing.value || installing.value || state.value === "checking" || state.value === "downloading") return;
    dismissed.value = false;
    if (state.value === "available") {
      await downloadAndInstall();
      return;
    }
    if (state.value === "downloaded") {
      await install();
      return;
    }
    await check(true);
  }

  function dismiss() {
    dismissed.value = true;
  }

  function title(): string {
    switch (state.value) {
      case "checking":
        return "正在检查更新…";
      case "available":
        return `发现新版本 ${info.value?.latest_version ?? ""}，点击下载并重启安装`;
      case "downloading":
        return "正在下载更新…";
      case "downloaded":
        return "更新已下载，点击重启安装";
      case "none":
        return "已是最新版本";
      default:
        return "检查更新";
    }
  }

  /** 挂载时调用：订阅下载进度、恢复缓存更新、安排自动检查。 */
  async function start() {
    if (started || !isTauri) return;
    started = true;
    initializing.value = true;
    const generation = ++lifecycle;
    try {
      await app.init();
      if (generation !== lifecycle) return;
      const unlisten = await listen<UpdateDownloadProgress>(UPDATE_DOWNLOAD_PROGRESS_EVENT, (event) => {
        if (
          generation !== lifecycle ||
          !activeAttempt ||
          event.payload.attempt_id !== activeAttempt.id ||
          event.payload.version !== activeAttempt.version
        )
          return;
        progress.value = { downloaded: event.payload.downloaded, total: event.payload.total };
      });
      if (generation !== lifecycle) {
        unlisten();
        return;
      }
      unlistenProgress = unlisten;
      if (await restore(generation)) return;
      if (generation !== lifecycle) return;
      autoCheckTimer = window.setTimeout(() => {
        void check();
      }, AUTO_CHECK_DELAY_MS);
    } catch (err) {
      if (generation === lifecycle) {
        stop();
        error.value = String(err);
      }
    } finally {
      if (generation === lifecycle) initializing.value = false;
    }
  }

  /** 卸载时调用：释放事件订阅与定时器。 */
  function stop() {
    lifecycle += 1;
    initializing.value = false;
    unlistenProgress?.();
    unlistenProgress = null;
    if (autoCheckTimer !== null) {
      window.clearTimeout(autoCheckTimer);
      autoCheckTimer = null;
    }
    if (noticeTimer !== null) {
      window.clearTimeout(noticeTimer);
      noticeTimer = null;
    }
    started = false;
  }

  return {
    info,
    cached,
    state,
    dismissed,
    error,
    progress,
    installing,
    initializing,
    notice,
    bannerVisible,
    actionable,
    progressPercent,
    check,
    install,
    downloadAndInstall,
    primaryAction,
    onClick,
    dismiss,
    dismissNotice,
    title,
    start,
    stop,
  };
});
