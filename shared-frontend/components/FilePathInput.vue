<script setup lang="ts">
import { computed, useId } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../i18n'

type FileKind = 'certificate' | 'private-key'

const props = defineProps<{
  modelValue: string
  label: string
  placeholder?: string
  kind: FileKind
}>()

const emit = defineEmits<{
  (event: 'update:modelValue', value: string): void
}>()

const { t } = useI18n()
const inputId = useId()

const filterName = computed(() => props.kind === 'private-key'
  ? t('common.privateKeyFiles')
  : t('common.certificateFiles'))

async function chooseFile() {
  const selected = await invoke<string | null>('choose_tls_file', {
    kind: props.kind,
    title: props.label,
    filterName: filterName.value,
  })
  if (typeof selected === 'string') emit('update:modelValue', selected)
}

function updateValue(event: Event) {
  emit('update:modelValue', (event.target as HTMLInputElement).value)
}
</script>

<template>
  <div class="file-path-field">
    <label class="file-path-label" :for="inputId">{{ label }}</label>
    <span class="file-path-row">
      <input
        :id="inputId"
        class="file-path-input"
        type="text"
        :value="modelValue"
        :placeholder="placeholder"
        @input="updateValue"
      />
      <button
        class="file-path-button"
        type="button"
        :aria-label="`${label} — ${t('common.browse')}`"
        @click="chooseFile"
      >
        {{ t('common.browse') }}
      </button>
    </span>
  </div>
</template>

<style scoped>
.file-path-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  font-size: 12px;
  color: var(--c-subtext0, var(--c-overlay0));
}

.file-path-row {
  display: flex;
  align-items: stretch;
  gap: 8px;
  min-width: 0;
}

.file-path-input {
  flex: 1 1 auto;
  min-width: 0;
  padding: 6px 10px;
  box-sizing: border-box;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 13px;
  outline: none;
}

.file-path-input:focus {
  border-color: var(--c-blue);
}

.file-path-button {
  flex: 0 0 auto;
  padding: 6px 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-surface1);
  color: var(--c-text);
  font-size: 12px;
  cursor: pointer;
}

.file-path-button:hover {
  border-color: var(--c-blue);
  color: var(--c-blue);
}

.file-path-button:focus-visible {
  outline: 2px solid var(--c-blue);
  outline-offset: 2px;
}
</style>
