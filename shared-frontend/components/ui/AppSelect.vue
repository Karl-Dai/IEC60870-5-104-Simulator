<script setup lang="ts">
// Shared <select> with themed chrome (native dropdown list, custom trigger).
import AppIcon from './AppIcon.vue'

const model = defineModel<string>({ default: '' })

withDefaults(defineProps<{
  options: { value: string; label: string }[]
  disabled?: boolean
}>(), { disabled: false })
</script>

<template>
  <span class="app-select">
    <select v-model="model" :disabled="disabled">
      <option v-for="opt in options" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
    </select>
    <AppIcon name="chevron-down" :size="13" />
  </span>
</template>

<style scoped>
.app-select {
  position: relative;
  display: inline-flex;
  align-items: center;
  width: 100%;
}
select {
  appearance: none;
  width: 100%;
  padding: 6px calc(var(--space-4) + 8px) 6px var(--space-2);
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-family: inherit;
  font-size: var(--text-md);
  cursor: pointer;
  transition: border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}
select:hover:not(:disabled):not(:focus) { border-color: var(--text-disabled); }
select:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 30%, transparent);
}
select:disabled {
  opacity: 0.5;
  cursor: default;
}
.app-select svg {
  position: absolute;
  right: var(--space-2);
  pointer-events: none;
  color: var(--text-muted);
}
</style>
