# MyApp —— Tauri 2 桌面应用框架

一个可直接二次开发的桌面应用脚手架，架构参考 [dbx](https://github.com/t8y2/dbx)（Apache-2.0）的桌面壳，
**保留其自动更新 / 安装打包 / 启动 / 运行四大机制，裁剪其全部数据库业务功能**。

技术栈：Tauri 2.10 + Vue 3 + TypeScript + Vite + pnpm monorepo + Rust workspace。

## 目录结构

```
myapp/
├── crates/
│   ├── app-core/          # 核心库：设置存储(SQLite)、更新检查/变更日志、系统字体、worker runtime
│   ├── app-platform/      # 平台能力：下载(单源/竞速)、代理、路径、进程封装
│   └── app-mcp/           # MCP 网关：聚合外部 MCP 服务器（stdio/HTTP）供 Codex 发现与调用
├── src-tauri/             # Tauri 桌面壳
│   ├── src/
│   │   ├── lib.rs         # run() 启动流程 + invoke 注册表
│   │   ├── data_dir.rs    # 数据目录：默认 / 环境变量(MYAPP_DATA_DIR)
│   │   ├── startup_recovery.rs    # 启动看门狗（60s 探测 + 恢复）
│   │   ├── webview2_recovery.rs   # WebView2 渲染进程崩溃恢复
│   │   ├── migration_gate.rs      # 迁移门槛（白名单命令 gate）
│   │   ├── window_state_guard.rs  # 主窗口边界守护
│   │   └── commands/      # IPC 命令
│   │       ├── update*.rs         # 自动更新（GitHub Releases + minisign 验签 + 已下载包缓存）
│   │       ├── app_settings.rs    # 桌面/外观/语言/关闭行为/标签页设置
│   │       ├── deep_link.rs       # myapp:// 深链接
│   │       ├── window_controls.rs # 窗口控制
│   │       ├── support_info.rs    # 支持信息（诊断用）
│   │       └── ...（business/ 放你的业务命令）
│   ├── tauri.conf.json    # 应用配置（名称/标识符/更新端点/打包）
│   └── capabilities/      # 权限声明
├── apps/desktop/          # 前端（Vue 3）
│   └── src/
│       ├── main.ts        # 入口：启动握手（mark_frontend_ready）+ Pinia/Router/全局错误处理
│       ├── StartupGate.vue # 启动门：关闭确认 / 二次启动文件 / 深链接
│       ├── App.vue        # 应用外壳：标题栏 + 侧边导航 + <RouterView>（业务页面别写这里）
│       ├── router/        # 路由表（hash 模式）：新增页面在此加 route
│       ├── pages/         # 路由页面：HomePage / SettingsPage / McpConfigPage
│       ├── stores/        # Pinia：app(版本/平台) / settings(开机启动/主题) / updater(更新状态机)
│       ├── components/    # 组件：McpPanel、ToastHost、ErrorBoundary、ui/(基础组件)
│       ├── lib/           # backend.ts(类型化 IPC) / theme.ts / toast.ts / errors.ts / startup.ts
│       └── styles.css     # 设计令牌（CSS 变量）+ 全局样式
├── scripts/               # dev / check / release / sync-version 脚本
└── .github/workflows/release.yml  # tag v* → Windows 构建并正式发布 Release
```


## 底座复用与发布校验

应用公开身份以 `src-tauri/tauri.conf.json` 为准：`productName`、`identifier`、深链接 `schemes`、updater 的公钥与 endpoint。Rust 编译时生成共享常量，前端只注入公开显示名；更新检查、下载、发布日志、自启动和启动恢复读取同一份身份。当前更新后端支持一个 GitHub `latest.json` 地址。

派生新应用时，使用独立 identifier、名称、协议名、GitHub 更新仓库和签名密钥。不要给已有应用随意更换 identifier 或公钥：前者改变数据目录，后者需要设计密钥轮换过渡。版本仍由 `pnpm sync-version` 同步。发布流水线拒绝标签/版本不一致或更新地址仍指向其他仓库的构建。

`pnpm validate` 检查版本、应用身份、候选公开文件、lint、格式、前端回归测试、生产前端构建以及 Rust workspace 单元测试。`pnpm check` 保留为快速编译检查。普通主分支提交和 PR 会跑检查；推送版本标签后先跑相同检查，再构建签名草稿，下载已上传资产并用内置公钥验签，全部成功才自动发布为 latest。失败草稿不会被客户端更新地址选中。

`pnpm check:public` 检查 Git 跟踪及未忽略文件中的常见凭据和私人配置文件名，只报告位置，不打印密钥内容。这是误提交防线，不是完整秘密扫描器；提交前仍应检查差异。私钥放 `.signing/`，CI 私钥只放仓库 Secret。数据目录中的 `mcp-servers.json`、`launch-items.json`、数据库及其备份不应提交或打包。全新安装的启动列表为空；格式参考 `launch-items.example.json`，已有本机配置不受影响。

底座仍使用轻量设置存储，尚未实现业务表的版本迁移和升级后自动回滚。引入正式业务数据前，应为该应用设计迁移和恢复策略。当前版本不提供便携安装模式；此前使用便携目录的用户需要单独迁移，不能直接把旧目录识别逻辑移除当作数据迁移。

## 快速开始

```bash
# 首次
pnpm install

# 开发（前端 + Tauri 一体启动）
pnpm dev:tauri

# 只跑前端（Web 模式，可在浏览器调试）
pnpm dev:web

# 检查（前端 typecheck + cargo check）
pnpm check

# 代码规范（详见下方「代码规范与工具链」）
pnpm lint          # ESLint 检查前端与脚本
pnpm format        # Prettier 格式化（含 .editorconfig 约束）
pnpm fmt:rs:check  # rustfmt 检查（仅自有 crate，跳过 vendor/）

# 构建安装包
pnpm tauri build
```

## 四大机制（保留自 dbx）

| 机制 | 组成 | 位置 |
| --- | --- | --- |
| 自动更新 | updater 插件 + GitHub Releases(latest.json) + minisign 验签 + 下载进度事件 + 缓存恢复与安装前复验 | `src-tauri/src/commands/update*.rs`、`crates/app-core/src/update.rs`、`crates/app-platform/src/download.rs` |
| 安装打包 | 仅 Windows：NSIS(currentUser)，`createUpdaterArtifacts: true` 生成 latest.json | `src-tauri/tauri.conf.json`、`windows/nsis/installer.nsi` |
| 启动机制 | 单实例 + 深链接(myapp://) + 二次启动文件交接 + 启动看门狗 + WebView2 崩溃恢复 + 窗口状态 | `src-tauri/src/lib.rs`、`startup_recovery.rs`、`webview2_recovery.rs` |
| 运行机制 | 托盘(显示/退出) + 关闭行为状态机(前端确认后退出) + 默认/环境变量数据目录 + 迁移门槛 + 调试日志 | `lib.rs`、`data_dir.rs`、`migration_gate.rs` |

## 保留的基础能力

- **设置存储**：SQLite 键值存储 + 迁移表，`Storage` 提供 `get_json/set_json/get_setting/set_setting`，任意业务配置直接存取
- **桌面设置**：调试日志开关、托盘显隐、托盘图标主题（黑/默认）
- **外观设置**：主题/强调色 patch 持久化
- **窗口控制**：macOS 红绿灯位置（Overlay 标题栏配套）
- **深链接**：`myapp://` 通用解析（`parse_deep_link`）+ 单实例去重交接
- **外部文件打开**：二次启动 `file_paths_from_args` 安全提取 + 前端事件
- **系统信息**：支持信息（进程/系统/版本）、诊断快照、系统字体列表、macOS keychain 存取
- **调试日志**：`load_native_debug_logs`（最近 6 个文件、单文件 512KiB）

## 裁剪清单（相对 dbx）

- 7 个 `dbx-driver` crate + `dbx-sql`×5 + `dbx-ai-provider` / `dbx-mcp` / `dbx-cli` / `dbx-web`：全部移除
- `src-tauri` 约 40 个数据库命令（连接/表/SQL 执行/导入导出等）：移除
- 前端 connection/grid/editor 等 30+ 组件与 CodeMirror/虚拟滚动/图表库：移除
- `packages/*`（cli/mcp/插件）、`plugins/*`（JDBC 插件）、`agents/*`（Java 驱动代理）：移除
- CI：Nix / Docker / fnOS / CNB / Windows 7 / VSign / Feishu 通知：移除

## MCP 代理（Codex 接入）

MyApp 内置一个 **MCP 网关**（默认 `http://127.0.0.1:9876/mcp`，Streamable HTTP + stdio 双入口），把 MyApp 上配置的外部 MCP 服务器**聚合**给 Codex：

- `tools/list` 回答来自**持久化工具缓存**——外部 MCP 没启动时，Codex 启动后依然能发现这些工具；
- `tools/call` **实时转发**到对应外部 MCP——外部 MCP 后启动后，直接调用即成功，**无需重启 Codex / 恢复会话**。
- **按需启动桌面应用**：为后端配置 `autoStart` 后，实际调用或手动刷新会自动打开未运行的应用并等待本地服务就绪；并发请求合并启动，Windows 不弹控制台黑窗。

借鉴 [mcp-gateway](https://github.com/MikkoParkkola/mcp-gateway) 补齐的能力：

- **自动预取（默认开）**：启动时后台连接外部 MCP 扫描工具写入缓存（失败退避重试），无需手动点刷新；
- **熔断保护**：后端连续失败自动熔断（快速失败而非挂 60s 超时），超时自动恢复，支持手动停用/恢复（kill switch / revive）；
- **Meta 模式（可选）**：`metaMode: true` 时只暴露 4 个 `gateway_*` 元工具（search/list/invoke/servers），后端很多时避免工具清单撑爆上下文。

接入步骤与配置格式见 [`MCP代理-Codex接入指南.md`](./MCP代理-Codex接入指南.md)。应用内置 **MCP 管理面板**（主界面下方）：查看网关状态 / 服务器列表 / 状态徽标（正常/故障/熔断/停用）/ 工具缓存，一键「刷新工具列表」「重载配置」「停用/恢复」；后端命令 `mcp_status` / `mcp_reload_config` / `mcp_refresh_tools` / `mcp_kill_server` / `mcp_revive_server` 亦可直接调用。

## 本机端口监测

侧边栏「端口监测」自动读取 Windows 本机全部 TCP 监听端口和 UDP 绑定地址，无需指定端口或登记启动项，也能发现其他 Agent 或终端启动的前后端。支持按端口、PID、进程名、路径搜索，按协议与疑似开发服务筛选，页面打开时每 10 秒自动刷新。

同一进程的端口合并展示，附程序路径、启动命令、父 PID 和根据路径推断的项目目录。开发服务分类与目录仅作线索，不能据此确定启动者是 Agent；相对路径命令或无法读取的进程可能无法识别项目。浏览器预览不具备扫描能力。

「停止进程」立即结束指定进程并释放其全部端口；勾选多个进程可以一起停止前后端（最多 64 个）。后端重新扫描并通过同一 Windows 进程句柄验证 PID 与创建时间，避免 PID 复用造成误停。系统服务、Windows 系统目录下的进程、应用自身及祖先进程、身份无法验证的进程禁止停止。不会按名称批量结束同名程序，也不会自动结束父进程或进程树；由 watcher、容器或守护程序管理的服务可能自动重启，需要在对应启动器中停止。扫描/停止权限不足会显示错误，不自动提升权限。

## 写你的业务

1. **后端命令**：在 `src-tauri/src/commands/business/` 新建模块（脚手架自带 `business_ping` / `business_echo` 示例），在 `commands/mod.rs` 声明、`lib.rs` 的 `generate_handler!` 里注册；前端在 `lib/backend.ts` 补类型化封装。
   > 命令若依赖 `tauri::State<T>`，务必在 Builder 里 `.manage(...)` 注册，否则运行时报 `state not managed for field ...`（更新按钮"点了没反应"就是这么来的）。
2. **前端页面**：在 `apps/desktop/src/pages/` 新建页面，在 `router/index.ts` 加一条 route（hash 模式），侧边导航加 `<RouterLink>`；页面状态放进 `stores/` 的 Pinia store，不要写进 `App.vue`。
3. **UI 与主题**：优先复用 `components/ui/` 基础组件（BaseButton / BaseCard / BaseField / BaseToggle）；颜色一律引用 `styles.css` 的 CSS 变量（自动适配暗色模式），不要写死色值。
4. **业务数据**：用 `AppState.storage` 的 `get_json/set_json` 或加业务表（见 `crates/app-core/src/storage.rs`）。
5. **应用信息**：改 `src-tauri/tauri.conf.json`（productName / identifier / 版本 / 更新端点 / 签名公钥）。

## 代码规范与工具链

| 用途 | 命令 | 配置文件 |
| --- | --- | --- |
| 前端 lint | `pnpm lint` / `pnpm lint:fix` | `eslint.config.js`（flat config，ESLint 10 + typescript-eslint + eslint-plugin-vue） |
| 前端格式化 | `pnpm format` / `pnpm format:check` | `.prettierrc.json` + `.prettierignore` |
| 编辑器约定 | 自动生效 | `.editorconfig`（通用 2 空格、`*.rs` 4 空格、LF） |
| Rust 格式化 | `pnpm fmt:rs` / `pnpm fmt:rs:check` | `rustfmt.toml`（`max_width = 120`） |

> **注意**：`vendor/` 下的 crate 是定制的上游依赖（通过 `[patch.crates-io]` 引入），**不要格式化**。
> `cargo fmt --all` 会连带格式化这些本地路径依赖，因此 `fmt:rs` 脚本显式用 `-p` 只格式化自有 crate
> （`myapp` / `app-core` / `app-platform` / `app-mcp`）；`.prettierignore` 与 ESLint 的 `ignores` 亦已排除 `vendor/`。

## 版本管理

应用版本号需要出现在 4 个文件里（根 `package.json`、`apps/desktop/package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`），
`scripts/sync-version.mjs` 以**根 `package.json` 为单一事实来源**统一同步：

```bash
pnpm sync-version 0.2.0   # 改根 package.json 并同步到其它 3 处
pnpm sync-version         # 用根 package.json 的现有版本覆盖其它 3 处
pnpm sync-version --check # 只校验四处版本是否一致（不一致退出码 1，可用于 CI）
```

发布记录见 [`CHANGELOG.md`](./CHANGELOG.md)。

## 发布

流水线在 [`.github/workflows/release.yml`](./.github/workflows/release.yml)：推送 `v*` 标签后，在 `windows-2022` 上构建 **Windows NSIS 安装包**，产出 `latest.json` 更新清单并**先构建草稿，校验上传的安装包与更新元数据后自动正式发布**。客户端启动后会自动检查该 Release，弹提示后点一下即可完成更新。

```bash
# 1) 本地完整校验（发布流水线也会强制执行）
pnpm validate

# 2) 同步版本号（4 个文件保持一致）
pnpm sync-version 0.2.0

# 3) 打标签并推送 → 触发流水线
git tag v0.2.0
git push origin v0.2.0
```

发布前需要在仓库配置 Secret：

| Secret | 说明 |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | minisign 私钥内容（`.signing/myapp.key` 的文本内容），用于给安装包签名；`latest.json` 携带签名，清单本身不单独签名 |

> 密钥已按**无口令**生成，因此不需要 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

> 更新地址固定指向 `https://github.com/liuzheng110/yy_desk/releases/latest/download/latest.json`，因此**必须发布正式 Release**，草稿不会被 `latest` 命中。

本地构建需要签名时，设置环境变量后运行 `pnpm tauri build`：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "C:\path\to\myapp\.signing\myapp.key"  # 私钥文件路径（或密钥内容）
pnpm tauri build
```

## 待办（发布前必须替换的占位）

- `src-tauri/icons/`：替换为你的应用图标
- `src-tauri/tauri.conf.json`：按需改 `productName` / `identifier`

## 说明

- **窗口控制按钮（最小化/最大化/关闭）由前端标题栏自绘**（`apps/desktop/src/App.vue` 的 `.window-controls`）：框架沿用 dbx 的 Overlay 无边框标题栏，Windows 原生按钮被隐藏，必须由前端补齐；关闭按钮走框架关闭状态机（当前默认退出应用）。业务替换 `App.vue` 时保留这段标题栏即可。
- **构建正式版/安装包必须用 `pnpm tauri build`（tauri CLI 会注入正式构建标记）。不要直接用 `cargo build --release` 构建 `src-tauri`**——那样 tauri-build 会把它编译成**开发模式**，窗口会加载 `localhost:1420`（没有本地 dev server 时显示"无法访问此页面"）。分发的 exe 一律从 `tauri build` 的产物（`target\release\bundle\` 或 `target\release\myapp.exe`）取。
- 当前关闭按钮与托盘退出均走正常退出流程，避免直接强杀进程（强杀可能残留 single-instance 管道，导致下一次启动窗口被恢复到屏幕外不可见；若遇到"双击没反应/窗口不见了"，先到任务管理器结束残留的 MyApp 进程再启动）。
- 依赖 dbx 的 Apache-2.0 许可，使用/分发时请保留相应版权声明。
