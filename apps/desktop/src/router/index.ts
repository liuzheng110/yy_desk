// 路由表。
// 新增功能页：在这里加一条 route，并在 App.vue 的侧边导航里加一个对应的 <RouterLink>。
import { createRouter, createWebHashHistory } from "vue-router";
import type { RouteRecordRaw } from "vue-router";

export const routes: RouteRecordRaw[] = [
  {
    path: "/",
    name: "home",
    component: () => import("@/pages/HomePage.vue"),
    meta: { title: "主功能" },
  },
  {
    path: "/settings",
    name: "settings",
    component: () => import("@/pages/SettingsPage.vue"),
    meta: { title: "设置" },
  },
  {
    path: "/mcp",
    name: "mcp",
    component: () => import("@/pages/McpConfigPage.vue"),
    meta: { title: "MCP 配置" },
  },
  // 未知路径统一回到首页，避免桌面端出现空白页
  { path: "/:pathMatch(.*)*", redirect: "/" },
];

// 桌面端用 hash 路由：自定义协议（tauri://）下 history 模式刷新会 404。
export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
