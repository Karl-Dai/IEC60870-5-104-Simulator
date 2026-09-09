<script setup lang="ts">
// Themed checkbox replacing the platform-default white box that clashed with
// the dark theme. Keyboard operable via the hidden native input.
import AppIcon from './AppIcon.vue'

const model = defineModel<boolean>({ default: false })

withDefaults(defineProps<{
  label?: string
  disabled?: boolean
}>(), { label: '', disabled: false })
</script>

<template>
  <label :class="['app-checkbox', { checked: model, disabled }]">
    <input type="checkbox" v-model="model" :disabled="disabled" class="sr-only" />
    <span class="box" aria-hidden="true">
      <AppIcon v-if="model" name="check" :size="11" />
    </span>
    <span v-if="label || $slots.default" class="label"><slot>{{ label }}</slot></span>
  </label>
</template>

<style scoped>
.app-checkbox {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  cursor: pointer;
  font-size: var(--text-md);
  color: var(--text-primary);
  user-select: none;
}
.box {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  flex: none;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-panel);
  color: var(--on-accent);
  transition: background var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}
.app-checkbox:hover:not(.disabled) .box { border-color: var(--text-disabled); }
.checked .box {
  background: var(--accent);
  border-color: var(--accent);
}
/* Move the keyboard focus ring onto the visible box. */
.sr-only:focus-visible + .box {
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
