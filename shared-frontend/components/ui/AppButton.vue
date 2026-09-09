<script setup lang="ts">
// Shared button — replaces the per-component .btn/.btn-primary copies.
// States follow the spec table: default / hover / active / disabled all read
// semantic tokens so both themes stay correct.
import AppIcon from './AppIcon.vue'
import type { IconName } from './AppIcon.vue'

withDefaults(defineProps<{
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md'
  icon?: IconName
  loading?: boolean
  disabled?: boolean
  type?: 'button' | 'submit'
}>(), {
  variant: 'secondary',
  size: 'md',
  loading: false,
  disabled: false,
  type: 'button',
})
</script>

<template>
  <button
    :type="type"
    :class="['app-btn', `app-btn-${variant}`, `app-btn-${size}`, { loading }]"
    :disabled="disabled || loading"
  >
    <svg v-if="loading" class="spinner" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="9" stroke="currentColor" stroke-width="2.5"
        stroke-linecap="round" stroke-dasharray="42" stroke-dashoffset="14" />
    </svg>
    <AppIcon v-else-if="icon" :name="icon" :size="size === 'sm' ? 13 : 14" />
    <slot />
  </button>
</template>

<style scoped>
.app-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-1);
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  font-family: inherit;
  font-size: var(--text-md);
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--duration-fast) var(--ease-out),
    color var(--duration-fast) var(--ease-out),
    border-color var(--duration-fast) var(--ease-out);
}
.app-btn-md { padding: 6px var(--space-3); }
.app-btn-sm { padding: 3px var(--space-2); font-size: var(--text-sm); }

.app-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

/* primary: accent fill, readable on-accent text in both themes */
.app-btn-primary {
  background: var(--accent);
  color: var(--on-accent);
  font-weight: 600;
}
.app-btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
.app-btn-primary:active:not(:disabled) { filter: brightness(0.92); }

/* secondary: raised surface with a visible border */
.app-btn-secondary {
  background: var(--bg-raised);
  color: var(--text-primary);
  border-color: var(--border-strong);
}
.app-btn-secondary:hover:not(:disabled) { background: var(--bg-active); }
.app-btn-secondary:active:not(:disabled) { border-color: var(--text-disabled); }

/* ghost: borderless, for toolbars and quiet actions */
.app-btn-ghost {
  background: transparent;
  color: var(--text-primary);
}
.app-btn-ghost:hover:not(:disabled) { background: var(--bg-hover); }

/* danger: destructive actions */
.app-btn-danger {
  background: var(--danger);
  color: #ffffff;
  font-weight: 600;
}
.app-btn-danger:hover:not(:disabled) { filter: brightness(1.08); }
.app-btn-danger:active:not(:disabled) { filter: brightness(0.92); }

.spinner {
  width: 13px;
  height: 13px;
  animation: app-btn-spin 0.8s linear infinite;
}
@keyframes app-btn-spin {
  to { transform: rotate(360deg); }
}
</style>
