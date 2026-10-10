<script setup lang="ts">
// 开关（switch）。受控组件：用 v-model 绑定布尔值。
const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    label?: string;
    disabled?: boolean;
    busy?: boolean;
  }>(),
  { label: "", disabled: false, busy: false },
);

const emit = defineEmits<{ (e: "update:modelValue", value: boolean): void }>();

function toggle() {
  if (props.disabled || props.busy) return;
  emit("update:modelValue", !props.modelValue);
}
</script>

<template>
  <button
    type="button"
    class="ui-toggle"
    role="switch"
    :aria-checked="modelValue"
    :aria-label="label"
    :disabled="disabled || busy"
    :class="{ 'is-on': modelValue, 'is-busy': busy }"
    @click="toggle"
  >
    <span class="ui-toggle__track">
      <span class="ui-toggle__thumb" />
    </span>
    <span v-if="label" class="ui-toggle__label">{{ label }}</span>
  </button>
</template>

<style scoped>
.ui-toggle {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  border: none;
  background: transparent;
  padding: 0;
  font: inherit;
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
}

.ui-toggle:disabled {
  opacity: 0.55;
  cursor: default;
}

.ui-toggle.is-busy {
  cursor: progress;
}

.ui-toggle__track {
  position: relative;
  flex: 0 0 auto;
  width: 34px;
  height: 20px;
  border-radius: 999px;
  background: var(--border-strong);
  transition: background-color 140ms ease;
}

.ui-toggle.is-on .ui-toggle__track {
  background: var(--primary);
}

.ui-toggle__thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  transition: transform 140ms ease;
}

.ui-toggle.is-on .ui-toggle__thumb {
  transform: translateX(14px);
}

.ui-toggle:focus-visible {
  outline: none;
}

.ui-toggle:focus-visible .ui-toggle__track {
  box-shadow: 0 0 0 2px var(--focus-ring);
}
</style>
