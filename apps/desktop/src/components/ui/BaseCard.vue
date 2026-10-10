<script setup lang="ts">
// 通用卡片容器。标题/操作/底部均为可选插槽，正文走默认插槽。
defineProps<{
  title?: string;
}>();
</script>

<template>
  <section class="ui-card">
    <header v-if="title || $slots.header || $slots.actions" class="ui-card__head">
      <div class="ui-card__heading">
        <h3 v-if="title" class="ui-card__title">{{ title }}</h3>
        <slot name="header" />
      </div>
      <div v-if="$slots.actions" class="ui-card__actions">
        <slot name="actions" />
      </div>
    </header>

    <div class="ui-card__body">
      <slot />
    </div>

    <footer v-if="$slots.footer" class="ui-card__foot">
      <slot name="footer" />
    </footer>
  </section>
</template>

<style scoped>
.ui-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-app);
  overflow: hidden;
}

.ui-card__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  padding: 14px 16px;
  border-bottom: 1px solid var(--border-faint);
}

.ui-card__heading {
  min-width: 0;
}

.ui-card__title {
  margin: 0;
  font-size: 15px;
  color: var(--text-strong);
}

.ui-card__actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.ui-card__body {
  padding: 16px;
}

.ui-card__foot {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  padding: 12px 16px;
  border-top: 1px solid var(--border-faint);
  background: var(--bg-panel);
}
</style>
