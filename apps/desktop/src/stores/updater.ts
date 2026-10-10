import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { backend, UPDATE_DOWNLOAD_PROGRESS_EVENT } from "@/lib/backend";
import type { DownloadedUpdate, UpdateDownloadProgress, UpdateInfo } from "@/lib/backend";
import { useAppStore } from "./app";

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
  // 手动检查的结果反馈（已是最新 / 检查失败），避免“点了没反应”
  const notice = ref<{ kind: "info" | "error"; text: string } | null>(null);

  let unlistenProgress: UnlistenFn | null = null;
  let autoCheckTimer: number | null = null;
  let noticeTimer: number | null = null;
  let started = false;

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
    if (state.value === "checking" || state.value === "downloading") return;
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
      showNotice("error", `检查更新失败：${String(err)}`);
    }
  }

  // 启动时先看缓存里是否已有校验通过的更新包，避免重复下载。
  async function restore(): Promise<boolean> {
    try {
      const found = await backend.getDownloadedUpdate();
      if (!found) return false;
      cached.value = found;
      info.value = {
        current_version: app.version,
        latest_version: found.version,
        update_available: true,
        portable_mode: found.portable_mode,
        manual_update_only: false,
        release_name: `MyApp v${found.version}`,
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
    if (!target) return;
    installing.value = true;
    error.value = "";
    try {
      // 安装成功会重启应用，这里的 Promise 通常不会 resolve。
      await backend.installDownloadedUpdate(target.cache_id, target.version);
    } catch (err) {
      console.error("[UPDATE] install failed", err);
      error.value = String(err);
      installing.value = false;
    }
  }

  async function downloadAndInstall() {
    const target = info.value;
    if (!target) return;
    state.value = "downloading";
    error.value = "";
    progress.value = { downloaded: 0, total: null };
    dismissed.value = false;
    try {
      const downloaded = await backend.downloadUpdate(target.latest_version, attemptId(), target.release_notes || "");
      cached.value = downloaded;
      state.value = "downloaded";
      await install();
    } catch (err) {
      console.error("[UPDATE] download failed", err);
      error.value = String(err);
      state.value = "available";
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
    if (state.value === "checking" || state.value === "downloading") return;
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
    await app.init();
    unlistenProgress = await listen<UpdateDownloadProgress>(UPDATE_DOWNLOAD_PROGRESS_EVENT, (event) => {
      progress.value = { downloaded: event.payload.downloaded, total: event.payload.total };
    });
    if (await restore()) return;
    autoCheckTimer = window.setTimeout(() => {
      void check();
    }, AUTO_CHECK_DELAY_MS);
  }

  /** 卸载时调用：释放事件订阅与定时器。 */
  function stop() {
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
