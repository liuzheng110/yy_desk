<script setup lang="ts">
// 通用按钮。variant 决定视觉层级，size 决定尺寸。
withDefaults(
  defineProps<{
    variant?: "primary" | "default" | "danger" | "ghost";
    size?: "sm" | "md";
    disabled?: boolean;
    loading?: boolean;
    block?: boolean;
    type?: "button" | "submit";
  }>(),
  {
    variant: "default",
    size: "md",
    disabled: false,
    loading: false,
    block: false,
    type: "button",
  },
);
</script>

<template>
  <button
    class="ui-btn"
    :class="[`ui-btn--${variant}`, `ui-btn--${size}`, { 'ui-btn--block': block }]"
    :type="type"
    :disabled="disabled || loading"
  >
    <span v-if="loading" class="ui-btn__spinner" aria-hidden="true" />
    <slot />
  </button>
</template>

<style scoped>
.ui-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-app);
  color: var(--text);
  font: inherit;
  font-size: 12.5px;
  cursor: pointer;
  white-space: nowrap;
  transition:
    background-color 120ms ease,
    border-color 120ms ease,
    color 120ms ease;
}

.ui-btn--md {
  padding: 6px 13px;
}

.ui-btn--sm {
  padding: 3px 10px;
  font-size: 12px;
}

.ui-btn--block {
  width: 100%;
}

.ui-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  border-color: var(--text-faint);
  color: var(--text-strong);
}

.ui-btn:disabled {
  opacity: 0.6;
  cursor: default;
}

.ui-btn:focus-visible {
  outline: none;
  border-color: var(--primary);
  box-shadow: 0 0 0 2px var(--focus-ring);
}

.ui-btn--primary {
  background: var(--primary);
  border-color: var(--primary);
  color: var(--on-primary);
  font-weight: 600;
}

.ui-btn--primary:hover:not(:disabled) {
  background: var(--primary-hover);
  border-color: var(--primary-hover);
  color: var(--on-primary);
}

.ui-btn--danger {
  color: var(--err-text);
  border-color: var(--err-border);
  background: var(--bg-app);
}

.ui-btn--danger:hover:not(:disabled) {
  background: var(--err-bg);
  border-color: var(--err-text);
  color: var(--err-text);
}

.ui-btn--ghost {
  background: transparent;
  border-color: transparent;
  color: var(--text-muted);
}

.ui-btn--ghost:hover:not(:disabled) {
  background: var(--overlay-hover);
  border-color: transparent;
  color: var(--text-strong);
}

.ui-btn__spinner {
  width: 12px;
  height: 12px;
  border: 2px solid currentColor;
  border-top-color: transparent;
  border-radius: 999px;
  animation: ui-btn-spin 0.7s linear infinite;
}

@keyframes ui-btn-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
