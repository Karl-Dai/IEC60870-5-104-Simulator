<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { REPO_URL } from '@app/releaseNotes'
import { useI18n } from '../i18n'
import { useClipboardFlash } from '../composables/useClipboardFlash'

const { t, locale } = useI18n()
const { flash, copy, openOrCopy } = useClipboardFlash()
const version = ref('')
const documentationUrl = computed(() =>
  `${REPO_URL}/blob/main/${locale.value === 'zh-CN' ? 'README_CN.md' : 'README.md'}`,
)

onMounted(async () => {
  try { version.value = await getVersion() } catch { version.value = '' }
})
</script>

<template>
  <div class="version-badge">
    <button
      v-if="version"
      type="button"
      class="version-text"
      :title="`v${version} · ${t('about.copiedSuffix')}`"
      @click="copy(`v${version}`)"
    >v{{ version }}</button>
    <button
      type="button"
      class="github-link"
      :title="documentationUrl"
      :aria-label="documentationUrl"
      @click="openOrCopy(documentationUrl, 'GitHub')"
    >
      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true">
        <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38
                 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13
                 -.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66
                 .07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15
                 -.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0
                 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82
                 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01
                 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z"/>
      </svg>
    </button>
    <span v-if="flash" class="flash">{{ flash }}</span>
  </div>
</template>

<style scoped>
.version-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--c-overlay0);
  font-variant-numeric: tabular-nums;
  font-family: var(--font-mono);
  position: relative;
  padding: 0 4px;
}
.version-text,
.github-link {
  background: transparent;
  border: none;
  padding: 2px 4px;
  margin: 0;
  color: inherit;
  cursor: pointer;
  border-radius: 3px;
  display: inline-flex;
  align-items: center;
  line-height: 1;
}
.version-text { font: inherit; }
.version-text:hover,
.github-link:hover { color: var(--c-text); background: var(--c-surface0); }
.github-link svg { display: block; }
.flash {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  background: var(--c-surface1);
  color: var(--c-text);
  font-size: 10px;
  padding: 3px 8px;
  border-radius: 4px;
  white-space: nowrap;
  pointer-events: none;
  font-family: -apple-system, system-ui, sans-serif;
}
</style>
