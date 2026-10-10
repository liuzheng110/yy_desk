<script setup lang="ts">
// 表单字段容器：标签 + 控件 + 提示/错误。控件通过默认插槽传入，
// 用插槽参数把 id 绑到控件上即可获得「点标签聚焦控件」的能力。
import { useId } from "vue";

defineProps<{
  label: string;
  hint?: string;
  error?: string;
}>();

const uid = useId();
</script>

<template>
  <label class="ui-field" :for="uid">
    <span class="ui-field__label">{{ label }}</span>
    <slot :id="uid" :invalid="Boolean(error)" />
    <span v-if="error" class="ui-field__error">{{ error }}</span>
    <span v-else-if="hint" class="ui-field__hint">{{ hint }}</span>
  </label>
</template>

<style scoped>
.ui-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 12.5px;
  color: var(--text);
  margin-bottom: 10px;
}

.ui-field__label {
  font-weight: 500;
}

.ui-field__hint {
  color: var(--text-muted);
  font-size: 11.5px;
}

.ui-field__error {
  color: var(--err-text);
  font-size: 11.5px;
}
</style>
