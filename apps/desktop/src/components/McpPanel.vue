<script setup lang="ts">
// MCP 代理（Codex 接入）管理面板。
// 展示网关状态、外部 MCP 服务器列表与生命周期状态（active/failed/circuitOpen/killed）、
// 工具缓存，支持刷新工具列表、重载配置、启停单个后端。
import { onMounted, ref } from "vue";
import { backend } from "../lib/backend";

interface McpServer {
  name: string;
  enabled: boolean;
  transport: string;
  toolPrefix: string;
  command: string | null;
  url: string | null;
  cachedTools: number;
  status: string;
  consecutiveFailures: number;
  lastError: string | null;
  autoStart: boolean;
}

interface McpStatus {
  port: number;
  servers: McpServer[];
  refreshedAt: string;
  metaMode: boolean;
  autoWarmStart: boolean;
}

interface RefreshReport {
  server: string;
  ok: boolean;
  toolCount: number;
  error: string | null;
}

const emit = defineEmits<{ (e: "configure"): void }>();

const status = ref<McpStatus | null>(null);
const error = ref("");
const busy = ref(false);
const lastReport = ref<RefreshReport[] | null>(null);

const statusLabel: Record<string, string> = {
  unknown: "未探测",
  active: "正常",
  failed: "故障",
  circuitOpen: "熔断",
  killed: "已停用",
};

const statusClass: Record<string, string> = {
  unknown: "off",
  active: "ok",
  failed: "err",
  circuitOpen: "warn",
  killed: "off",
};

function fmtTs(ts: string | null | undefined): string {
  if (!ts) return "未刷新";
  // unix seconds (gateway stores seconds since epoch)
  const n = Number(ts);
  if (!Number.isFinite(n) || n <= 0) return ts;
  const d = new Date(n * 1000);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

async function load() {
  try {
    status.value = await backend.mcpStatus();
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}

async function refreshTools() {
  busy.value = true;
  lastReport.value = null;
  try {
    const r = await backend.mcpRefreshTools();
    lastReport.value = r.refreshed ?? [];
    await load();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function reloadConfig() {
  busy.value = true;
  try {
    await backend.mcpReloadConfig();
    await load();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function killServer(name: string) {
  busy.value = true;
  try {
    await backend.mcpKillServer(name);
    await load();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function reviveServer(name: string) {
  busy.value = true;
  try {
    await backend.mcpReviveServer(name);
    await load();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section class="card mcp-card">
    <div class="mcp-head">
      <div>
        <h3>MCP 代理（Codex 接入）</h3>
        <p class="muted" style="margin: 2px 0 0">
          Codex 通过本网关发现/调用外部 MCP：工具列表来自缓存（外部 MCP 未启动也能发现），调用时实时转发
        </p>
      </div>
      <div class="mcp-actions">
        <button class="tb-btn" @click="emit('configure')">配置服务器</button>
        <button class="tb-btn" :disabled="busy" @click="reloadConfig">重载配置</button>
        <button class="tb-btn primary" :disabled="busy" @click="refreshTools">
          {{ busy ? "处理中…" : "刷新工具列表" }}
        </button>
      </div>
    </div>

    <p v-if="error" class="mcp-error">{{ error }}</p>

    <template v-if="status">
      <div class="mcp-statusline">
        <span class="badge ok">网关在线</span>
        <span class="muted"
          >端口 <code>{{ status.port }}</code></span
        >
        <span class="muted"
          >缓存刷新 <code>{{ fmtTs(status.refreshedAt) }}</code></span
        >
        <span class="muted"
          >服务器 <code>{{ status.servers.length }}</code> 个 · 工具缓存
          <code>{{ status.servers.reduce((n, s) => n + s.cachedTools, 0) }}</code> 个</span
        >
        <span
          class="muted"
          title="meta 模式：只暴露 gateway_search/gateway_invoke 等少量元工具，防止大量工具撑爆上下文"
        >
          模式 <code>{{ status.metaMode ? "meta（按需搜索调用）" : "全量暴露" }}</code>
        </span>
        <span v-if="status.autoWarmStart" class="muted" title="启动时自动连接外部 MCP 预取工具缓存，失败自动重试">
          <span class="badge ok">自动预取已开</span>
        </span>
      </div>

      <div v-if="status.servers.length === 0" class="mcp-empty">
        <p>尚未配置任何外部 MCP 服务器。</p>
        <p class="muted">
          点上方「配置服务器」在界面里新增（支持 stdio / http、环境变量、按需自动启动等），
          保存后网关立即加载；自动预取会在启动时扫描工具（也可手动点「刷新工具列表」）。
        </p>
      </div>

      <div v-else class="mcp-servers">
        <table>
          <thead>
            <tr>
              <th>名称</th>
              <th>传输</th>
              <th>前缀</th>
              <th>目标</th>
              <th>缓存工具</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="s in status.servers" :key="s.name">
              <td>
                <code>{{ s.name }}</code>
                <span v-if="s.autoStart" class="badge ok" title="调用或手动刷新时，应用未运行则自动启动并等待就绪"
                  >按需启动</span
                >
              </td>
              <td>{{ s.transport }}</td>
              <td>
                <code>{{ s.toolPrefix }}</code>
              </td>
              <td class="mcp-target">
                <code v-if="s.command">{{ s.command }}</code>
                <code v-else-if="s.url">{{ s.url }}</code>
                <span v-else class="muted">-</span>
              </td>
              <td>{{ s.cachedTools }}</td>
              <td>
                <span :class="['badge', statusClass[s.status] ?? 'off']" :title="s.lastError || ''">
                  {{ statusLabel[s.status] ?? s.status }}
                </span>
                <span v-if="s.consecutiveFailures > 0" class="muted" style="margin-left: 6px">
                  连续失败 {{ s.consecutiveFailures }}
                </span>
              </td>
              <td>
                <button
                  v-if="s.status !== 'killed'"
                  class="tb-btn small-btn"
                  :disabled="busy"
                  @click="killServer(s.name)"
                >
                  停用
                </button>
                <button v-else class="tb-btn primary small-btn" :disabled="busy" @click="reviveServer(s.name)">
                  恢复
                </button>
              </td>
            </tr>
          </tbody>
        </table>
        <p
          v-if="status.servers.some((s) => s.status === 'failed' || s.status === 'circuitOpen')"
          class="muted"
          style="font-size: 12px"
        >
          提示：故障/熔断的后端会保留工具缓存（Codex 仍可发现），外部 MCP
          恢复后调用自动成功；熔断会在重置时间后自动恢复，也可点「恢复」立即启用。
        </p>
      </div>

      <div v-if="lastReport && lastReport.length" class="mcp-report">
        <h4>刷新结果</h4>
        <ul>
          <li v-for="r in lastReport" :key="r.server">
            <span :class="['badge', r.ok ? 'ok' : 'err']">{{ r.ok ? "成功" : "失败" }}</span>
            <code>{{ r.server }}</code>
            <template v-if="r.ok">：缓存 {{ r.toolCount }} 个工具</template>
            <span v-else class="muted">：{{ r.error }}</span>
          </li>
        </ul>
      </div>

      <div class="mcp-hint">
        <h4>Codex 接入方式（写入 <code>~/.codex/config.toml</code>）</h4>
        <pre>
[mcp_servers.myapp]
url = "http://127.0.0.1:{{ status.port }}/mcp"</pre>
        <p class="muted">
          或用 stdio 入口：<code>command = "…MyApp.exe"</code>、<code>args = ["--mcp-proxy"]</code
          >（代理会自动拉起本程序）
        </p>
        <p v-if="status.metaMode" class="muted" style="margin-top: 4px">
          meta 模式下客户端先调用 <code>gateway_search_tools(query)</code> 搜索工具，再用
          <code>gateway_invoke(server, tool, arguments)</code> 调用；详见《MCP代理-Codex接入指南.md》。
        </p>
      </div>
    </template>
  </section>
</template>

<style scoped>
.mcp-card {
  margin-top: 16px;
}
.mcp-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.mcp-head h3 {
  margin: 0 0 4px;
}
.mcp-actions {
  display: flex;
  gap: 8px;
}
.mcp-error {
  color: var(--err-text);
  background: var(--err-bg);
  border: 1px solid var(--err-border);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 13px;
  margin: 10px 0 0;
}
.mcp-statusline {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
  margin: 12px 0;
  font-size: 13px;
}
.badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 600;
}
.badge.ok {
  background: var(--ok-soft-bg);
  color: var(--ok-soft-text);
}
.badge.off {
  background: var(--bg-hover);
  color: var(--text-muted);
}
.badge.err {
  background: var(--err-bg);
  color: var(--err-text);
}
.badge.warn {
  background: var(--warn-bg);
  color: var(--warn-text);
}
.mcp-empty {
  background: var(--bg-sunken);
  border: 1px dashed var(--border-strong);
  border-radius: 10px;
  padding: 14px 16px;
  font-size: 13px;
  margin: 8px 0;
}
.mcp-empty p {
  margin: 4px 0;
}
.mcp-servers table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
  margin: 8px 0;
}
.mcp-servers th,
.mcp-servers td {
  text-align: left;
  padding: 7px 10px;
  border-bottom: 1px solid var(--border);
  vertical-align: middle;
}
.mcp-servers th {
  color: var(--text-muted);
  font-weight: 600;
  font-size: 12px;
}
.mcp-target {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.small-btn {
  padding: 3px 10px;
  font-size: 12px;
}
.mcp-report {
  background: var(--bg-sunken);
  border-radius: 10px;
  padding: 10px 14px;
  font-size: 13px;
  margin: 8px 0;
}
.mcp-report h4,
.mcp-hint h4 {
  margin: 0 0 6px;
  font-size: 13px;
}
.mcp-report ul {
  margin: 0;
  padding-left: 18px;
}
.mcp-report li {
  margin: 3px 0;
}
.mcp-hint {
  margin-top: 12px;
  border-top: 1px solid var(--border);
  padding-top: 10px;
}
.mcp-hint pre {
  background: var(--bg-hover);
  border-radius: 8px;
  padding: 10px 12px;
  font-size: 12px;
  margin: 6px 0;
  overflow-x: auto;
}
</style>
