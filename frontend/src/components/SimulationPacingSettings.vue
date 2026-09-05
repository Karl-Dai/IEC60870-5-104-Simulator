<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import type { RandomMutationPacing, RemoteOperationConfig } from '../types'

const props = defineProps<{ serverId: string; visible: boolean }>()
const { t } = useI18n()
const batchSize = ref<number | string>(2000)
const delayMs = ref<number | string>(50)
const loading = ref(false)
const saving = ref(false)
const loaded = ref(false)
const error = ref('')
const saved = ref(false)
let epoch = 0
const valid = computed(() =>
  typeof batchSize.value === 'number' && Number.isInteger(batchSize.value) && batchSize.value >= 1 && batchSize.value <= 100000 &&
  typeof delayMs.value === 'number' && Number.isInteger(delayMs.value) && delayMs.value >= 0 && delayMs.value <= 60000,
)

async function load() {
  const session = ++epoch
  const id = props.serverId
  loaded.value = false
  saving.value = false
  saved.value = false
  error.value = ''
  loading.value = props.visible && !!id
  if (!loading.value) return
  try {
    const ops = await invoke<RemoteOperationConfig>('get_remote_operation_config', { serverId: id })
    if (session !== epoch) return
    batchSize.value = ops.random_pacing.batch_size
    delayMs.value = ops.random_pacing.delay_ms
    loaded.value = true
  } catch (e) {
    if (session === epoch) error.value = String(e)
  } finally {
    if (session === epoch) loading.value = false
  }
}

async function save() {
  if (!valid.value || !loaded.value || saving.value) return
  const session = epoch
  const id = props.serverId
  const pacing = { batch_size: Number(batchSize.value), delay_ms: Number(delayMs.value) }
  saving.value = true
  saved.value = false
  error.value = ''
  try {
    const result = await invoke<RandomMutationPacing>('set_simulation_pacing', { serverId: id, pacing })
    if (session !== epoch) return
    batchSize.value = result.batch_size
    delayMs.value = result.delay_ms
    saved.value = true
  } catch (e) {
    if (session === epoch) error.value = String(e)
  } finally {
    if (session === epoch) saving.value = false
  }
}

watch([() => props.serverId, () => props.visible], load, { immediate: true })
onBeforeUnmount(() => { epoch++ })
</script>

<template>
  <section class="pacing-settings">
    <h4>{{ t('remoteParams.randomPacing') }}</h4>
    <p class="pacing-hint">{{ t('remoteParams.pacingHint') }}</p>
    <form @submit.prevent="save" @input="saved = false">
      <fieldset :disabled="loading || saving || !loaded">
        <label>
          <span>{{ t('remoteParams.perSend') }}</span>
          <div class="pacing-input">
            <input v-model.number="batchSize" class="pacing-batch" type="number" min="1" max="100000" step="1" required />
            <span>{{ t('remoteParams.unitCount') }}</span>
          </div>
        </label>
        <label>
          <span>{{ t('remoteParams.delay') }}</span>
          <div class="pacing-input">
            <input v-model.number="delayMs" class="pacing-delay" type="number" min="0" max="60000" step="1" required />
            <span>ms</span>
          </div>
        </label>
      </fieldset>
      <div class="pacing-actions">
        <button class="pacing-save" type="submit" :disabled="loading || saving || !loaded || !valid">
          {{ loading || saving ? t('common.loading') : t('common.save') }}
        </button>
        <button v-if="!loaded && !loading" type="button" @click="load">{{ t('common.refresh') }}</button>
        <span v-if="saved" role="status">{{ t('remoteParams.pacingSaved') }}</span>
      </div>
      <p v-if="error" class="pacing-error" role="alert">{{ error }}</p>
    </form>
  </section>
</template>

<style scoped>
.pacing-settings { margin-bottom: 14px; padding: 14px; border: 1px solid var(--c-surface0); border-radius: 6px; background: var(--c-base); }
h4 { margin: 0; font-size: 13px; }
.pacing-hint { margin: 8px 0 12px; color: var(--c-subtext0); font-size: 12px; line-height: 1.6; }
fieldset { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; border: 0; padding: 0; margin: 0; min-width: 0; }
label { display: flex; flex-direction: column; gap: 6px; min-width: 0; font-size: 12px; color: var(--c-subtext1); }
.pacing-input { display: flex; align-items: center; gap: 6px; }
input { width: 100%; min-width: 0; box-sizing: border-box; padding: 7px 8px; color: var(--c-text); background: var(--c-mantle); border: 1px solid var(--c-surface1); border-radius: 4px; }
button { padding: 7px 12px; color: var(--c-text); background: var(--c-surface0); border: 1px solid var(--c-surface1); border-radius: 4px; cursor: pointer; }
.pacing-save { color: var(--c-base); background: var(--c-blue); border-color: var(--c-blue); }
:disabled { opacity: .55; cursor: default; }
input:focus-visible, button:focus-visible { outline: 2px solid var(--c-blue); outline-offset: 2px; }
.pacing-actions { display: flex; align-items: center; gap: 10px; margin-top: 12px; font-size: 12px; color: var(--c-green); }
.pacing-error { color: var(--c-red); overflow-wrap: anywhere; font-size: 12px; }
</style>
