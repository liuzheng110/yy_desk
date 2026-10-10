<script setup lang="ts">
// 框架示例壳。这里就是你的业务 UI 的挂载点：
// 替换本组件的内容，其余（标题栏/关闭行为/更新/设置）保持框架即可。
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { backend, UPDATE_DOWNLOAD_PROGRESS_EVENT } from "./lib/backend";
import type { DownloadedUpdate, UpdateDownloadProgress, UpdateInfo } from "./lib/backend";
import McpPanel from "./components/McpPanel.vue";

// 启动后延迟自动检查，避免与首帧渲染争抢资源
const AUTO_CHECK_DELAY_MS = 3000;

const version = ref("");
const platform = ref("");
const update = ref<UpdateInfo | null>(null);
const cachedUpdate = ref<DownloadedUpdate | null>(null);
const updateState = ref<"idle" | "checking" | "available" | "downloading" | "downloaded" | "none">("idle");
const updateDismissed = ref(false);
const updateError = ref("");
const progress = ref<{ downloaded: number; total: number | null } | null>(null);
const installing = ref(false);

const autostart = ref(false);
const autostartBusy = ref(false);

// 功能页导航（左侧菜单；后续新增功能页时在此扩展）
const page = ref<"home" | "settings">("home");

const isTauri = typeof window.__TAURI_INTERNALS__ !== "undefined";

let unlistenProgress: UnlistenFn | null = null;
let autoCheckTimer: number | null = null;

// ---- 窗口控制（Overlay 标题栏下原生按钮被隐藏，由前端补齐标配）----
async function minimizeWindow() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().minimize();
}

async function toggleMaximizeWindow() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().toggleMaximize();
}

// 关闭走框架关闭状态机（CloseRequested → 前端确认 → 默认隐藏到托盘，托盘可退出）
async function closeWindow() {
  if (!isTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

// ---- 更新（启动自动检查 → 提示条 → 一键下载安装并重启）----
const updateBannerVisible = computed(
  () => !updateDismissed.value && ["available", "downloading", "downloaded"].includes(updateState.value),
);
const updateActionable = computed(() => updateState.value === "available" || updateState.value === "downloaded");
const progressPercent = computed(() => {
  const current = progress.value;
  if (!current || !current.total) return null;
  return Math.min(100, Math.round((current.downloaded / current.total) * 100));
});

function formatSize(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function newAttemptId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `attempt-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function checkUpdate(manual = false) {
  if (updateState.value === "checking" || updateState.value === "downloading") return;
  updateState.value = "checking";
  updateError.value = "";
  try {
    const info = await backend.checkForUpdates();
    update.value = info;
    updateState.value = info.update_available ? "available" : "none";
    if (manual) updateDismissed.value = false;
  } catch (error) {
    console.error("[UPDATE] check failed", error);
    updateError.value = String(error);
    updateState.value = "idle";
  }
}

// 启动时先看缓存里是否已有校验通过的更新包，避免重复下载。
async function restoreDownloadedUpdate(): Promise<boolean> {
  try {
    const cached = await backend.getDownloadedUpdate();
    if (!cached) return false;
    cachedUpdate.value = cached;
    update.value = {
      current_version: version.value,
      latest_version: cached.version,
      update_available: true,
      portable_mode: cached.portable_mode,
      manual_update_only: false,
      release_name: `MyApp v${cached.version}`,
      release_url: cached.release_url,
      release_notes: cached.release_notes,
    };
    updateState.value = "downloaded";
    return true;
  } catch (error) {
    console.error("[UPDATE] restore cached update failed", error);
    return false;
  }
}

async function installUpdate() {
  const cached = cachedUpdate.value;
  if (!cached) return;
  installing.value = true;
  updateError.value = "";
  try {
    // 安装成功会重启应用，这里的 Promise 通常不会 resolve。
    await backend.installDownloadedUpdate(cached.cache_id, cached.version);
  } catch (error) {
    console.error("[UPDATE] install failed", error);
    updateError.value = String(error);
    installing.value = false;
  }
}

async function downloadAndInstall() {
  const info = update.value;
  if (!info) return;
  updateState.value = "downloading";
  updateError.value = "";
  progress.value = { downloaded: 0, total: null };
  updateDismissed.value = false;
  try {
    const downloaded = await backend.downloadUpdate(info.latest_version, newAttemptId(), info.release_notes || "");
    cachedUpdate.value = downloaded;
    updateState.value = "downloaded";
    await installUpdate();
  } catch (error) {
    console.error("[UPDATE] download failed", error);
    updateError.value = String(error);
    updateState.value = "available";
  }
}

function onPrimaryUpdateAction() {
  if (updateState.value === "downloaded") {
    void installUpdate();
  } else {
    void downloadAndInstall();
  }
}

async function onUpdateClick() {
  if (updateState.value === "checking" || updateState.value === "downloading") return;
  updateDismissed.value = false;
  if (updateState.value === "available") {
    await downloadAndInstall();
    return;
  }
  if (updateState.value === "downloaded") {
    await installUpdate();
    return;
  }
  await checkUpdate(true);
}

function dismissUpdate() {
  updateDismissed.value = true;
}

function updateTitle(): string {
  switch (updateState.value) {
    case "checking":
      return "正在检查更新…";
    case "available":
      return `发现新版本 ${update.value?.latest_version ?? ""}，点击下载并重启安装`;
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

// ---- 导航：设置按钮跳转到设置功能页 ----
function openSettings() {
  page.value = "settings";
}

function goHome() {
  page.value = "home";
}

async function toggleAutostart(event: Event) {
  const enabled = (event.target as HTMLInputElement).checked;
  autostartBusy.value = true;
  try {
    await backend.setAutostartEnabled(enabled);
  } catch (error) {
    console.error("[SETTINGS] set autostart failed", error);
    autostart.value = !enabled; // 回滚
  } finally {
    autostartBusy.value = false;
  }
}

onMounted(async () => {
  version.value = await backend.getAppVersion().catch(() => "?");
  platform.value = await backend.getPlatform().catch(() => "?");
  if (!isTauri) return;

  autostart.value = await backend.getAutostartEnabled().catch(() => false);

  unlistenProgress = await listen<UpdateDownloadProgress>(UPDATE_DOWNLOAD_PROGRESS_EVENT, (event) => {
    progress.value = { downloaded: event.payload.downloaded, total: event.payload.total };
  });

  if (await restoreDownloadedUpdate()) return;
  autoCheckTimer = window.setTimeout(() => {
    void checkUpdate();
  }, AUTO_CHECK_DELAY_MS);
});

onBeforeUnmount(() => {
  unlistenProgress?.();
  if (autoCheckTimer !== null) window.clearTimeout(autoCheckTimer);
});
</script>

<template>
  <div class="shell">
    <!-- 细标题栏：左侧标题，右侧统一一排按钮（更新 / 设置 / 最小化 / 最大化 / 关闭） -->
    <div class="titlebar" data-tauri-drag-region>
      <span class="title" data-tauri-drag-region>MyApp</span>

      <div class="titlebar-actions">
        <!-- 检查更新（图标按钮） -->
        <button
          class="tb-btn-icon"
          :class="{
            'is-spinning': updateState === 'checking' || updateState === 'downloading',
            'has-badge': updateState === 'available' || updateState === 'downloaded',
          }"
          :title="updateTitle()"
          :aria-label="updateTitle()"
          :disabled="updateState === 'checking' || updateState === 'downloading'"
          @click="onUpdateClick"
        >
          <svg
            v-if="updateState === 'available' || updateState === 'downloaded'"
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" y1="15" x2="12" y2="3" />
          </svg>
          <svg
            v-else
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polyline points="23 4 23 10 17 10" />
            <polyline points="1 20 1 14 7 14" />
            <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
          </svg>
        </button>

        <!-- 设置（跳转设置功能页） -->
        <button
          class="tb-btn-icon"
          :class="{ active: page === 'settings' }"
          title="设置"
          aria-label="设置"
          @click="openSettings"
        >
          <svg
            viewBox="0 0 24 24"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path
              d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
            />
          </svg>
        </button>

        <!-- 窗口控制（最小化 / 最大化 / 关闭） -->
        <button class="wc-btn" title="最小化" aria-label="最小化" @click="minimizeWindow">&#x2013;</button>
        <button class="wc-btn" title="最大化 / 还原" aria-label="最大化" @click="toggleMaximizeWindow">&#x2610;</button>
        <button class="wc-btn wc-close" title="关闭" aria-label="关闭" @click="closeWindow">&#x2715;</button>
      </div>
    </div>

    <!-- 新版本提示条：启动检测到新版本 / 下载中 / 已下载时出现在标题栏下方 -->
    <div v-if="updateBannerVisible" class="update-banner">
      <div class="update-banner-body">
        <div class="update-banner-title">
          <template v-if="updateState === 'available'">发现新版本 v{{ update?.latest_version }}</template>
          <template v-else-if="updateState === 'downloading'">正在下载更新 v{{ update?.latest_version }}…</template>
          <template v-else>更新已下载完成，安装后将自动重启</template>
        </div>

        <div v-if="updateState === 'downloading'" class="update-banner-progress">
          <div class="update-progress-track">
            <div
              class="update-progress-fill"
              :class="{ indeterminate: progressPercent === null }"
              :style="progressPercent !== null ? { width: `${progressPercent}%` } : undefined"
            ></div>
          </div>
          <span class="update-progress-text">
            <template v-if="progress">
              {{ formatSize(progress.downloaded) }}<template v-if="progress.total"> / {{ formatSize(progress.total) }}（{{ progressPercent }}%）</template>
            </template>
            <template v-else>准备中…</template>
          </span>
        </div>
        <div v-else-if="update?.release_notes" class="update-banner-notes">{{ update.release_notes }}</div>
        <div v-if="updateError" class="update-banner-error">{{ updateError }}</div>
      </div>

      <div class="update-banner-actions">
        <button
          v-if="updateActionable"
          class="update-btn primary"
          :disabled="installing"
          @click="onPrimaryUpdateAction"
        >
          {{ updateState === "downloaded" ? "立即安装并重启" : "立即更新" }}
        </button>
        <button v-else class="update-btn" disabled>下载中…</button>
        <button class="update-btn" @click="dismissUpdate">稍后</button>
      </div>
    </div>

    <!-- 左侧导航菜单 + 功能页内容 -->
    <div class="body">
      <aside class="sidebar">
        <nav class="side-nav">
          <button
            class="nav-item"
            :class="{ active: page === 'home' }"
            title="主功能"
            @click="goHome"
          >
            <svg
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect x="3" y="3" width="7" height="7" rx="1" />
              <rect x="14" y="3" width="7" height="7" rx="1" />
              <rect x="3" y="14" width="7" height="7" rx="1" />
              <rect x="14" y="14" width="7" height="7" rx="1" />
            </svg>
            <span>主功能</span>
          </button>

          <button
            class="nav-item"
            :class="{ active: page === 'settings' }"
            title="设置"
            @click="openSettings"
          >
            <svg
              viewBox="0 0 24 24"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <circle cx="12" cy="12" r="3" />
              <path
                d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
              />
            </svg>
            <span>设置</span>
          </button>
        </nav>

        <div class="sidebar-footer">v{{ version }}</div>
      </aside>

      <main class="content">
        <!-- 主功能页 -->
        <section v-if="page === 'home'" class="page">
          <h1>MyApp 框架已就绪</h1>
          <p class="muted">
            版本 <code>{{ version }}</code> · 平台 <code>{{ platform }}</code>
          </p>
          <section class="cards">
            <div class="card">
              <h3>四大机制已保留</h3>
              <ul>
                <li>自动更新（updater 插件 + GitHub Releases + minisign 验签 + 便携包）</li>
                <li>安装打包（NSIS，`createUpdaterArtifacts: true` 生成 latest.json）</li>
                <li>启动机制（单实例 / 深链接 / 启动看门狗 / WebView2 崩溃恢复 / 窗口状态）</li>
                <li>运行机制（托盘 / 关闭行为状态机 / 数据目录三模式 / 迁移门槛）</li>
              </ul>
            </div>
            <div class="card">
              <h3>从这里开始写业务</h3>
              <ol>
                <li>后端：<code>src-tauri/src/commands/business/</code> 新建命令并在 <code>lib.rs</code> 注册</li>
                <li>前端：替换 <code>apps/desktop/src/App.vue</code></li>
                <li>配置：<code>src-tauri/tauri.conf.json</code>（名称 / 更新端点 / 签名公钥）</li>
              </ol>
            </div>
          </section>

          <McpPanel />
        </section>

        <!-- 设置页 -->
        <section v-else class="page">
          <h1>设置</h1>
          <div class="settings-card">
            <label class="setting-row" :class="{ busy: autostartBusy }">
              <input
                type="checkbox"
                v-model="autostart"
                :disabled="autostartBusy"
                @change="toggleAutostart"
              />
              <span>开机启动</span>
            </label>
            <div class="setting-row static">
              <span>应用版本</span>
              <span class="setting-value">{{ version }}</span>
            </div>
            <div class="setting-row static">
              <span>运行平台</span>
              <span class="setting-value">{{ platform }}</span>
            </div>
            <div class="setting-row static">
              <span>检查更新</span>
              <button class="update-btn" :disabled="updateState === 'checking'" @click="checkUpdate(true)">
                {{ updateTitle() }}
              </button>
            </div>
          </div>
        </section>
      </main>
    </div>
  </div>
</template>
