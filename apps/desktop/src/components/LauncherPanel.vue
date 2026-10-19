<script setup lang="ts">
// 启动管理功能页：读取 data/launch-items.json 的启动项，
// 一键启动 / 呼出前台，绿点显示运行状态。
import { onMounted, ref } from "vue";
import { backend } from "../lib/backend";
import type { LaunchItem } from "../lib/backend";

const items = ref<LaunchItem[]>([]);
const statuses = ref<Map<string, boolean>>(new Map());
const busyId = ref<string | null>(null);
const loading = ref(false);
const error = ref("");

function isRunning(id: string): boolean {
  return statuses.value.get(id) ?? false;
}

async function refreshStatuses() {
  try {
    const list = await backend.listLaunchStatuses();
    const map = new Map<string, boolean>();
    for (const s of list) map.set(s.id, s.running);
    statuses.value = map;
  } catch (e) {
    error.value = String(e);
  }
}

async function load() {
  loading.value = true;
  error.value = "";
  try {
    items.value = await backend.listLaunchItems();
    await refreshStatuses();
  } catch (e) {
    error.value = String(e);
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
    // 启动后稍等进程就绪再刷新状态
    await new Promise((r) => setTimeout(r, action === "started" ? 900 : 150));
    await refreshStatuses();
  } catch (e) {
    error.value = String(e);
  } finally {
    busyId.value = null;
  }
}

onMounted(load);
</script>

<template>
  <section class="launcher-page">
    <div class="launcher-head">
      <h1>启动管理</h1>
      <button class="tb-btn" :disabled="loading" @click="load">
        {{ loading ? "刷新中…" : "刷新" }}
      </button>
    </div>
    <p class="muted">
      一键启动 / 呼出常用软件。编辑
      <code>data\launch-items.json</code>
      即可添加新的启动项（exe / bat / ps1 / py）。
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
        <button class="tb-btn primary" :disabled="busyId === item.id || !item.enabled" @click="onItemClick(item)">
          {{ busyId === item.id ? "处理中…" : isRunning(item.id) ? "呼出前台" : "启动" }}
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.launcher-page {
  max-width: 720px;
}

.launcher-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.launcher-error {
  margin-top: 12px;
  padding: 10px 12px;
  border-radius: 8px;
  background: #fef2f2;
  color: #b91c1c;
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
  border: 1px solid #e5e7eb;
  border-radius: 12px;
  background: #fff;
}

.status-dot {
  flex: 0 0 auto;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #d1d5db;
}

.status-dot.on {
  background: #22c55e;
  box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.18);
}

.launcher-info {
  flex: 1 1 auto;
  min-width: 0;
}

.launcher-name {
  font-size: 14px;
  font-weight: 600;
  color: #111827;
}

.launcher-cmd {
  font-size: 11.5px;
  color: #9ca3af;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.launcher-empty {
  padding: 24px;
  text-align: center;
  color: #6b7280;
  font-size: 13px;
  border: 1px dashed #d1d5db;
  border-radius: 12px;
}
</style>
