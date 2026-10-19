# Changelog

本文件记录 MyApp 的显著变更。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.5] - 2026-10-19

### 底座加固

- 统一读取 Tauri 配置中的应用身份、深链接和 GitHub 更新地址，减少派生应用漏改配置导致的串用。
- 前端完成挂载和事件订阅后才报告启动就绪；订阅失败时清理已建立的监听。
- 修复已下载更新再次检查时的状态冲突、重复安装请求及失效缓存重试；过滤其他下载尝试的进度事件。
- 修复 Windows 桌面壳测试缺少 Common Controls v6 清单导致的入口点错误。
- 增加本地/CI 完整检查、发布标签与仓库校验、公开文件检查及回归测试。发布先生成草稿，复验上传的安装包签名和清单后再自动公开；拒绝覆盖已公开版本。
- 补齐私人配置与备份忽略规则，移除源码中的个人默认启动路径，保留已有本机配置；本地 MCP 增加 Host/Origin 校验。

### 当前兼容范围

- 保留当前应用标识与更新公钥；本轮不改变用户数据存储格式。
- 分发形式为 Windows NSIS 安装包。旧便携数据需要单独迁移。

## [0.1.4] - 2026-10-10

### 新增

- 前端工程化脚手架：`vue-router` 路由 + 布局壳、`Pinia` 状态管理、通用 UI 组件（Button/Card/Field/Toggle）、全局 Toast 与错误边界。
- 设计令牌与暗色模式：`styles.css` 统一 CSS 变量，`lib/theme.ts` 支持 浅色/深色/跟随系统 三态并持久化。
- 代码规范工具链：ESLint(flat) + Prettier + EditorConfig + rustfmt 配置与脚本。
- 版本同步脚本 `scripts/sync-version.mjs`（根 `package.json` 为单一事实来源）。
- MCP 可视化配置页（`mcp_get_config` / `mcp_save_config`，保存后自动重载网关）。

### 变更

- `App.vue` 拆分：仅保留应用外壳（标题栏 + 侧边导航 + 路由出口），业务页面移入 `apps/desktop/src/pages/`，更新逻辑抽到 `stores/updater.ts`。

## [0.1.3]

### 修复

- 修复点击「立即更新」无响应的问题：去 dbx 化重构时遗漏了 `PendingUpdateState` 的 `.manage()` 注册，导致依赖 `tauri::State` 的更新命令被框架拦截。

## [0.1.0]

### 新增

- 首个可用版本：从 dbx 桌面壳裁剪而来，保留**自动更新 / 安装打包 / 启动 / 运行**四大机制，移除全部数据库业务功能。
- 仅发布 Windows NSIS 安装包；更新源固定为 GitHub Releases（`latest.json` + minisign 验签）。
- 内置 MCP 网关（Streamable HTTP + stdio），聚合外部 MCP 服务器供 Codex 调用。
