<script setup lang="ts">
// Switch toggle for boolean settings.
const model = defineModel<boolean>({ default: false })

withDefaults(defineProps<{
  label?: string
  disabled?: boolean
}>(), { label: '', disabled: false })
</script>

<template>
  <label :class="['app-toggle', { on: model, disabled }]">
    <input type="checkbox" v-model="model" :disabled="disabled" class="sr-only" role="switch" :aria-checked="model" />
    <span class="track" aria-hidden="true"><span class="thumb" /></span>
    <span v-if="label || $slots.default" class="label"><slot>{{ label }}</slot></span>
  </label>
</template>

<style scoped>
.app-toggle {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  cursor: pointer;
  font-size: var(--text-md);
  color: var(--text-primary);
  user-select: none;
}
.track {
  position: relative;
  width: 32px;
  height: 18px;
  flex: none;
  border-radius: 9px;
  background: var(--bg-active);
  border: 1px solid var(--border-strong);
  transition: background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}
.thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--text-secondary);
  transition: transform var(--duration-fast) var(--ease-out),
    background var(--duration-fast) var(--ease-out);
}
.on .track {
  background: var(--accent);
  border-color: var(--accent);
}
.on .thumb {
  transform: translateX(14px);
  background: var(--on-accent);
}
.sr-only:focus-visible + .track {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}
.disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
