// 框架级 IPC 封装。
// 你的业务命令在 commands/business/ 注册后，在这里补一层类型化封装。

import { invoke } from "@tauri-apps/api/core";

export interface DesktopSettings {
  debug_logging_enabled: boolean;
  show_tray_icon: boolean;
  icon_theme: "Default" | "Black";
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  portable_mode: boolean;
  manual_update_only: boolean;
  release_name: string;
  release_url: string;
  release_notes: string;
}

/** 已下载并校验通过、等待安装的更新。 */
export interface DownloadedUpdate {
  cache_id: string;
  version: string;
  portable_mode: boolean;
  release_url: string;
  release_notes: string;
  downloaded_at: number;
}

/** `update-download-progress` 事件负载。 */
export interface UpdateDownloadProgress {
  downloaded: number;
  total: number | null;
  attempt_id: string;
  version: string;
}

export const UPDATE_DOWNLOAD_PROGRESS_EVENT = "update-download-progress";

export const backend = {
  // 设置
  loadDesktopSettings: () => invoke<DesktopSettings>("load_desktop_settings"),
  saveDesktopSettings: (settings: DesktopSettings) => invoke<void>("save_desktop_settings", { settings }),

  // 关闭行为
  markFrontendReady: () => invoke<void>("mark_frontend_ready"),
  completeAppClose: (action: "quit" | "hide") => invoke<void>("complete_app_close", { action }),

  // 窗口
  showMainWindow: () => invoke<boolean>("show_main_window"),

  // 开机自启动
  getAutostartEnabled: () => invoke<boolean>("get_autostart_enabled"),
  setAutostartEnabled: (enabled: boolean) => invoke<void>("set_autostart_enabled", { enabled }),

  // 更新
  checkForUpdates: () => invoke<UpdateInfo>("check_for_updates"),
  downloadUpdate: (latestVersion: string, attemptId: string, releaseNotes: string) =>
    invoke<DownloadedUpdate>("download_update", { latestVersion, attemptId, releaseNotes }),
  installDownloadedUpdate: (cacheId: string, expectedVersion: string) =>
    invoke<void>("install_downloaded_update", { cacheId, expectedVersion }),
  getDownloadedUpdate: () => invoke<DownloadedUpdate | null>("get_downloaded_update"),
  discardDownloadedUpdate: (cacheId: string) => invoke<void>("discard_downloaded_update", { cacheId }),
  cancelUpdateDownload: () => invoke<void>("cancel_update_download"),

  // 信息
  getAppVersion: () => invoke<string>("get_app_version"),
  getPlatform: () => invoke<string>("get_platform"),
  getSupportInfo: () => invoke<string>("get_support_info"),

  // MCP 代理网关
  mcpStatus: () => invoke<any>("mcp_status"),
  mcpReloadConfig: () => invoke<any>("mcp_reload_config"),
  mcpRefreshTools: () => invoke<any>("mcp_refresh_tools"),
  mcpKillServer: (name: string) => invoke<any>("mcp_kill_server", { name }),
  mcpReviveServer: (name: string) => invoke<any>("mcp_revive_server", { name }),
  mcpGetConfig: () => invoke<{ path: string; config: unknown }>("mcp_get_config"),
  mcpSaveConfig: (config: unknown) => invoke<any>("mcp_save_config", { config }),

  // 业务示例命令（脚手架，替换成你自己的）
  businessPing: (name?: string) => invoke<string>("business_ping", { name }),
  businessEcho: (payload: unknown) => invoke<unknown>("business_echo", { payload }),
};
