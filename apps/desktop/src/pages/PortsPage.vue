<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import BaseButton from "@/components/ui/BaseButton.vue";
import { backend, type PortProcess, type PortStopResult } from "@/lib/backend";
import { toErrorMessage } from "@/lib/errors";

const isDesktop = typeof window.__TAURI_INTERNALS__ !== "undefined";
const processes = ref<PortProcess[]>([]);
const query = ref("");
const scope = ref("all");
const protocol = ref("all");
const autoRefresh = ref(true);
const loading = ref(false);
const stopping = ref(false);
const error = ref("");
const updatedAt = ref("");
const selected = ref<string[]>([]);
const results = ref<PortStopResult[]>([]);
let disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;

const identity = (row: PortProcess) => `${row.pid}:${row.started_at}`;
const visible = computed(() => {
  const needle = query.value.trim().toLowerCase();
  return processes.value.filter((row) => {
    if (scope.value === "development" && !row.development) return false;
    const endpoints = row.endpoints.filter(
      (endpoint) => protocol.value === "all" || endpoint.protocol === protocol.value,
    );
    if (!endpoints.length) return false;
    if (!needle) return true;
    if (/^\d+$/.test(needle)) return row.pid === Number(needle) || endpoints.some((e) => e.port === Number(needle));
    return [
      row.name,
      row.executable,
      row.command_line,
      row.project_path ?? "",
      ...endpoints.map((e) => e.address),
    ].some((value) => value.toLowerCase().includes(needle));
  });
});
const selectedRows = computed(() => processes.value.filter((row) => selected.value.includes(identity(row))));
const endpointCount = computed(() => processes.value.reduce((sum, row) => sum + row.endpoints.length, 0));
const visibleStoppable = computed(() => visible.value.filter((row) => !row.protected_reason));
const allVisibleSelected = computed(
  () =>
    visibleStoppable.value.length > 0 && visibleStoppable.value.every((row) => selected.value.includes(identity(row))),
);

function selectVisible() {
  const keys = new Set(visibleStoppable.value.map(identity));
  selected.value = allVisibleSelected.value
    ? selected.value.filter((key) => !keys.has(key))
    : [...new Set([...selected.value, ...keys])];
}

async function refresh() {
  if (!isDesktop || loading.value || stopping.value || disposed) return;
  loading.value = true;
  error.value = "";
  try {
    const rows = await backend.scanLocalPorts();
    if (disposed) return;
    processes.value = rows;
    const valid = new Set(rows.filter((row) => !row.protected_reason).map(identity));
    selected.value = selected.value.filter((key) => valid.has(key));
    updatedAt.value = new Date().toLocaleTimeString();
  } catch (cause) {
    error.value = toErrorMessage(cause);
  } finally {
    loading.value = false;
  }
}

async function stop(rows: PortProcess[]) {
  if (loading.value || stopping.value || !rows.length) return;
  stopping.value = true;
  error.value = "";
  results.value = [];
  try {
    results.value = await backend.stopPortProcesses(rows.map(({ pid, started_at }) => ({ pid, started_at })));
    const stopped = new Set(results.value.filter((result) => result.stopped).map((result) => result.pid));
    selected.value = selected.value.filter((key) => !rows.some((row) => stopped.has(row.pid) && identity(row) === key));
  } catch (cause) {
    error.value = toErrorMessage(cause);
  } finally {
    stopping.value = false;
  }
  // Preserve a stop error if the subsequent scan succeeds.
  const stopError = error.value;
  await refresh();
  if (stopError) error.value = [stopError, error.value].filter(Boolean).join("；");
}

function protectedLabel(row: PortProcess) {
  if (row.protected_reason.includes("Windows")) return "系统进程 / 服务，禁止停止";
  if (row.protected_reason.includes("parent")) return "应用自身或父进程，禁止停止";
  return "进程信息不可用，请刷新或检查权限";
}

function scheduleRefresh() {
  timer = setTimeout(async () => {
    if (autoRefresh.value && document.visibilityState === "visible") await refresh();
    if (!disposed) scheduleRefresh();
  }, 10000);
}

onMounted(async () => {
  await refresh();
  if (!disposed && isDesktop) scheduleRefresh();
});
onBeforeUnmount(() => {
  disposed = true;
  clearTimeout(timer);
});
</script>

<template>
  <section class="page ports-page">
    <div class="ports-heading">
      <div>
        <h1>端口监测</h1>
        <p class="muted">自动发现本机监听端口，定位占用它的前端、后端和其他服务。</p>
      </div>
      <BaseButton :loading="loading" :disabled="!isDesktop || stopping" @click="refresh">刷新端口</BaseButton>
    </div>

    <div v-if="!isDesktop" class="notice">请在 Windows 桌面应用中使用端口监测，浏览器无法读取本机进程。</div>
    <div class="ports-summary">
      <span
        ><strong>{{ processes.length }}</strong> 个进程</span
      >
      <span
        ><strong>{{ endpointCount }}</strong> 个监听 / 绑定地址</span
      >
      <span class="muted">{{ updatedAt ? `最近扫描 ${updatedAt}` : "尚未扫描" }}</span>
      <label><input v-model="autoRefresh" type="checkbox" :disabled="!isDesktop" /> 每 10 秒自动刷新</label>
    </div>

    <div class="ports-filters">
      <input
        v-model="query"
        class="search"
        type="search"
        aria-label="搜索端口、PID、项目或进程"
        placeholder="搜索端口（如 8080）、PID、项目或进程…"
      />
      <select v-model="scope" aria-label="进程范围">
        <option value="all">全部进程</option>
        <option value="development">疑似开发服务</option>
      </select>
      <select v-model="protocol" aria-label="协议">
        <option value="all">TCP + UDP</option>
        <option value="TCP">TCP</option>
        <option value="UDP">UDP</option>
      </select>
    </div>
    <p class="muted guidance">无需指定端口。项目目录根据程序和命令路径推断；无法仅凭端口确定是否由 Agent 启动。</p>

    <div class="selection-bar">
      <label
        ><input
          type="checkbox"
          :checked="allVisibleSelected"
          :disabled="!visibleStoppable.length || loading || stopping"
          @change="selectVisible"
        />
        选择当前列表</label
      >
      <span>已选 {{ selectedRows.length }} 个进程（含筛选外）</span>
      <BaseButton
        variant="danger"
        :disabled="!selectedRows.length || selectedRows.length > 64 || loading || stopping"
        :loading="stopping"
        @click="stop(selectedRows)"
        >停止所选进程</BaseButton
      >
      <BaseButton v-if="selected.length" variant="ghost" :disabled="stopping" @click="selected = []"
        >清空选择</BaseButton
      >
    </div>
    <p class="muted guidance">
      停止会立即结束整个进程并释放它的全部端口。可勾选前后端一起停止，每次最多 64 个；自动重启的服务会再次出现。
    </p>

    <div v-if="error" class="notice error" role="alert">
      {{ error }}<span v-if="updatedAt">（下方保留上次扫描结果）</span>
    </div>
    <div v-if="results.length" class="stop-results" role="status">
      <div v-for="result in results" :key="result.pid" :class="result.stopped ? 'success' : 'failure'">
        PID {{ result.pid }} · {{ result.message }}
      </div>
    </div>
    <div v-if="!visible.length" class="empty">
      {{ loading ? "正在读取本机端口和进程…" : !updatedAt ? "扫描结果将在这里显示" : "没有符合条件的端口" }}
    </div>

    <article v-for="row in visible" :key="identity(row)" class="process-card">
      <div class="process-heading">
        <input
          v-model="selected"
          type="checkbox"
          :value="identity(row)"
          :aria-label="`选择 ${row.name}，PID ${row.pid}`"
          :disabled="!!row.protected_reason || loading || stopping"
        />
        <strong>{{ row.name }}</strong>
        <span class="muted">PID {{ row.pid }}</span>
        <span v-if="row.development" class="tag">疑似开发服务</span>
        <BaseButton
          class="stop-button"
          variant="danger"
          size="sm"
          :disabled="!!row.protected_reason || loading || stopping"
          :title="row.protected_reason ? protectedLabel(row) : '立即结束该进程并释放全部端口'"
          @click="stop([row])"
          >停止进程</BaseButton
        >
      </div>
      <div class="endpoints">
        <code v-for="endpoint in row.endpoints" :key="`${endpoint.protocol}:${endpoint.address}:${endpoint.port}`"
          >{{ endpoint.protocol }}
          {{ endpoint.address.includes(":") ? `[${endpoint.address}]` : endpoint.address }}:<b>{{
            endpoint.port
          }}</b></code
        >
      </div>
      <div class="path">
        <span class="muted">项目目录（推断）</span> {{ row.project_path || "未识别，可查看程序路径和启动命令" }}
      </div>
      <div class="path"><span class="muted">程序路径</span> {{ row.executable || "无权限读取或进程已退出" }}</div>
      <div v-if="row.protected_reason" class="muted protection">{{ protectedLabel(row) }}</div>
      <details>
        <summary>启动命令与进程信息</summary>
        <p class="muted">父进程 PID {{ row.parent_pid }} · 启动标识 {{ row.started_at || "不可用" }}</p>
        <pre>{{ row.command_line || "无权限读取或进程未提供命令行" }}</pre>
      </details>
    </article>
  </section>
</template>

<style scoped>
.ports-page {
  max-width: 1200px;
}
.ports-heading,
.ports-summary,
.ports-filters,
.selection-bar,
.process-heading {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.ports-heading {
  justify-content: space-between;
}
h1 {
  margin: 0;
}
.ports-heading p {
  margin: 8px 0 0;
}
.ports-summary {
  margin: 22px 0 16px;
  padding: 14px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}
.ports-summary strong {
  color: var(--text-strong);
  font-size: 20px;
}
.ports-summary label {
  margin-left: auto;
}
label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.search,
select {
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  padding: 8px 10px;
  font: inherit;
  color: var(--text);
  background: var(--bg-app);
}
.search {
  flex: 1;
  min-width: 220px;
}
.search:focus,
select:focus {
  outline: 2px solid var(--focus-ring);
}
.guidance {
  font-size: 12px;
  line-height: 1.7;
}
.selection-bar {
  margin-top: 20px;
  font-size: 13px;
}
.process-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  margin-top: 12px;
  background: var(--bg-app);
}
.process-heading strong {
  color: var(--text-strong);
}
.stop-button {
  margin-left: auto;
}
.tag {
  background: var(--primary-soft);
  color: var(--primary-soft-text);
  padding: 3px 7px;
  border-radius: var(--radius-sm);
  font-size: 11px;
}
.endpoints {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 14px 0;
}
.endpoints code {
  background: var(--bg-sunken);
  border: 1px solid var(--border-faint);
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  overflow-wrap: anywhere;
}
.endpoints b {
  color: var(--primary);
}
.path {
  overflow-wrap: anywhere;
  font-size: 12px;
  line-height: 1.8;
}
.path span {
  margin-right: 8px;
}
.protection {
  font-size: 12px;
  margin-top: 6px;
}
details {
  margin-top: 12px;
  font-size: 12px;
}
summary {
  cursor: pointer;
  color: var(--text-muted);
}
pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--bg-sunken);
  padding: 10px;
  border-radius: var(--radius-sm);
  user-select: text;
}
.notice,
.stop-results {
  padding: 12px;
  margin-top: 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
}
.notice {
  background: var(--info-bg);
  color: var(--info-text);
}
.error {
  color: var(--err-text);
  background: var(--err-bg);
  border-color: var(--err-border);
}
.success {
  color: var(--ok-text);
}
.failure {
  color: var(--err-text);
}
.empty {
  padding: 40px;
  text-align: center;
  color: var(--text-muted);
}
input[type="checkbox"] {
  accent-color: var(--primary);
}
</style>
