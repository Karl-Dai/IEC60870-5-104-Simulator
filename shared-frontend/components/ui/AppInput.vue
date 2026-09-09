<script setup lang="ts">
// Shared text input with consistent height, focus ring and error state.
import { ref } from 'vue'

const model = defineModel<string>({ default: '' })

withDefaults(defineProps<{
  placeholder?: string
  type?: string
  error?: boolean
  disabled?: boolean
  monospace?: boolean
}>(), {
  placeholder: '',
  type: 'text',
  error: false,
  disabled: false,
  monospace: false,
})

const inputEl = ref<HTMLInputElement | null>(null)
defineExpose({
  focus: () => inputEl.value?.focus(),
  select: () => inputEl.value?.select(),
})
</script>

<template>
  <input
    ref="inputEl"
    v-model="model"
    :type="type"
    :placeholder="placeholder"
    :disabled="disabled"
    :class="['app-input', { error, mono: monospace }]"
    :aria-invalid="error || undefined"
  />
</template>

<style scoped>
.app-input {
  width: 100%;
  padding: 6px var(--space-2);
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-family: inherit;
  font-size: var(--text-md);
  transition: border-color var(--duration-fast) var(--ease-out),
    box-shadow var(--duration-fast) var(--ease-out);
}
.app-input.mono { font-family: var(--font-mono); }
.app-input::placeholder { color: var(--text-disabled); }
.app-input:hover:not(:disabled):not(:focus) { border-color: var(--text-disabled); }
.app-input:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 30%, transparent);
}
.app-input.error { border-color: var(--danger); }
.app-input:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
