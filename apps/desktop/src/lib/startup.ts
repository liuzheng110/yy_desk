// 启动握手与关闭行为状态机（对应后端 CloseBehaviorState / 托盘 Quit 兜底）。
//
// 流程：
// 1. 前端挂载完成后调用 mark_frontend_ready()，此后窗口关闭走"前端确认"路径；
// 2. 后端 on_window_event CloseRequested 隐藏窗口并 emit "myapp-app-close-requested"；
// 3. 前端监听该事件，弹确认对话框，再调用 complete_app_close("quit" | "hide")。

import { listen } from "@tauri-apps/api/event";
import { backend } from "./backend";

export const APP_CLOSE_REQUESTED_EVENT = "myapp-app-close-requested";
export const APP_OPEN_FILES_EVENT = "myapp-open-files";
export const APP_OPEN_DEEP_LINKS_EVENT = "myapp-open-deep-links";

let frontendReady = false;

export async function markFrontendReady(): Promise<void> {
  frontendReady = true;
  await backend.markFrontendReady();
}

export function isFrontendReady(): boolean {
  return frontendReady;
}

/**
 * 订阅关闭请求事件。resolve 回调返回用户选择的动作：
 * "quit" 退出进程；"hide" 隐藏到托盘。
 */
export function onAppCloseRequested(
  resolve: (action: "quit" | "hide") => void,
): Promise<() => void> {
  return listen<string>(APP_CLOSE_REQUESTED_EVENT, (event) => {
    const target = event.payload ?? "settings";
    if (target === "quit" && !isFrontendReady()) {
      // 后端兜底：前端未就绪时由托盘 Quit 直接退出。
      resolve("quit");
      return;
    }
    resolve("hide");
  });
}

/** 订阅二次启动带入的文件路径。 */
export function onAppOpenFiles(handler: (paths: string[]) => void): Promise<() => void> {
  return listen<string[]>(APP_OPEN_FILES_EVENT, (event) => handler(event.payload));
}

/** 订阅二次启动带入的深链接。 */
export function onAppOpenDeepLinks(handler: (links: string[]) => void): Promise<() => void> {
  return listen<string[]>(APP_OPEN_DEEP_LINKS_EVENT, (event) => handler(event.payload));
}
