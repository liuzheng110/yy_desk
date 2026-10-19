<script setup lang="ts">
// 启动管理页：读取 data/launch-items.json 的启动项，
// 一键启动 / 呼出前台，绿点显示运行状态。
// 插件化：编辑 launch-items.json 加条目即可新增（exe / bat / ps1 / py），无需改代码。
import { onMounted, ref } from "vue";
import { backend } from "@/lib/backend";
import type { LaunchItem } from "@/lib/backend";
import { toErrorMessage } from "@/lib/errors";
import { toastError, toastSuccess } from "@/lib/toast";

const items = ref<LaunchItem[]>([]);
const statuses = ref<Map<string, boolean>>(new Map());
const busyId = ref<string | null>(null);
const loading = ref(false);
const error = ref("");

function isRunning(id: string): boolean {
  return statuses.value.get(id) ?? false;
}

async function refreshStatuses() {
  const list = await backend.listLaunchStatuses();
  const map = new Map<string, boolean>();
  for (const s of list) map.set(s.id, s.running);
  statuses.value = map;
}

async function load() {
  loading.value = true;
  error.value = "";
  try {
    items.value = await backend.listLaunchItems();
    await refreshStatuses();
  } catch (e) {
    error.value = toErrorMessage(e);
  } finally {
    loading.value = false;
  }
}

async function onItemClick(item: LaunchItem) {
  if (busyId.value) return;
  busyId.value = item.id;
  error.value = "";
  try {
    const action = await backend.launchItem(item.id);
    if (action === "started") {
      toastSuccess(`正在启动「${item.name}」…`);
      // 脚本/服务类项目首次启动可能较慢（依赖安装、服务初始化），
      // 持续轮询直到就绪（最长约 90 秒，与启动脚本超时窗口对齐）
      for (let i = 0; i < 45; i++) {
        await new Promise((r) => setTimeout(r, 2000));
        await refreshStatuses();
        if (isRunning(item.id)) {
          toastSuccess(`「${item.name}」已就绪`);
          break;
        }
      }
    } else {
      toastSuccess(item.url ? `已打开「${item.name}」界面` : `已将「${item.name}」呼出到前台`);
      await new Promise((r) => setTimeout(r, 300));
      await refreshStatuses();
    }
  } catch (e) {
    error.value = toErrorMessage(e);
    toastError(`操作失败：${toErrorMessage(e)}`);
  } finally {
    busyId.value = null;
  }
}

async function onStopClick(item: LaunchItem) {
  if (busyId.value) return;
  busyId.value = item.id;
  error.value = "";
  try {
    await backend.stopLaunchItem(item.id);
    toastSuccess(`已停止「${item.name}」`);
    // 停止脚本/服务可能异步收尾：轮询等待状态变为未运行（最长约 20 秒）
    for (let i = 0; i < 10; i++) {
      await new Promise((r) => setTimeout(r, 2000));
      await refreshStatuses();
      if (!isRunning(item.id)) break;
    }
  } catch (e) {
    error.value = toErrorMessage(e);
    toastError(`停止失败：${toErrorMessage(e)}`);
  } finally {
    busyId.value = null;
  }
}

onMounted(load);
</script>

<template>
  <section class="page">
    <div class="launcher-head">
      <h1>启动管理</h1>
      <button class="tb-btn small-btn" :disabled="loading" @click="load">
        {{ loading ? "刷新中…" : "刷新" }}
      </button>
    </div>
    <p class="muted">
      一键启动 / 呼出常用软件。编辑 <code>data\launch-items.json</code> 即可添加新的启动项（exe / bat / ps1 / py）。
    </p>

    <div v-if="error" class="launcher-error">{{ error }}</div>

    <div class="launcher-list">
      <div v-if="items.length === 0 && !loading" class="launcher-empty">
        暂无启动项，请编辑 <code>launch-items.json</code> 添加。
      </div>

      <div v-for="item in items" :key="item.id" class="launcher-item">
        <span
          class="status-dot"
          :class="{ on: isRunning(item.id) }"
          :title="isRunning(item.id) ? '运行中' : '未运行'"
        ></span>
        <div class="launcher-info">
          <div class="launcher-name">{{ item.name }}</div>
          <div class="launcher-cmd" :title="item.command">{{ item.command }}</div>
        </div>
        <div class="launcher-actions">
          <button
            class="tb-btn"
            :class="{ primary: !isRunning(item.id) }"
            :disabled="busyId === item.id || !item.enabled"
            @click="onItemClick(item)"
          >
            {{ busyId === item.id ? "处理中…" : isRunning(item.id) ? (item.url ? "打开页面" : "呼出前台") : "启动" }}
          </button>
          <button
            v-if="isRunning(item.id)"
            class="tb-btn danger"
            :disabled="busyId === item.id || !item.enabled"
            @click="onStopClick(item)"
          >
            {{ busyId === item.id ? "处理中…" : "停止" }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.launcher-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.launcher-head h1 {
  margin: 0;
}

.launcher-error {
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--err-bg);
  border: 1px solid var(--err-border);
  color: var(--err-text);
  font-size: 13px;
}

.launcher-list {
  margin-top: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.launcher-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-app);
}

.status-dot {
  flex: 0 0 auto;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--text-faint);
}

.status-dot.on {
  background: #22c55e;
  box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.18);
}

.launcher-actions {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
}

.launcher-info {
  flex: 1 1 auto;
  min-width: 0;
}

.launcher-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-strong);
}

.launcher-cmd {
  font-size: 11.5px;
  color: var(--text-faint);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.launcher-empty {
  padding: 24px;
  text-align: center;
  color: var(--text-muted);
  font-size: 13px;
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-lg);
}
</style>
