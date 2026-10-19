<script setup lang="ts">
// 应用外壳：细标题栏（拖拽 + 窗口控制 + 检查更新）、更新提示条、左侧导航、内容区 <RouterView>。
// 业务页面放 src/pages/，路由表在 src/router/index.ts；新增页面时在侧边导航里加一个 <RouterLink>。
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import ErrorBoundary from "@/components/ErrorBoundary.vue";
import ToastHost from "@/components/ToastHost.vue";
import { useAppStore } from "@/stores/app";
import { useUpdaterStore } from "@/stores/updater";
import { APP_NAME } from "@/lib/identity";

const route = useRoute();
const app = useAppStore();
const updater = useUpdaterStore();

const contentRef = ref<HTMLElement | null>(null);

const isTauri = typeof window.__TAURI_INTERNALS__ !== "undefined";

function formatSize(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

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

// 换页时把内容区滚回顶部（滚动容器是 .content，不是 window）
watch(
  () => route.fullPath,
  () => contentRef.value?.scrollTo({ top: 0 }),
);

onMounted(() => {
  void app.init();
  void updater.start();
});

onBeforeUnmount(() => {
  updater.stop();
});
</script>

<template>
  <div class="shell">
    <!-- 细标题栏：左侧标题，右侧统一一排按钮（更新 / 设置 / 最小化 / 最大化 / 关闭） -->
    <div class="titlebar" data-tauri-drag-region>
      <span class="title" data-tauri-drag-region>{{ APP_NAME }}</span>

      <div class="titlebar-actions">
        <!-- 检查更新（图标按钮） -->
        <button
          class="tb-btn-icon"
          :class="{
            'is-spinning': updater.state === 'checking' || updater.state === 'downloading',
            'has-badge': updater.state === 'available' || updater.state === 'downloaded',
          }"
          :title="updater.title()"
          :aria-label="updater.title()"
          :disabled="updater.state === 'checking' || updater.state === 'downloading' || updater.installing"
          @click="updater.onClick"
        >
          <svg
            v-if="updater.state === 'available' || updater.state === 'downloaded'"
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
        <RouterLink to="/settings" class="tb-btn-icon" active-class="active" title="设置" aria-label="设置">
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
        </RouterLink>

        <!-- 窗口控制（最小化 / 最大化 / 关闭） -->
        <button class="wc-btn" title="最小化" aria-label="最小化" @click="minimizeWindow">&#x2013;</button>
        <button class="wc-btn" title="最大化 / 还原" aria-label="最大化" @click="toggleMaximizeWindow">&#x2610;</button>
        <button class="wc-btn wc-close" title="关闭" aria-label="关闭" @click="closeWindow">&#x2715;</button>
      </div>
    </div>

    <!-- 检查更新反馈：手动检查“已是最新”或检查失败时出现，避免点按钮无反应 -->
    <div v-if="updater.notice" class="update-notice" :class="updater.notice.kind">
      <span class="update-notice-text">{{ updater.notice.text }}</span>
      <button class="update-notice-close" aria-label="关闭提示" @click="updater.dismissNotice()">✕</button>
    </div>

    <!-- 新版本提示条：启动检测到新版本 / 下载中 / 已下载时出现在标题栏下方 -->
    <div v-if="updater.bannerVisible" class="update-banner">
      <div class="update-banner-body">
        <div class="update-banner-title">
          <template v-if="updater.state === 'available'">发现新版本 v{{ updater.info?.latest_version }}</template>
          <template v-else-if="updater.state === 'downloading'"
            >正在下载更新 v{{ updater.info?.latest_version }}…</template
          >
          <template v-else>更新已下载完成，安装后将自动重启</template>
        </div>

        <div v-if="updater.state === 'downloading'" class="update-banner-progress">
          <div class="update-progress-track">
            <div
              class="update-progress-fill"
              :class="{ indeterminate: updater.progressPercent === null }"
              :style="updater.progressPercent !== null ? { width: `${updater.progressPercent}%` } : undefined"
            ></div>
          </div>
          <span class="update-progress-text">
            <template v-if="updater.progress">
              {{ formatSize(updater.progress.downloaded)
              }}<template v-if="updater.progress.total">
                / {{ formatSize(updater.progress.total) }}（{{ updater.progressPercent }}%）</template
              >
            </template>
            <template v-else>准备中…</template>
          </span>
        </div>
        <div v-else-if="updater.info?.release_notes" class="update-banner-notes">{{ updater.info.release_notes }}</div>
        <div v-if="updater.error" class="update-banner-error">{{ updater.error }}</div>
      </div>

      <div class="update-banner-actions">
        <button
          v-if="updater.actionable"
          class="update-btn primary"
          :disabled="updater.installing"
          @click="updater.primaryAction()"
        >
          {{ updater.state === "downloaded" ? "立即安装并重启" : "立即更新" }}
        </button>
        <button v-else class="update-btn" disabled>下载中…</button>
        <button class="update-btn" @click="updater.dismiss()">稍后</button>
      </div>
    </div>

    <!-- 左侧导航菜单 + 功能页内容 -->
    <div class="body">
      <aside class="sidebar">
        <nav class="side-nav">
          <RouterLink to="/" class="nav-item" exact-active-class="active" title="主功能">
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
          </RouterLink>

          <RouterLink to="/launcher" class="nav-item" active-class="active" title="启动管理">
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
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
            </svg>
            <span>启动管理</span>
          </RouterLink>

          <RouterLink to="/ports" class="nav-item" active-class="active" title="端口监测">
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="3" width="18" height="12" rx="2" />
              <path d="M8 21h8M12 15v6M6 9h3l2-3 2 6 2-3h3" />
            </svg>
            <span>端口监测</span>
          </RouterLink>

          <RouterLink to="/settings" class="nav-item" active-class="active" title="设置">
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
          </RouterLink>

          <RouterLink to="/mcp" class="nav-item" active-class="active" title="MCP 配置">
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
              <path d="M12 2v6" />
              <path d="M12 16v6" />
              <path d="M2 12h6" />
              <path d="M16 12h6" />
              <circle cx="12" cy="12" r="3" />
              <circle cx="12" cy="5" r="1.5" />
              <circle cx="12" cy="19" r="1.5" />
              <circle cx="5" cy="12" r="1.5" />
              <circle cx="19" cy="12" r="1.5" />
            </svg>
            <span>MCP 配置</span>
          </RouterLink>
        </nav>

        <div class="sidebar-footer">v{{ app.version }}</div>
      </aside>

      <main ref="contentRef" class="content">
        <ErrorBoundary>
          <RouterView />
        </ErrorBoundary>
      </main>
    </div>

    <!-- 全局 Toast 出口（业务侧调用 lib/toast.ts 即可） -->
    <ToastHost />
  </div>
</template>
