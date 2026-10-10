<script setup lang="ts">
// 主功能页。把这里的内容换成你的业务 UI 即可，外壳（标题栏/导航/更新）由 App.vue 提供。
import { useRouter } from "vue-router";
import { useAppStore } from "@/stores/app";
import McpPanel from "@/components/McpPanel.vue";

const router = useRouter();
const app = useAppStore();
</script>

<template>
  <section class="page">
    <h1>MyApp 框架已就绪</h1>
    <p class="muted">
      版本 <code>{{ app.version }}</code> · 平台 <code>{{ app.platform }}</code>
    </p>

    <section class="cards">
      <div class="card">
        <h3>四大机制已保留</h3>
        <ul>
          <li>自动更新（updater 插件 + GitHub Releases + minisign 验签 + 便携包）</li>
          <li>安装打包（NSIS，<code>createUpdaterArtifacts: true</code> 生成 latest.json）</li>
          <li>启动机制（单实例 / 深链接 / 启动看门狗 / WebView2 崩溃恢复 / 窗口状态）</li>
          <li>运行机制（托盘 / 关闭行为状态机 / 数据目录三模式 / 迁移门槛）</li>
        </ul>
      </div>
      <div class="card">
        <h3>从这里开始写业务</h3>
        <ol>
          <li>后端：<code>src-tauri/src/commands/business/</code> 新建命令并在 <code>lib.rs</code> 注册</li>
          <li>前端：在 <code>src/pages/</code> 加页面，在 <code>src/router/index.ts</code> 加路由</li>
          <li>配置：<code>src-tauri/tauri.conf.json</code>（名称 / 更新端点 / 签名公钥）</li>
        </ol>
      </div>
    </section>

    <McpPanel @configure="router.push('/mcp')" />
  </section>
</template>
