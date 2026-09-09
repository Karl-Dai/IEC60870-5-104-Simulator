<script setup lang="ts">
// CSS-only tooltip. Wrap any element; text shows above it on hover/focus.
withDefaults(defineProps<{
  text: string
  placement?: 'top' | 'bottom'
}>(), { placement: 'top' })
</script>

<template>
  <span class="app-tooltip" :data-tip="text" :data-placement="placement">
    <slot />
  </span>
</template>

<style scoped>
.app-tooltip {
  position: relative;
  display: inline-flex;
}
.app-tooltip::after {
  content: attr(data-tip);
  position: absolute;
  left: 50%;
  bottom: calc(100% + 6px);
  transform: translateX(-50%) translateY(2px);
  padding: 4px var(--space-2);
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md);
  color: var(--text-primary);
  font-size: var(--text-xs);
  white-space: nowrap;
  z-index: var(--z-dropdown);
  opacity: 0;
  pointer-events: none;
  transition: opacity var(--duration-fast) var(--ease-out),
    transform var(--duration-fast) var(--ease-out);
}
.app-tooltip[data-placement='bottom']::after {
  bottom: auto;
  top: calc(100% + 6px);
  transform: translateX(-50%) translateY(-2px);
}
.app-tooltip:hover::after,
.app-tooltip:focus-within::after {
  opacity: 1;
  transform: translateX(-50%) translateY(0);
}
</style>
