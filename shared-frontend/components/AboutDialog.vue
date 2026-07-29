<script setup lang="ts">
import { computed, ref, onMounted } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { ABOUT_RELEASE_NOTES, APP_NAME, REPO_URL, RELEASES_URL } from '@app/releaseNotes'
import { useI18n } from '../i18n'
import { documentationUrlForLocale } from '../i18n/documentation'
import { useClipboardFlash } from '../composables/useClipboardFlash'

defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()

const { t, locale } = useI18n()
const { flash: copied, openOrCopy } = useClipboardFlash()
const version = ref('')
const releaseNotes = computed(() => ABOUT_RELEASE_NOTES[locale.value])
const documentationUrl = computed(() => documentationUrlForLocale(REPO_URL, locale.value))
onMounted(async () => {
  try { version.value = await getVersion() } catch { version.value = '' }
})
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="about-backdrop dialog-blur" @mousedown.self="emit('close')">
      <div class="about-box" role="dialog" aria-modal="true">
        <div class="about-header">
          <div class="about-title">{{ APP_NAME }}</div>
          <div class="about-version">v{{ version || '—' }}</div>
        </div>
        <div class="about-body">
          <div class="about-section-title">{{ t('about.whatsNew') }}</div>
          <ul class="about-notes">
            <li v-for="(note, i) in releaseNotes" :key="i">{{ note }}</li>
          </ul>
          <div class="about-links">
            <a href="#" @click.prevent="openOrCopy(documentationUrl, t('about.homepageLabel'))">{{ t('about.homepageLabel') }}</a>
            <span class="about-sep">·</span>
            <a href="#" @click.prevent="openOrCopy(RELEASES_URL, t('about.releasesLabel'))">{{ t('about.releasesLabel') }}</a>
            <span v-if="copied" class="about-toast">{{ copied }}</span>
          </div>
        </div>
        <div class="about-footer">
          <button class="btn btn-primary" @click="emit('close')">{{ t('common.close') }}</button>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.about-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2100;
}
.about-box {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 420px;
  max-width: 90vw;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.about-header {
  padding: 18px 22px 10px;
  border-bottom: 1px solid var(--c-surface0);
}
.about-title { font-size: 16px; font-weight: 600; color: var(--c-text); }
.about-version { font-size: 12px; color: var(--c-subtext0); margin-top: 2px; font-variant-numeric: tabular-nums; }
.about-body { padding: 14px 22px 8px; color: var(--c-subtext1); font-size: 13px; }
.about-section-title { color: var(--c-text); font-weight: 600; margin-bottom: 6px; }
.about-notes { margin: 0 0 14px; padding-left: 18px; line-height: 1.65; }
.about-links { font-size: 12px; display: flex; align-items: center; flex-wrap: wrap; gap: 4px; }
.about-links a { color: var(--c-blue); text-decoration: none; cursor: pointer; }
.about-links a:hover { text-decoration: underline; }
.about-sep { color: var(--c-surface2); }
.about-toast { color: var(--c-green); margin-left: 10px; font-size: 11px; }
.about-footer { display: flex; justify-content: flex-end; padding: 8px 22px 16px; }
.btn { padding: 7px 20px; border: none; border-radius: 6px; cursor: pointer; font-size: 13px; }
.btn-primary { background: var(--c-blue); color: var(--c-base); }
.btn-primary:hover { background: var(--c-sapphire); }
</style>
