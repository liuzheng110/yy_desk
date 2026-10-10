<script setup lang="ts">
// 错误边界：捕获子组件树里未处理的渲染/生命周期错误，展示兜底 UI 而不是整页白屏。
// 用法：<ErrorBoundary><RouterView /></ErrorBoundary>
import { onErrorCaptured, ref } from "vue";
import { toErrorMessage } from "@/lib/errors";

const message = ref("");
const detail = ref("");

onErrorCaptured((error, _instance, info) => {
  console.error("[UI] captured error", error, info);
  message.value = toErrorMessage(error);
  detail.value = info;
  // 返回 false 阻止错误继续向上冒泡
  return false;
});

function retry() {
  message.value = "";
  detail.value = "";
}
</script>

<template>
  <div v-if="message" class="error-boundary">
    <h2>页面出错了</h2>
    <p class="muted">这个功能页渲染失败，应用其余部分仍可正常使用。</p>
    <pre class="error-boundary__detail">{{ message }}<template v-if="detail">

（{{ detail }}）</template></pre>
    <button class="tb-btn primary" @click="retry">重新加载页面</button>
  </div>
  <slot v-else />
</template>

<style scoped>
.error-boundary {
  border: 1px solid var(--err-border);
  border-radius: var(--radius-lg);
  background: var(--err-bg);
  color: var(--err-text);
  padding: 18px 20px;
}

.error-boundary h2 {
  margin: 0 0 6px;
  font-size: 16px;
}

.error-boundary .muted {
  color: var(--err-text);
  opacity: 0.85;
  margin: 0 0 12px;
}

.error-boundary__detail {
  margin: 0 0 14px;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--bg-app);
  color: var(--text);
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
</style>
