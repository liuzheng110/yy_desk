<script setup lang="ts">
// MCP 服务器配置页：在界面里增删改外部 MCP 服务器，保存后写回
// 数据目录下的 mcp-servers.json 并让网关立即重新加载。
// 字段与 crates/app-mcp/src/config.rs 的 McpServerConfig / AppAutoStartConfig 对齐。
import { onMounted, ref } from "vue";
import { backend } from "@/lib/backend";

interface AutoStartForm {
  enabled: boolean;
  command: string;
  args: string;
  cwd: string;
  portFile: string;
  portField: string;
  startupTimeoutMs: string;
}

interface ServerForm {
  open: boolean;
  name: string;
  enabled: boolean;
  transport: "stdio" | "http";
  command: string;
  args: string;
  cwd: string;
  env: string;
  url: string;
  headers: string;
  toolPrefix: string;
  toolTimeoutMs: string;
  failureThreshold: string;
  resetTimeoutSec: string;
  autoStart: AutoStartForm | null;
}

interface FailsafeForm {
  failureThreshold: string;
  resetTimeoutSec: string;
  warmStartRetries: string;
}

const servers = ref<ServerForm[]>([]);
const configPath = ref("");
const port = ref("");
const metaMode = ref(false);
const autoWarmStart = ref(true);
const failsafe = ref<FailsafeForm>({ failureThreshold: "", resetTimeoutSec: "", warmStartRetries: "" });

const loading = ref(false);
const saving = ref(false);
const error = ref("");
const notice = ref("");

// ---- 表单 <-> JSON 转换 ----

function linesToArray(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function linesToMap(text: string): Record<string, string> | null {
  const out: Record<string, string> = {};
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const idx = trimmed.indexOf("=");
    if (idx <= 0) continue;
    const key = trimmed.slice(0, idx).trim();
    const value = trimmed.slice(idx + 1).trim();
    if (key) out[key] = value;
  }
  return Object.keys(out).length ? out : null;
}

function mapToLines(map: unknown): string {
  if (!map || typeof map !== "object") return "";
  return Object.entries(map as Record<string, unknown>)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
}

function numOrNull(text: string): number | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

function strOrEmpty(value: unknown): string {
  return value == null ? "" : String(value);
}

function toServerForm(raw: any): ServerForm {
  const as = raw?.autoStart;
  return {
    open: false,
    name: strOrEmpty(raw?.name),
    enabled: raw?.enabled !== false,
    transport: raw?.transport === "http" ? "http" : "stdio",
    command: strOrEmpty(raw?.command),
    args: Array.isArray(raw?.args) ? raw.args.join("\n") : "",
    cwd: strOrEmpty(raw?.cwd),
    env: mapToLines(raw?.env),
    url: strOrEmpty(raw?.url),
    headers: mapToLines(raw?.headers),
    toolPrefix: strOrEmpty(raw?.toolPrefix),
    toolTimeoutMs: strOrEmpty(raw?.toolTimeoutMs),
    failureThreshold: strOrEmpty(raw?.failureThreshold),
    resetTimeoutSec: strOrEmpty(raw?.resetTimeoutSec),
    autoStart: as
      ? {
          enabled: as.enabled !== false,
          command: strOrEmpty(as.command),
          args: Array.isArray(as.args) ? as.args.join("\n") : "",
          cwd: strOrEmpty(as.cwd),
          portFile: strOrEmpty(as.portFile),
          portField: strOrEmpty(as.portField),
          startupTimeoutMs: as.startupTimeoutMs != null ? String(as.startupTimeoutMs) : "20000",
        }
      : null,
  };
}

function fromServerForm(f: ServerForm): any {
  const out: any = { name: f.name.trim(), enabled: f.enabled, transport: f.transport };

  if (f.transport === "stdio") {
    if (f.command.trim()) out.command = f.command.trim();
    const args = linesToArray(f.args);
    if (args.length) out.args = args;
    if (f.cwd.trim()) out.cwd = f.cwd.trim();
    const env = linesToMap(f.env);
    if (env) out.env = env;
  } else {
    if (f.url.trim()) out.url = f.url.trim();
    const headers = linesToMap(f.headers);
    if (headers) out.headers = headers;
  }

  if (f.toolPrefix.trim()) out.toolPrefix = f.toolPrefix.trim();
  const toolTimeout = numOrNull(f.toolTimeoutMs);
  if (toolTimeout != null) out.toolTimeoutMs = toolTimeout;
  const failureThreshold = numOrNull(f.failureThreshold);
  if (failureThreshold != null) out.failureThreshold = failureThreshold;
  const resetTimeout = numOrNull(f.resetTimeoutSec);
  if (resetTimeout != null) out.resetTimeoutSec = resetTimeout;

  if (f.autoStart) {
    out.autoStart = {
      enabled: f.autoStart.enabled,
      command: f.autoStart.command.trim(),
      args: linesToArray(f.autoStart.args),
      cwd: f.autoStart.cwd.trim() || null,
      portFile: f.autoStart.portFile.trim(),
      portField: f.autoStart.portField.trim() || null,
      startupTimeoutMs: numOrNull(f.autoStart.startupTimeoutMs) ?? 20000,
    };
  }
  return out;
}

function buildConfig(): any {
  const out: any = { servers: servers.value.map(fromServerForm) };
  const p = numOrNull(port.value);
  if (p != null) out.port = p;
  out.metaMode = metaMode.value;
  out.autoWarmStart = autoWarmStart.value;
  out.failsafe = {
    failureThreshold: numOrNull(failsafe.value.failureThreshold) ?? 5,
    resetTimeoutSec: numOrNull(failsafe.value.resetTimeoutSec) ?? 30,
    warmStartRetries: numOrNull(failsafe.value.warmStartRetries) ?? 6,
  };
  return out;
}

// ---- 加载 / 保存 ----

async function load() {
  loading.value = true;
  error.value = "";
  try {
    const data = await backend.mcpGetConfig();
    configPath.value = data.path ?? "";
    const cfg: any = data.config ?? {};
    port.value = cfg.port != null ? String(cfg.port) : "";
    metaMode.value = cfg.metaMode === true;
    autoWarmStart.value = cfg.autoWarmStart !== false;
    const fs = cfg.failsafe ?? {};
    failsafe.value = {
      failureThreshold: strOrEmpty(fs.failureThreshold),
      resetTimeoutSec: strOrEmpty(fs.resetTimeoutSec),
      warmStartRetries: strOrEmpty(fs.warmStartRetries),
    };
    servers.value = Array.isArray(cfg.servers) ? cfg.servers.map(toServerForm) : [];
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

function validate(): string | null {
  const seen = new Set<string>();
  for (const s of servers.value) {
    const name = s.name.trim();
    if (!name) return "服务器名称不能为空";
    if (seen.has(name)) return `服务器名称重复：${name}`;
    seen.add(name);
    if (s.transport === "stdio" && !s.command.trim()) return `服务器「${name}」缺少 command`;
    if (s.transport === "http" && !s.url.trim()) return `服务器「${name}」缺少 url`;
    if (s.autoStart) {
      if (!s.autoStart.command.trim()) return `服务器「${name}」的自动启动缺少 command`;
      if (!s.autoStart.portFile.trim()) return `服务器「${name}」的自动启动缺少 portFile`;
    }
  }
  return null;
}

async function save() {
  error.value = "";
  notice.value = "";
  const problem = validate();
  if (problem) {
    error.value = problem;
    return;
  }
  saving.value = true;
  try {
    await backend.mcpSaveConfig(buildConfig());
    notice.value = "已保存并重新加载 MCP 配置。";
    window.setTimeout(() => {
      notice.value = "";
    }, 3500);
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

// ---- 服务器增删改 ----

function addServer() {
  servers.value.push({
    open: true,
    name: "",
    enabled: true,
    transport: "stdio",
    command: "",
    args: "",
    cwd: "",
    env: "",
    url: "",
    headers: "",
    toolPrefix: "",
    toolTimeoutMs: "",
    failureThreshold: "",
    resetTimeoutSec: "",
    autoStart: null,
  });
}

function removeServer(index: number) {
  servers.value.splice(index, 1);
}

function onTransportChange(f: ServerForm) {
  if (f.transport === "stdio") {
    f.url = "";
    f.headers = "";
  } else {
    f.command = "";
    f.args = "";
    f.cwd = "";
    f.env = "";
  }
}

function toggleAutoStart(f: ServerForm) {
  if (f.autoStart) {
    f.autoStart = null;
  } else {
    f.autoStart = {
      enabled: true,
      command: "",
      args: "",
      cwd: "",
      portFile: "",
      portField: "",
      startupTimeoutMs: "20000",
    };
  }
}

onMounted(load);
</script>

<template>
  <section class="page mcp-config">
    <h1>MCP 服务器配置</h1>
    <p class="muted">
      配置 Codex 通过本网关接入的外部 MCP 服务器。保存后写入
      <code>mcp-servers.json</code> 并立即重载网关，无需手工改文件。
    </p>
    <p v-if="configPath" class="muted path-line">
      配置文件：<code>{{ configPath }}</code>
    </p>

    <p v-if="error" class="cfg-error">{{ error }}</p>
    <p v-if="notice" class="cfg-notice">{{ notice }}</p>

    <!-- 网关设置 -->
    <div class="card cfg-card">
      <h3>网关设置</h3>
      <div class="grid">
        <label class="field">
          <span>监听端口</span>
          <input v-model="port" type="number" placeholder="9876" />
        </label>
        <label class="field">
          <span>熔断阈值（连续失败次数）</span>
          <input v-model="failsafe.failureThreshold" type="number" placeholder="5" />
        </label>
        <label class="field">
          <span>熔断重置（秒）</span>
          <input v-model="failsafe.resetTimeoutSec" type="number" placeholder="30" />
        </label>
        <label class="field">
          <span>预热重试次数</span>
          <input v-model="failsafe.warmStartRetries" type="number" placeholder="6" />
        </label>
      </div>
      <div class="toggles">
        <label class="check">
          <input v-model="metaMode" type="checkbox" />
          <span>meta 模式（只暴露 gateway_search / gateway_invoke 等元工具，避免工具过多撑爆上下文）</span>
        </label>
        <label class="check">
          <input v-model="autoWarmStart" type="checkbox" />
          <span>启动自动预热（开机连接各后端并预取工具缓存）</span>
        </label>
      </div>
    </div>

    <!-- 服务器列表 -->
    <div class="servers-head">
      <h3>外部 MCP 服务器（{{ servers.length }}）</h3>
      <div class="head-actions">
        <button class="tb-btn small-btn" :disabled="loading" @click="addServer">新增服务器</button>
        <button class="tb-btn primary small-btn" :disabled="loading" @click="load">重新读取</button>
      </div>
    </div>

    <p v-if="loading" class="muted">加载中…</p>
    <p v-else-if="servers.length === 0" class="empty">尚无服务器，点「新增服务器」开始配置。</p>

    <div v-for="(s, i) in servers" :key="i" class="server-card" :class="{ disabled: !s.enabled }">
      <div class="server-head" @click="s.open = !s.open">
        <span class="chevron" :class="{ open: s.open }">▸</span>
        <span class="server-name">{{ s.name.trim() || "（未命名服务器）" }}</span>
        <span class="badge">{{ s.transport }}</span>
        <span v-if="s.autoStart" class="badge ok">按需启动</span>
        <span v-if="!s.enabled" class="badge off">已禁用</span>
        <span class="spacer"></span>
        <button class="tb-btn small-btn danger" @click.stop="removeServer(i)">删除</button>
      </div>

      <div v-if="s.open" class="server-body">
        <div class="grid">
          <label class="field">
            <span>名称（唯一）</span>
            <input v-model="s.name" placeholder="例如 dbx" />
          </label>
          <label class="field">
            <span>传输方式</span>
            <select v-model="s.transport" @change="onTransportChange(s)">
              <option value="stdio">stdio（本地进程）</option>
              <option value="http">http（远程地址）</option>
            </select>
          </label>
          <label class="field">
            <span>工具前缀（可空，默认同名称）</span>
            <input v-model="s.toolPrefix" placeholder="dbx" />
          </label>
          <label class="field">
            <span>调用超时 ms（可空，默认 60000）</span>
            <input v-model="s.toolTimeoutMs" type="number" placeholder="60000" />
          </label>
          <label class="field">
            <span>熔断阈值（可空，用全局值）</span>
            <input v-model="s.failureThreshold" type="number" placeholder="5" />
          </label>
          <label class="field">
            <span>熔断重置 秒（可空，用全局值）</span>
            <input v-model="s.resetTimeoutSec" type="number" placeholder="30" />
          </label>
        </div>

        <label class="check">
          <input v-model="s.enabled" type="checkbox" />
          <span>启用此服务器</span>
        </label>

        <!-- stdio 字段 -->
        <template v-if="s.transport === 'stdio'">
          <label class="field">
            <span>启动命令 command</span>
            <input v-model="s.command" placeholder="C:\...\dbx-mcp.exe" />
          </label>
          <label class="field">
            <span>参数 args（每行一个）</span>
            <textarea v-model="s.args" rows="3" placeholder="--verbose&#10;--port&#10;9000"></textarea>
          </label>
          <div class="grid">
            <label class="field">
              <span>工作目录 cwd（可空）</span>
              <input v-model="s.cwd" placeholder="C:\Path\To" />
            </label>
          </div>
          <label class="field">
            <span>环境变量 env（每行 <code>KEY=VALUE</code>，可空）</span>
            <textarea
              v-model="s.env"
              rows="3"
              placeholder="NETCATTY_EXTERNAL_MCP_DISCOVERY_FILE=C:\...\discovery.json"
            ></textarea>
          </label>
        </template>

        <!-- http 字段 -->
        <template v-else>
          <label class="field">
            <span>服务地址 url</span>
            <input v-model="s.url" placeholder="http://127.0.0.1:9000/mcp" />
          </label>
          <label class="field">
            <span>请求头 headers（每行 <code>KEY=VALUE</code>，可空）</span>
            <textarea v-model="s.headers" rows="2" placeholder="Authorization=Bearer xxx"></textarea>
          </label>
        </template>

        <!-- 自动启动 -->
        <div class="autostart">
          <label class="check">
            <input type="checkbox" :checked="!!s.autoStart" @change="toggleAutoStart(s)" />
            <span>按需自动启动桌面应用（调用或刷新时若未运行则拉起并等待就绪）</span>
          </label>

          <div v-if="s.autoStart" class="autostart-body">
            <label class="field">
              <span>应用可执行文件 command</span>
              <input v-model="s.autoStart.command" placeholder="C:\...\DBX\dbx.exe" />
            </label>
            <label class="field">
              <span>参数 args（每行一个，可空）</span>
              <textarea v-model="s.autoStart.args" rows="2"></textarea>
            </label>
            <div class="grid">
              <label class="field">
                <span>工作目录 cwd（可空）</span>
                <input v-model="s.autoStart.cwd" placeholder="C:\Path\To" />
              </label>
              <label class="field">
                <span>启动超时 ms</span>
                <input v-model="s.autoStart.startupTimeoutMs" type="number" placeholder="20000" />
              </label>
            </div>
            <div class="grid">
              <label class="field">
                <span>端口文件 portFile</span>
                <input v-model="s.autoStart.portFile" placeholder="C:\...\mcp-bridge-port 或 C:\...\discovery.json" />
              </label>
              <label class="field">
                <span>端口字段 portField（纯文本端口文件留空；Netcatty 填 port）</span>
                <input v-model="s.autoStart.portField" placeholder="port" />
              </label>
            </div>
            <label class="check">
              <input v-model="s.autoStart.enabled" type="checkbox" />
              <span>启用自动启动</span>
            </label>
          </div>
        </div>
      </div>
    </div>

    <div class="footer-actions">
      <button class="tb-btn primary" :disabled="saving || loading" @click="save">
        {{ saving ? "保存中…" : "保存并重载" }}
      </button>
      <span class="muted">保存后网关立即生效；回到「主功能」可刷新工具列表查看状态。</span>
    </div>
  </section>
</template>

<style scoped>
.mcp-config {
  padding-bottom: 24px;
}
.path-line {
  margin-top: -4px;
  font-size: 12.5px;
}
.cfg-error {
  color: var(--err-text);
  background: var(--err-bg);
  border: 1px solid var(--err-border);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 13px;
  margin: 12px 0 0;
}
.cfg-notice {
  color: var(--ok-text);
  background: var(--ok-bg);
  border: 1px solid var(--ok-border);
  border-radius: 8px;
  padding: 8px 12px;
  font-size: 13px;
  margin: 12px 0 0;
  user-select: text;
}
.cfg-card {
  background: var(--bg-app);
  margin-top: 16px;
}
.cfg-card h3,
.servers-head h3 {
  margin: 0 0 12px;
  font-size: 15px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 12px 16px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 12.5px;
  color: var(--text);
  margin-bottom: 10px;
}
.field > span {
  font-weight: 500;
}
.field input,
.field select,
.field textarea {
  font: inherit;
  font-size: 13px;
  padding: 6px 9px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: var(--bg-app);
  color: var(--text-strong);
  width: 100%;
}
.field textarea {
  resize: vertical;
  font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  font-size: 12.5px;
  line-height: 1.55;
}
.field input:focus,
.field select:focus,
.field textarea:focus {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 2px var(--focus-ring);
}
.field code {
  font-size: 11.5px;
  padding: 0 4px;
}
.toggles {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 4px;
}
.check {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: 12.5px;
  color: var(--text);
  cursor: pointer;
  line-height: 1.5;
}
.check input[type="checkbox"] {
  width: 15px;
  height: 15px;
  margin: 1px 0 0;
  accent-color: var(--primary);
  cursor: pointer;
  flex: 0 0 auto;
}
.servers-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin: 24px 0 10px;
  flex-wrap: wrap;
}
.servers-head h3 {
  margin: 0;
}
.head-actions {
  display: flex;
  gap: 8px;
}
.empty {
  border: 1px dashed var(--border-strong);
  border-radius: 10px;
  background: var(--bg-sunken);
  padding: 16px;
  font-size: 13px;
  color: var(--text-muted);
}
.server-card {
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-app);
  margin-bottom: 10px;
  overflow: hidden;
}
.server-card.disabled {
  opacity: 0.7;
}
.server-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  background: var(--bg-panel);
  cursor: pointer;
  user-select: none;
}
.server-head:hover {
  background: var(--bg-hover);
}
.chevron {
  color: var(--text-muted);
  font-size: 11px;
  transition: transform 120ms ease;
  display: inline-block;
}
.chevron.open {
  transform: rotate(90deg);
}
.server-name {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-strong);
}
.spacer {
  flex: 1 1 auto;
}
.badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 11.5px;
  font-weight: 600;
  background: var(--bg-hover);
  color: var(--text-muted);
}
.badge.ok {
  background: var(--ok-soft-bg);
  color: var(--ok-text);
}
.badge.off {
  background: var(--bg-hover);
  color: var(--text-muted);
}
.tb-btn.small-btn.danger {
  color: var(--err-text);
  border-color: var(--err-border);
}
.tb-btn.small-btn.danger:hover:not(:disabled) {
  background: var(--err-bg);
  border-color: var(--err-text);
  color: var(--err-text);
}
.server-body {
  padding: 14px 16px 6px;
  border-top: 1px solid var(--border);
}
.autostart {
  margin-top: 8px;
  border-top: 1px dashed var(--border);
  padding-top: 12px;
}
.autostart-body {
  margin-top: 10px;
  padding: 12px 14px;
  background: var(--bg-panel);
  border: 1px solid var(--border-faint);
  border-radius: 8px;
}
.footer-actions {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
  flex-wrap: wrap;
}
</style>
