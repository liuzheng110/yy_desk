# MCP 代理 —— Codex 接入指南

MyApp 内置一个 **MCP 网关**，让 Codex（或其他 MCP 客户端）通过 MyApp 使用你配置的外部 MCP 服务器。

**解决的问题**：Codex 先启动、外部 MCP 还没启动时，Codex 会话里找不到这些 MCP、没法用。

接入本网关后：**只要 MyApp 在运行，Codex 会话里就能发现 MyApp 上挂接的全部 MCP 工具**（工具清单来自 MyApp 的持久化缓存，不依赖外部 MCP 在线）；真正调用时才实时转发到外部 MCP——**外部 MCP 之后启动，调用立即成功，无需重启 Codex 或恢复会话**。

## 架构



```
Codex CLI（~/.codex/config.toml 里配置一个 MCP 服务器）
  │
  ├─ 方式 A（推荐，常驻）：url = "http://127.0.0.1:9876/mcp"   ──▶ MyApp 内置 HTTP 网关
  │
  └─ 方式 B（stdio）：command = "MyApp.exe" args = ["--mcp-proxy"] ──▶ 代理进程自拉起 MyApp
                                                                      │  并转发到本地 HTTP 网关
                                                                      ▼
                                              MyApp MCP 网关（127.0.0.1:9876/mcp）
                                                ├─ tools/list  ← 工具缓存（离线也可发现）
                                                └─ tools/call  → 实时连接并转发
                                                                    ├─▶ 外部 MCP（本地 stdio 程序）
                                                                    └─▶ 外部 MCP（远程 HTTP URL）
```



* **发现与执行分离**：`tools/list` 永远从缓存回答（外部 MCP 不要求在线）；`tools/call` 才连真实 MCP。

* 配置 `autoStart` 后，调用时会先自动打开对应桌面应用并等待服务就绪。未配置自动启动的后端离线时会报错（`isError: true`），但工具一直在列表里。

## 一、配置外部 MCP（在 MyApp 的数据目录）

配置文件：`data/mcp-servers.json`（与 `app.db` 同目录；便携版在 exe 旁 `data\`，安装版在 `%APPDATA%\com.myapp.app\`）。



```
{
  "port": 9876,                       // 网关端口，改后需同步改 Codex 配置
  "metaMode": false,                  // 可选：meta 模式（默认关，见第五节）
  "autoWarmStart": true,              // 可选：启动自动预取工具缓存（默认开）
  "failsafe": {                       // 可选：熔断参数（不填用默认）
    "failureThreshold": 5,            //   连续失败多少次后熔断该后端（默认 5）
    "resetTimeoutSec": 30,            //   熔断后多少秒自动恢复（默认 30）
    "warmStartRetries": 6             //   自动预取单个后端最多重试次数（默认 6）
  },
  "servers": [
    {
      "name": "filesystem",          // 工具名前缀：filesystem__read_file
      "enabled": true,
      "transport": "stdio",          // 本地程序式：MyApp 替你拉起子进程
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "./"],
      "cwd": null,                   // 可选：工作目录
      "env": { "NODE_ENV": "production" }   // 可选：环境变量
    },
    {
      "name": "remote-api",
      "enabled": true,
      "transport": "http",           // 远程 Streamable HTTP
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer xxx" }   // 可选
    },
    {
      "name": "example-disabled",    // 示例：禁用态，不参与扫描/列出
      "enabled": false,
      "transport": "stdio",
      "command": "echo",
      "args": []
    }
  ]
}
```

字段说明：



| 字段                                 | 说明                                                  |
| ---------------------------------- | --------------------------------------------------- |
| `name`                             | 唯一标识，同时作为工具名前缀（`{name}__{tool}`，可用 `toolPrefix` 覆盖） |
| `enabled`                          | 是否参与聚合（工具列出 + 调用路由）                                 |
| `transport`                        | `stdio`（本地命令子进程）或 `http`（Streamable HTTP URL）       |
| `command` / `args` / `cwd` / `env` | stdio 型必填 `command`，其余可选                            |
| `url` / `headers`                  | http 型必填 `url`，`headers` 可选                         |
| `toolPrefix`                       | 可选，覆盖工具名前缀                                          |
| `toolTimeoutMs`                    | 可选，单次调用超时（毫秒，默认 60000）                              |
| `failureThreshold`                 | 可选（per-server 覆盖熔断阈值，默认取 `failsafe.failureThreshold`）   |
| `resetTimeoutSec`                  | 可选（per-server 覆盖熔断恢复时间，默认取 `failsafe.resetTimeoutSec`） |

## 二、启动 MyApp（自动预取 + 熔断）



1. 启动 MyApp（网关随主程序自动监听 `127.0.0.1:9876`）。

2. **自动预取（默认开启）**：启动时后台逐个连接已启用的外部 MCP 并扫描工具清单写入缓存——你**不用再手动点刷新**。外部 MCP 当时没启动也没关系：会按 2s→4s→8s…（封顶 30s）退避重试最多 `warmStartRetries` 次；外部 MCP 之后启动，重试命中即成功写入缓存。

3. **手动刷新**：外部 MCP 在线时，在主界面下方 **MCP 管理面板** 点「刷新工具列表」可立即重扫；「重载配置」用于修改 `data/mcp-servers.json` 后热加载，无需重启。

4. **熔断保护**：某后端连续调用失败达到 `failureThreshold` 次后自动进入熔断（该后端的调用会快速返回"熔断中"而不是挂 60 秒超时），`resetTimeoutSec` 秒后自动恢复；管理面板每个服务器行有状态徽标（正常/故障/熔断/已停用），可以点「停用」（kill switch）或「恢复」（revive）手动控制。

5. 之后即使外部 MCP 离线，Codex 依然能发现这些工具。

> 说明：工具缓存是 "最后成功扫描" 的快照。外部 MCP 新增 / 删除了工具，需要在其在线时重新扫描刷新。

## 三、接入 Codex（\~/.codex/config.toml）

网关支持 **scope 分组**：在 URL 上加 `?scope=<服务器名>`，该入口只暴露对应 MCP 的工具。MCP 一多，推荐**每个 MCP 一个入口**，工具各归各组、Codex 里可独立开关，避免全部工具挤在一个口里占上下文。

**方式 A（推荐，MCP 较多时）：每个 MCP 一个 scope 入口**



```
[mcp_servers.dbx]
url = "http://127.0.0.1:9876/mcp?scope=dbx"

[mcp_servers.netcatty]
url = "http://127.0.0.1:9876/mcp?scope=netcatty-external"
```

* `scope` 的值 = `mcp-servers.json` 里服务器的 `name` 字段；
* 每个入口只看到该组的工具（如 `dbx__dbx_execute_query`、`netcatty-external__terminal_execute`）；
* 入口外调用该组以外的工具会被网关拒绝（报 `not part of scope`）；
* 后启动容错不变：工具仍来自缓存，外部 MCP 后启动无需重启 Codex。

**方式 B（MCP 少时）：单入口全量暴露**



```
[mcp_servers.myapp]
url = "http://127.0.0.1:9876/mcp"
```

**方式 C —— stdio 入口（代理自拉起 MyApp，任何时刻都能用）**



```
[mcp_servers.myapp]
command = "C:\\AI_workspace\\yy_desk\\myapp\\portable\\MyApp_0.1.0_x64\\MyApp.exe"
args = ["--mcp-proxy"]
```

（路径换成你的 MyApp.exe 实际位置；`--mcp-proxy` 代理进程若发现网关没在跑，会自动拉起 MyApp 主程序并等待端口就绪。）

也可以用 CLI 添加：



```
codex mcp add dbx --url "http://127.0.0.1:9876/mcp?scope=dbx"
```

## 四、使用与验证



1. 启动 Codex（保持 MyApp 在运行）。

2. 在 Codex 会话里输入 `/mcp`，应看到各入口服务器（如 `dbx`、`netcatty`）以及各自组内的工具（形如 `dbx__dbx_execute_query`、`netcatty-external__terminal_execute`）。

3. 直接让 Codex 调用这些工具：

* 对应外部 MCP 在线 → 正常执行；

* 对应外部 MCP 离线 → 报错提示 "连不上"；

* 启动外部 MCP 后再让 Codex 调用同一工具 → **立即成功，无需重启 Codex / 恢复会话**。

## 五、Meta 模式（可选，后端很多时建议开）

默认（`metaMode: false`）下 `tools/list` 把全部后端工具一次性暴露给 Codex——工具少时最直接、最快。

当挂了**很多**外部 MCP（几十上百个工具）时，工具清单会占满 Codex 上下文。开启 `metaMode: true` 后，Codex 会话里只出现 4 个固定的元工具（借鉴 mcp-gateway 的做法）：



```
gateway_search_tools  按关键词在所有后端缓存里搜索工具（返回名称/服务器/描述/相关度分）
gateway_list_tools    列出某后端（或全部）的工具
gateway_list_servers  列出后端状态（正常/故障/熔断/停用 + 缓存工具数）
gateway_invoke        指定 server + tool + arguments 直接调用后端工具
```

典型用法：Codex 先 `gateway_search_tools("search weather")` 找到匹配工具，再 `gateway_invoke({"server":"tavily","tool":"web_search",...})` 执行。工具发现走缓存（离线也可搜），调用实时转发，行为与全量模式一致。

## 六、对应应用未打开时自动启动

在需要桌面应用配合的服务器条目中加入 `autoStart`。网关在 **实际工具调用或手动刷新** 前检查应用发布的本地端口；端口不可用时启动应用，等待就绪后再执行请求。列出缓存工具、后台自动预取不会主动打开桌面应用。

```json
"autoStart": {
  "enabled": true,
  "command": "C:\\Path\\To\\App.exe",
  "args": [],
  "portFile": "C:\\Path\\To\\discovery.json",
  "portField": "port",
  "startupTimeoutMs": 20000
}
```

- `command` 是桌面应用可执行文件；原服务器的 `command` 仍是 MCP 启动器，两者分开配置。
- `portFile` 是应用写入的服务端口文件。DBX 使用数据目录中的 `mcp-bridge-port`（纯文本），省略 `portField`；Netcatty 使用 `external-mcp/discovery.json`，设置 `portField: "port"`。
- 只探测 `127.0.0.1`，每次重新读取端口，避免使用应用重启前的旧端口。
- 可选 `cwd` 指定工作目录；`startupTimeoutMs` 默认 20000，允许 1–120000 毫秒，需小于客户端工具调用总超时。
- 同一应用的并发启动请求会合并。应用已就绪时直接使用；启动失败或超时返回错误，不会自动重放已经发送的业务工具调用。
- Windows 启动过程不创建控制台黑窗。桌面应用与短期 MCP 子进程分别管理，工具调用结束后桌面应用继续运行。
- 将 `autoStart.enabled` 设为 `false` 可关闭此功能。修改配置后点击“重载配置”，已开启的条目会显示“按需启动”。
- Netcatty 应用内的 External MCP 仍需启用。自动启动不会修改应用权限或启用被关闭的服务。

## 常见问题



* **Codex 里看不到工具**：先确认 MyApp 已启动且 `http://127.0.0.1:9876/mcp` 能访问（浏览器 /`curl` POST `initialize`）；再确认 `mcp-servers.json` 里 `enabled: true`；最后在外部 MCP 在线时执行过一次工具扫描（缓存里有数据）。

* **端口被占用**：改 `mcp-servers.json` 的 `port`，并同步改 Codex 的 `url`。

* **改了配置不生效**：修改 `mcp-servers.json` 后重启 MyApp，或调用 `mcp_reload_config` 命令。

* **HTTP 型外部 MCP 带认证**：在 `headers` 里加 `Authorization: Bearer xxx`。

* **某后端一直失败导致调用慢**：连续失败自动熔断后调用会快速报"熔断中"；等 `resetTimeoutSec` 秒自动恢复，或在管理面板点「恢复」立即启用。

* **Codex 看不到工具**：先确认 MyApp 已启动且 `http://127.0.0.1:9876/mcp` 能访问（浏览器 /`curl` POST `initialize`）；再确认 `mcp-servers.json` 里 `enabled: true`；最后确认缓存非空（自动预取或手动刷新过，`data/mcp-tools-cache.json` 里有数据）。

## 后续规划

* MCP 管理面板增强：在界面内添加 / 编辑服务器（目前面板支持查看状态 + 一键刷新 + 重载配置 + 单后端停用/恢复，添加服务器仍编辑 `data/mcp-servers.json`）。
* 后端能力代理（`sampling/createMessage`、`roots/list` 转发回客户端）——参考 mcp-gateway，后续按需加。
