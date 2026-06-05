<script setup lang="ts">
import { inject, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { correctTimingEdit, formatCorrections, isTimingField, type TimingCorrection } from '@shared/timing'
import type { ConnectionInfo } from '../types'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const selectedConnectionState = inject<Ref<string>>('selectedConnectionState')!
const refreshTree = inject<() => void>('refreshTree')!

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void }>()

// Bumped key to v2 when adding the IEC 104 protocol parameter fields so that
// older persisted forms (v1, missing the new fields) are discarded rather
// than spread-merged into the schema with NaN/undefined values.
const NEW_CONN_FORM_KEY = 'iec104master.newConnForm.v2'
type NewConnForm = {
  target_address: string
  port: number
  /** Free-form text user types: e.g. "1, 2, 3". Parsed on submit. */
  common_addresses_text: string
  use_tls: boolean
  ca_file: string
  cert_file: string
  key_file: string
  accept_invalid_certs: boolean
  tls_version: 'auto' | 'tls12_only' | 'tls13_only'
  t0: number
  t1: number
  t2: number
  t3: number
  k: number
  w: number
  default_qoi: number
  default_qcc: number
  interrogate_period_s: number
  counter_interrogate_period_s: number
  broadcast_address_hex: string
}
const defaultForm = (): NewConnForm => ({
  target_address: '127.0.0.1',
  port: 2404,
  common_addresses_text: '1',
  use_tls: false,
  ca_file: './ca.pem',
  cert_file: './client.pem',
  key_file: './client-key.pem',
  accept_invalid_certs: false,
  tls_version: 'auto',
  t0: 30,
  t1: 15,
  t2: 10,
  t3: 20,
  k: 12,
  w: 8,
  default_qoi: 20,
  default_qcc: 5,
  interrogate_period_s: 0,
  counter_interrogate_period_s: 0,
  broadcast_address_hex: 'FFFF',
})

function parseBroadcastHex(s: string): number | null {
  const trimmed = s.trim()
  if (!/^[0-9a-fA-F]{1,4}$/.test(trimmed)) return null
  return parseInt(trimmed, 16)
}

function parseCAList(s: string): number[] {
  const seen = new Set<number>()
  const out: number[] = []
  for (const tok of s.split(/[,，\s]+/)) {
    if (!tok) continue
    const n = parseInt(tok, 10)
    if (!Number.isFinite(n) || n < 1 || n > 65534) continue
    if (seen.has(n)) continue
    seen.add(n); out.push(n)
  }
  return out
}

function loadForm(): NewConnForm {
  try {
    const raw = localStorage.getItem(NEW_CONN_FORM_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<NewConnForm> & { common_address?: number }
      if (typeof parsed.common_address === 'number' && parsed.common_addresses_text == null) {
        parsed.common_addresses_text = String(parsed.common_address)
      }
      delete (parsed as { common_address?: number }).common_address
      const merged = { ...defaultForm(), ...parsed } as NewConnForm
      const def = defaultForm()
      if (!merged.ca_file) merged.ca_file = def.ca_file
      if (!merged.cert_file) merged.cert_file = def.cert_file
      if (!merged.key_file) merged.key_file = def.key_file
      if (!merged.common_addresses_text) merged.common_addresses_text = def.common_addresses_text
      return merged
    }
  } catch {}
  return defaultForm()
}

const form = ref<NewConnForm>(loadForm())
// When non-null, the dialog is in "edit" mode: clicking 创建 will first
// delete the connection with this id, then create a fresh one with the
// edited form. (IEC 104 connections are stateful runtime objects in the
// backend; we don't have a true "modify in place" command, but
// recreating preserves the ergonomics with one extra round-trip.)
const editingConnId = ref<string | null>(null)

// WebView2 on Windows reports "Windows NT 10.0" for both Win10 and Win11
// (Microsoft never bumped the UA token), so we can't tell the versions apart —
// surface the TLS 1.3 caveat on all Windows. Harmless on Win11/Server 2022+
// where SChannel does support it; critical on Win10/Server 2019 where it
// silently fails the handshake. macOS/Linux backends do support TLS 1.3.
const isWindows = /Windows/i.test(navigator.userAgent)

watch(form, (v) => {
  // Don't pollute the persisted "last new-connection" form with edit-time
  // values from another connection — only save when the user is filling
  // out a *new* connection.
  if (editingConnId.value !== null) return
  try { localStorage.setItem(NEW_CONN_FORM_KEY, JSON.stringify(v)) } catch {}
}, { deep: true })

watch(() => props.visible, (v) => {
  if (!v) editingConnId.value = null
})

function close() {
  emit('update:visible', false)
  editingConnId.value = null
}

function submitButtonLabel(): string {
  return editingConnId.value !== null ? t('common.save') : t('newConn.create')
}

async function openEditConnection(connId: string) {
  try {
    const conns = await invoke<Array<ConnectionInfo>>('list_connections')
    const conn = conns.find((c) => c.id === connId)
    if (!conn) return
    if (conn.state === 'Connected') {
      await showAlert(t('newConn.disconnectFirst'))
      return
    }
    editingConnId.value = connId
    // Pre-fill TLS cert paths from the connection itself (backend is the
    // authoritative source). Fall back to the persisted "new connection" form
    // only when the backend value is empty (e.g. TLS-disabled connection), so
    // toggling TLS on during edit still shows sensible default paths.
    const lf = loadForm()
    form.value = {
      ...lf,
      target_address: conn.target_address,
      port: conn.port,
      common_addresses_text: conn.common_addresses.join(', '),
      use_tls: conn.use_tls,
      ca_file: conn.ca_file || lf.ca_file,
      cert_file: conn.cert_file || lf.cert_file,
      key_file: conn.key_file || lf.key_file,
      accept_invalid_certs: conn.accept_invalid_certs ?? lf.accept_invalid_certs,
      tls_version: conn.tls_version || lf.tls_version,
      t0: conn.t0,
      t1: conn.t1,
      t2: conn.t2,
      t3: conn.t3,
      k: conn.k,
      w: conn.w,
      default_qoi: conn.default_qoi,
      default_qcc: conn.default_qcc,
      interrogate_period_s: conn.interrogate_period_s,
      counter_interrogate_period_s: conn.counter_interrogate_period_s,
      broadcast_address_hex: (conn.broadcast_address ?? 0xFFFF).toString(16).toUpperCase().padStart(4, '0'),
    }
    emit('update:visible', true)
  } catch (e) {
    await showAlert(String(e))
  }
}

function openNew() {
  editingConnId.value = null
  form.value = loadForm()
  emit('update:visible', true)
}

async function createConnection() {
  const cas = parseCAList(form.value.common_addresses_text)
  if (cas.length === 0) {
    await showAlert(t('newConn.invalidCA'))
    return
  }
  const bcast = parseBroadcastHex(form.value.broadcast_address_hex)
  if (bcast === null) {
    await showAlert(t('newConn.broadcastAddressInvalid'))
    return
  }
  try {
    if (editingConnId.value !== null) {
      await invoke('delete_connection', { id: editingConnId.value })
      if (selectedConnectionId.value === editingConnId.value) {
        selectedConnectionId.value = null
        selectedConnectionState.value = 'Disconnected'
      }
    }
    const info = await invoke<ConnectionInfo>('create_connection', {
      request: {
        target_address: form.value.target_address,
        port: form.value.port,
        common_addresses: cas,
        broadcast_address: bcast,
        use_tls: form.value.use_tls,
        ca_file: form.value.ca_file || undefined,
        cert_file: form.value.cert_file || undefined,
        key_file: form.value.key_file || undefined,
        accept_invalid_certs: form.value.accept_invalid_certs,
        tls_version: form.value.use_tls ? form.value.tls_version : undefined,
        t0: form.value.t0,
        t1: form.value.t1,
        t2: form.value.t2,
        t3: form.value.t3,
        k: form.value.k,
        w: form.value.w,
        default_qoi: form.value.default_qoi,
        default_qcc: form.value.default_qcc,
        interrogate_period_s: form.value.interrogate_period_s,
        counter_interrogate_period_s: form.value.counter_interrogate_period_s,
      }
    })
    // Defensive: the form pre-corrects, so this is normally empty. If the
    // backend still adjusted anything (e.g. stale persisted form), tell the user.
    if (info?.timing_corrections && info.timing_corrections.length > 0) {
      await showAlert(t('newConn.timingCorrected', { detail: formatCorrections(info.timing_corrections) }))
    }
    emit('update:visible', false)
    editingConnId.value = null
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

type ProtoFieldKey =
  | 't0' | 't1' | 't2' | 't3' | 'k' | 'w'
  | 'default_qoi' | 'default_qcc'
  | 'interrogate_period_s' | 'counter_interrogate_period_s'
type ProtoField = { key: ProtoFieldKey; label: string; min: number; max?: number; unit?: 'sec' }
const protoFields: ProtoField[] = [
  { key: 't0', label: 't0', unit: 'sec', min: 1, max: 255 },
  { key: 't1', label: 't1', unit: 'sec', min: 1, max: 255 },
  { key: 't2', label: 't2', unit: 'sec', min: 1, max: 255 },
  { key: 't3', label: 't3', unit: 'sec', min: 1, max: 255 },
  { key: 'k', label: 'k', min: 1, max: 32767 },
  { key: 'w', label: 'w', min: 1, max: 32767 },
  { key: 'default_qoi', label: 'newConn.defaultQoi', min: 0, max: 255 },
  { key: 'default_qcc', label: 'newConn.defaultQcc', min: 0, max: 255 },
  { key: 'interrogate_period_s', label: 'newConn.interrogatePeriod', min: 0 },
  { key: 'counter_interrogate_period_s', label: 'newConn.counterInterrogatePeriod', min: 0 },
]
function protoLabel(f: ProtoField): string {
  const base = f.label.includes('.') ? t(f.label) : f.label
  return f.unit === 'sec' ? `${base} (${t('newConn.unitSeconds')})` : base
}

// Edit-aware C3 auto-correction. Fires on blur/change (not per keystroke) so
// intermediate values aren't clobbered. Surfaces what was adjusted inline.
const recentCorrections = ref<TimingCorrection[]>([])
let correctionClearTimer: ReturnType<typeof setTimeout> | undefined
function onProtoChange(key: ProtoFieldKey) {
  if (!isTimingField(key)) return
  const changes = correctTimingEdit(form.value, key)
  if (changes.length === 0) return
  recentCorrections.value = changes
  if (correctionClearTimer) clearTimeout(correctionClearTimer)
  correctionClearTimer = setTimeout(() => { recentCorrections.value = [] }, 6000)
}

defineExpose({ openEditConnection, openNew })
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-backdrop dialog-blur" @mousedown.self="close">
      <div class="modal-box">
        <div class="modal-title">
          {{ editingConnId ? t('newConn.editTitle') : t('newConn.title') }}
        </div>
        <div class="modal-body">
          <label class="form-label">
            {{ t('newConn.targetAddress') }}
            <input v-model="form.target_address" class="form-input" type="text" placeholder="127.0.0.1" />
          </label>
          <label class="form-label">
            {{ t('newConn.port') }}
            <input v-model.number="form.port" class="form-input" type="number" min="1" max="65535" />
          </label>
          <label class="form-label">
            {{ t('newConn.commonAddress') }}
            <input
              v-model="form.common_addresses_text"
              class="form-input"
              type="text"
              placeholder="1, 2, 3"
            />
            <span class="form-hint">{{ t('newConn.commonAddressHint') }}</span>
          </label>
          <label class="form-label">
            {{ t('newConn.broadcastAddress') }}
            <input
              v-model="form.broadcast_address_hex"
              class="form-input hex-input"
              type="text"
              maxlength="4"
              placeholder="FFFF"
            />
            <span class="form-hint">{{ t('newConn.broadcastAddressHint') }}</span>
          </label>

          <details class="proto-section">
            <summary class="proto-summary">{{ t('newConn.protocolParams') }}</summary>
            <div class="proto-grid">
              <label v-for="f in protoFields" :key="f.key" class="form-label">
                <span>{{ protoLabel(f) }}</span>
                <input
                  v-model.number="form[f.key]"
                  class="form-input"
                  type="number"
                  :min="f.min"
                  :max="f.max"
                  @change="onProtoChange(f.key)"
                />
              </label>
            </div>
            <div v-if="recentCorrections.length" class="proto-corrected">
              {{ t('newConn.timingCorrected', { detail: formatCorrections(recentCorrections) }) }}
            </div>
            <div class="form-hint">{{ t('newConn.protocolParamsHint') }}</div>
          </details>

          <label class="form-label form-checkbox tls-toggle">
            <input type="checkbox" v-model="form.use_tls" />
            <span>{{ t('newConn.enableTls') }}</span>
          </label>

          <template v-if="form.use_tls">
            <label class="form-label">
              {{ t('newConn.tlsVersion') }}
              <select v-model="form.tls_version" class="form-input">
                <option value="auto">{{ t('newConn.tlsAuto') }}</option>
                <option value="tls12_only">{{ t('newConn.tls12') }}</option>
                <option value="tls13_only">{{ t('newConn.tls13') }}</option>
              </select>
              <span v-if="form.tls_version === 'tls13_only' && isWindows" class="tls13-warn">
                {{ t('newConn.tls13WinWarn') }}
              </span>
            </label>
            <label class="form-label">
              {{ t('newConn.caFile') }}
              <input v-model="form.ca_file" class="form-input" type="text" placeholder="/path/to/ca.crt" />
            </label>
            <label class="form-label">
              {{ t('newConn.certFile') }}
              <input v-model="form.cert_file" class="form-input" type="text" placeholder="/path/to/client.crt" />
            </label>
            <label class="form-label">
              {{ t('newConn.keyFile') }}
              <input v-model="form.key_file" class="form-input" type="text" placeholder="/path/to/client.key" />
            </label>
            <label class="form-label form-checkbox">
              <input type="checkbox" v-model="form.accept_invalid_certs" />
              <span>{{ t('newConn.acceptInvalidCerts') }}</span>
            </label>
          </template>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="close">{{ t('common.cancel') }}</button>
          <button class="btn btn-primary" @click="createConnection">{{ submitButtonLabel() }}</button>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}
.modal-box {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  padding: 20px;
  min-width: 340px;
  max-height: 86vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.modal-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
  margin-bottom: 16px;
  flex: 0 0 auto;
}
.modal-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1 1 auto;
  overflow-y: auto;
  padding-right: 4px;
}
.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
  flex: 0 0 auto;
}
.form-label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--c-overlay0);
}
.form-input {
  padding: 6px 10px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 13px;
}
.form-input:focus {
  outline: none;
  border-color: var(--c-blue);
}
.form-hint {
  font-size: 11px;
  color: var(--c-overlay0);
  margin-top: 2px;
}
.tls13-warn {
  margin-top: 4px;
  padding: 6px 8px;
  font-size: 11px;
  line-height: 1.45;
  color: var(--c-yellow, var(--c-text));
  background: color-mix(in srgb, var(--c-yellow, var(--c-surface1)) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--c-yellow, var(--c-surface1)) 35%, transparent);
  border-radius: 4px;
}
.tls-toggle {
  padding-top: 4px;
  border-top: 1px solid var(--c-surface0);
}
.proto-section {
  border-top: 1px solid var(--c-surface0);
  padding-top: 8px;
  margin-top: 4px;
}
.proto-summary {
  font-size: 12px;
  color: var(--c-text);
  cursor: pointer;
  padding: 2px 0;
  user-select: none;
}
.proto-summary:hover { color: var(--c-blue); }
.proto-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px 10px;
  margin-top: 6px;
}
.proto-corrected {
  margin-top: 8px;
  padding: 6px 8px;
  font-size: 11px;
  color: var(--c-yellow, var(--c-text));
  background: color-mix(in srgb, var(--c-yellow, var(--c-surface1)) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--c-yellow, var(--c-surface1)) 35%, transparent);
  border-radius: 4px;
}
.proto-grid .form-label {
  flex-direction: column;
  gap: 2px;
}
.proto-grid .form-label > span {
  font-size: 11px;
  color: var(--c-overlay0);
}
.proto-grid .form-input {
  padding: 4px 8px;
  font-size: 12px;
  height: 26px;
  box-sizing: border-box;
}
.proto-grid .form-input::-webkit-outer-spin-button,
.proto-grid .form-input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}
.proto-grid .form-input[type="number"] {
  -moz-appearance: textfield;
  appearance: textfield;
}
.form-checkbox {
  flex-direction: row;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  color: var(--c-text);
  font-size: 13px;
}
.form-checkbox input[type="checkbox"] {
  width: 16px;
  height: 16px;
  accent-color: var(--c-blue);
  cursor: pointer;
}
.btn {
  padding: 7px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}
.btn-primary {
  background: var(--c-blue);
  color: var(--c-base);
}
.btn-primary:hover { background: var(--c-sapphire); }
.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}
.btn-secondary:hover { background: var(--c-surface2); }
</style>
