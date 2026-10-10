<script setup lang="ts">
// Toast 容器：固定右下角堆叠显示。已在 App.vue 挂载，业务侧只需调用 lib/toast.ts。
import { dismissToast, useToasts } from "@/lib/toast";

const toasts = useToasts();
</script>

<template>
  <Teleport to="body">
    <div class="toast-host" role="status" aria-live="polite">
      <div v-for="item in toasts" :key="item.id" class="toast" :class="`toast--${item.kind}`">
        <span class="toast__text">{{ item.text }}</span>
        <button class="toast__close" aria-label="关闭提示" @click="dismissToast(item.id)">✕</button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-host {
  position: fixed;
  right: 16px;
  bottom: 16px;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-end;
  pointer-events: none;
}

.toast {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 10px;
  max-width: 380px;
  padding: 9px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-app);
  color: var(--text-strong);
  font-size: 12.5px;
  line-height: 1.5;
  box-shadow: var(--shadow-card);
  animation: toast-in 160ms ease-out;
}

.toast--success {
  border-color: var(--ok-border);
  background: var(--ok-bg);
  color: var(--ok-text);
}

.toast--error {
  border-color: var(--err-border);
  background: var(--err-bg);
  color: var(--err-text);
}

.toast__text {
  min-width: 0;
  word-break: break-word;
  user-select: text;
}

.toast__close {
  flex: 0 0 auto;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 12px;
  line-height: 1;
  padding: 2px 4px;
  cursor: pointer;
  opacity: 0.65;
}

.toast__close:hover {
  opacity: 1;
}

@keyframes toast-in {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
</style>
