<script setup lang="ts">
import { useTheme } from '../composables/useTheme'
import { useI18n } from '../i18n'

const { theme, toggleTheme } = useTheme()
const { t } = useI18n()
</script>

<template>
  <button
    class="theme-switch"
    :title="theme === 'dark' ? t('theme.toLight') : t('theme.toDark')"
    :aria-label="theme === 'dark' ? t('theme.toLight') : t('theme.toDark')"
    @click="toggleTheme"
  >
    <!-- sun -->
    <svg v-if="theme === 'dark'" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </svg>
    <!-- moon -->
    <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor"
      stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
    </svg>
  </button>
</template>

<style scoped>
.theme-switch {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  margin-right: 8px;
  background: transparent;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  color: var(--text-muted);
  cursor: pointer;
  transition: color var(--duration-fast) var(--ease-out),
    background var(--duration-fast) var(--ease-out);
}
.theme-switch:hover {
  color: var(--text-primary);
  background: var(--bg-hover);
}
svg {
  width: 14px;
  height: 14px;
}
</style>
