<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import FilePathInput from '@shared/components/FilePathInput.vue'
import type { ServerInfo, ServerTlsSettings, ServerTransportInfo } from '../types'

const props = defineProps<{ visible: boolean; serverId: string | null }>()
const emit = defineEmits<{ close: []; saved: [server: ServerInfo]; stopped: [serverId: string] }>()
const { t } = useI18n()
const form = reactive({ bind_address: '', port: 2404, tls: {
  enabled: false, cert_file: '', key_file: '', ca_file: '', require_client_cert: false,
} as ServerTlsSettings })
const loading = ref(false)
const saving = ref(false)
const stopping = ref(false)
const ready = ref(false)
const serverState = ref('')
const errorText = ref('')
const baseline = ref('')
let session = 0
const busy = computed(() => loading.value || saving.value || stopping.value)
const mutating = computed(() => saving.value || stopping.value)
const running = computed(() => serverState.value === 'Running')
const locked = computed(() => busy.value || !ready.value || running.value)
const dirty = computed(() => baseline.value !== '' && JSON.stringify(form) !== baseline.value)
const isCurrent = (id: string, epoch: number) => props.visible && props.serverId === id && session === epoch

async function load() {
  const id = props.serverId
  const epoch = ++session
  ready.value = false
  saving.value = false
  stopping.value = false
  errorText.value = ''
  if (!props.visible || !id) { loading.value = false; return }
  loading.value = true
  try {
    const info = await invoke<ServerTransportInfo>('get_server_transport', { serverId: id })
    if (!isCurrent(id, epoch)) return
    form.bind_address = info.bind_address
    form.port = info.port
    form.tls = { ...info.tls }
    serverState.value = info.state
    baseline.value = JSON.stringify(form)
    ready.value = true
  } catch (error) {
    if (isCurrent(id, epoch)) errorText.value = String(error)
  } finally {
    if (isCurrent(id, epoch)) loading.value = false
  }
}

watch([() => props.visible, () => props.serverId], load, { immediate: true, flush: 'sync' })

async function refreshStateOnly(id: string, epoch: number) {
  try {
    const info = await invoke<ServerTransportInfo>('get_server_transport', { serverId: id })
    if (isCurrent(id, epoch)) serverState.value = info.state
  } catch { /* Keep the original error and draft if the state read also fails. */ }
}

async function stopForEditing() {
  const id = props.serverId
  const epoch = session
  if (!id || busy.value || !ready.value || !running.value) return
  stopping.value = true
  errorText.value = ''
  try {
    await invoke('stop_server', { id })
    if (!isCurrent(id, epoch)) return
    serverState.value = 'Stopped'
    emit('stopped', id)
  } catch (error) {
    if (isCurrent(id, epoch)) {
      errorText.value = String(error)
      await refreshStateOnly(id, epoch)
    }
  } finally {
    if (isCurrent(id, epoch)) stopping.value = false
  }
}

async function save() {
  const id = props.serverId
  const epoch = session
  if (!id || locked.value || !dirty.value) return
  const port = Number(form.port)
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    errorText.value = t('errors.invalidPort'); return
  }
  if (form.tls.enabled && (!form.tls.cert_file.trim() || !form.tls.key_file.trim()
    || (form.tls.require_client_cert && !form.tls.ca_file.trim()))) {
    errorText.value = t('serverSettings.requiredFiles'); return
  }
  const request = { server_id: id, bind_address: form.bind_address.trim(), port, tls: { ...form.tls } }
  saving.value = true
  errorText.value = ''
  try {
    const info = await invoke<ServerInfo>('update_server_transport', { request })
    if (!isCurrent(id, epoch)) return
    emit('saved', info)
    emit('close')
  } catch (error) {
    if (isCurrent(id, epoch)) {
      errorText.value = String(error)
      await refreshStateOnly(id, epoch)
    }
  } finally {
    if (isCurrent(id, epoch)) saving.value = false
  }
}

function close() { if (!mutating.value) emit('close') }
function keydown(event: KeyboardEvent) { if (props.visible && event.key === 'Escape') close() }
onMounted(() => window.addEventListener('keydown', keydown))
onBeforeUnmount(() => { session++; window.removeEventListener('keydown', keydown) })
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="settings-backdrop dialog-blur">
        <div class="settings-modal" role="dialog" aria-modal="true" aria-labelledby="server-settings-title" :aria-busy="busy">
          <header>
            <h2 id="server-settings-title">{{ t('serverSettings.title') }}</h2>
            <button type="button" :disabled="mutating" :aria-label="t('common.cancel')" @click="close">×</button>
          </header>
          <div class="settings-body">
            <p v-if="loading" class="hint">{{ t('serverSettings.loading') }}</p>
            <div v-if="ready && running" class="running-notice">
              <p>{{ t('serverSettings.runningHint') }}</p>
              <button type="button" class="secondary" :disabled="busy" @click="stopForEditing">
                {{ stopping ? t('serverSettings.stopping') : t('serverSettings.stopAndEdit') }}
              </button>
            </div>
            <fieldset v-if="ready" :disabled="locked">
              <div class="address-row">
                <label>{{ t('remoteParams.bindAddress') }}<input v-model="form.bind_address" placeholder="0.0.0.0" /></label>
                <label>{{ t('remoteParams.port') }}<input v-model.number="form.port" type="number" min="1" max="65535" /></label>
              </div>
              <label class="tls-toggle"><input v-model="form.tls.enabled" type="checkbox" />{{ t('newServer.enableTls') }}</label>
              <p class="hint">{{ form.tls.enabled ? t('serverSettings.tlsOnHint') : t('serverSettings.tlsOffHint') }}</p>
              <div v-if="form.tls.enabled" class="tls-fields">
                <FilePathInput v-model="form.tls.cert_file" :label="t('newServer.serverCert')" kind="certificate" />
                <FilePathInput v-model="form.tls.key_file" :label="t('newServer.serverKey')" kind="private-key" />
                <label class="tls-toggle"><input v-model="form.tls.require_client_cert" type="checkbox" />{{ t('newServer.requireClientCert') }}</label>
                <FilePathInput v-model="form.tls.ca_file" :label="form.tls.require_client_cert ? t('serverSettings.caRequiredLabel') : t('newServer.caFile')" kind="certificate" />
              </div>
            </fieldset>
            <div v-if="errorText" class="settings-error" role="alert">
              <p>{{ errorText }}</p>
              <button v-if="!ready" type="button" :disabled="busy" @click="load">{{ t('serverSettings.reload') }}</button>
            </div>
          </div>
          <footer>
            <span class="hint">{{ t('serverSettings.saveHint') }}</span>
            <button type="button" class="secondary" :disabled="mutating" @click="close">{{ t('common.cancel') }}</button>
            <button type="button" class="primary" :disabled="locked || !dirty" @click="save">
              {{ saving ? t('runtimeParams.saving') : t('runtimeParams.save') }}
            </button>
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.settings-backdrop { position: fixed; inset: 0; z-index: 2000; display: flex; align-items: center; justify-content: center; background: rgb(0 0 0 / 55%); }
.settings-modal { width: 540px; max-width: 92vw; max-height: 90vh; display: flex; flex-direction: column; background: var(--c-base); color: var(--c-text); border: 1px solid var(--c-surface1); border-radius: 8px; box-shadow: 0 8px 24px rgb(0 0 0 / 50%); }
header { display: flex; align-items: center; justify-content: space-between; padding: 16px 20px; border-bottom: 1px solid var(--c-surface0); }
h2 { margin: 0; font-size: 15px; }
header button { padding: 0 4px; background: none; color: var(--c-subtext0); font-size: 22px; }
.settings-body { padding: 18px 20px; overflow-y: auto; min-height: 0; }
fieldset { margin: 0; padding: 0; border: 0; min-width: 0; }
.address-row { display: grid; grid-template-columns: 1fr 120px; gap: 14px; margin-bottom: 18px; }
.address-row label { display: flex; flex-direction: column; gap: 7px; color: var(--c-subtext0); font-size: 12px; }
.address-row input { min-width: 0; width: 100%; padding: 7px 10px; background: var(--c-surface0); border: 1px solid var(--c-surface1); border-radius: 4px; color: var(--c-text); }
.tls-toggle { display: flex; align-items: center; gap: 8px; font-size: 13px; }
input[type=checkbox] { accent-color: var(--c-blue); }
.hint { color: var(--c-subtext0); font-size: 12px; line-height: 1.5; }
fieldset > .hint { margin: 8px 0 14px; }
.tls-fields { display: grid; gap: 14px; }
.running-notice { padding: 12px; margin-bottom: 16px; border: 1px solid var(--c-peach); border-radius: 5px; }
.running-notice p { margin: 0 0 10px; color: var(--c-peach); font-size: 12px; line-height: 1.5; }
button { padding: 7px 14px; border: 0; border-radius: 5px; font-size: 13px; cursor: pointer; }
.secondary { background: var(--c-surface1); color: var(--c-text); }
.primary { background: var(--c-blue); color: var(--c-base); font-weight: 600; }
button:disabled, input:disabled, fieldset:disabled :deep(input), fieldset:disabled :deep(button) { opacity: .55; cursor: not-allowed; }
button:focus-visible, input:focus-visible { outline: 2px solid var(--c-blue); outline-offset: 2px; }
footer { padding: 14px 20px; display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--c-surface0); }
footer .hint { flex: 1; }
.settings-error { margin-top: 14px; padding: 10px; color: var(--c-red); background: color-mix(in srgb, var(--c-red) 10%, transparent); border-radius: 4px; font-size: 12px; white-space: pre-wrap; overflow-wrap: anywhere; }
.settings-error p { margin: 0; }
@media (max-width: 450px) { footer { flex-wrap: wrap; } footer .hint { flex-basis: 100%; } }
</style>
