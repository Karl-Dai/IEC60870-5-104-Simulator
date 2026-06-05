<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { CommandType, ControlResult } from '../types'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()

interface Props {
  visible: boolean
  connectionId: string | null
  commonAddress: number
  prefillIoa?: number | null
  prefillCommandType?: CommandType | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'close'): void
  (e: 'sent'): void
}>()

// Persist user-entered fields across opens AND across app restarts. The
// values reset *only* when the caller explicitly prefills them (clicking
// a specific data point), or on first launch.
const STORAGE_KEY = 'iec104master.controlDialog.v1'
type Persisted = {
  ca: number
  ioa: number
  commandType: CommandType
  selectMode: boolean
  singleValue: string
  doubleValue: string
  stepValue: string
  normalizedValue: string
  scaledValue: string
  floatValue: string
  bitstringValue: number
}
function loadPersisted(): Partial<Persisted> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw) as Partial<Persisted>
  } catch { /* ignore */ }
  return {}
}
const saved = loadPersisted()

const ioa = ref<number>(saved.ioa ?? 0)
const ca = ref<number>(saved.ca ?? 1)
const commandType = ref<CommandType>(saved.commandType ?? 'single')
const selectMode = ref(saved.selectMode ?? false)
const errorMsg = ref('')
const sending = ref(false)
const lastResult = ref<ControlResult | null>(null)

// Advanced parameters (intentionally not persisted — these are per-send
// tweaks the user usually wants to set deliberately each time)
const showAdvanced = ref(false)
const qualifier = ref<number>(0)
const cot = ref<number>(6)

// Value state per type — persisted so the user's last-typed value for
// each command type comes back the next time they open the dialog.
const singleValue = ref(saved.singleValue ?? 'true')
const doubleValue = ref(saved.doubleValue ?? '2')
const stepValue = ref(saved.stepValue ?? '2')
// 旧版本持久化的是 [-1,1) 小数；现在统一存原始 NVA 整数字符串。
// 加载时取整并夹到 i16 范围，避免历史小数残留导致解析失败。
function toRawNorm(x: string | undefined): string {
  const n = Number(x)
  if (x == null || x === '' || !Number.isFinite(n)) return '0'
  return String(Math.max(-32768, Math.min(32767, Math.round(n))))
}
const normalizedValue = ref(toRawNorm(saved.normalizedValue))
const scaledValue = ref(saved.scaledValue ?? '0')
const floatValue = ref(saved.floatValue ?? '0.0')
const bitstringValue = ref<number>(saved.bitstringValue ?? 0)

function savePersisted() {
  const data: Persisted = {
    ca: ca.value,
    ioa: ioa.value,
    commandType: commandType.value,
    selectMode: selectMode.value,
    singleValue: singleValue.value,
    doubleValue: doubleValue.value,
    stepValue: stepValue.value,
    normalizedValue: normalizedValue.value,
    scaledValue: scaledValue.value,
    floatValue: floatValue.value,
    bitstringValue: bitstringValue.value,
  }
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(data)) } catch { /* ignore */ }
}

watch(() => props.visible, (v) => {
  if (v) {
    errorMsg.value = ''
    sending.value = false
    lastResult.value = null
    qualifier.value = 0
    cot.value = 6
    showAdvanced.value = false
    // Only override CA / IOA / commandType when the caller explicitly asked
    // for a specific point (right-click on a data row). Toolbar's "custom
    // control" passes prefillIoa = null, so the previously-saved values
    // — including the CA the user typed last time — are preserved.
    if (props.prefillIoa != null) ioa.value = props.prefillIoa
    if (props.prefillCommandType) commandType.value = props.prefillCommandType
  }
})

watch(commandType, () => {
  qualifier.value = 0
})

const currentValueStr = computed(() => {
  // Coerce to string in every branch — backend expects `value: String` and
  // Vue's v-model on <input type="number"> can yield a JS number depending
  // on the entered text (e.g. typing "123" into a setpoint field), which
  // then fails serde deserialization with
  //   invalid type: integer `123`, expected a string
  switch (commandType.value) {
    case 'single': return String(singleValue.value)
    case 'double': return String(doubleValue.value)
    case 'step': return String(stepValue.value)
    case 'setpoint_normalized': return String(normalizedValue.value)
    case 'setpoint_scaled': return String(scaledValue.value)
    case 'setpoint_float': return String(floatValue.value)
    case 'bitstring': return String(bitstringValue.value)
  }
})

const qualifierHint = computed(() => {
  switch (commandType.value) {
    case 'single':
    case 'double':
    case 'step':
      return t('control.qulqlSingle')
    case 'setpoint_normalized':
    case 'setpoint_scaled':
    case 'setpoint_float':
      return t('control.qulqlSetpoint')
    case 'bitstring':
      return t('control.qulqlBitstring')
  }
})

const isBitstring = computed(() => commandType.value === 'bitstring')

const commandTypes = computed<{ value: CommandType; label: string }[]>(() => [
  { value: 'single', label: t('control.cmdSingle') },
  { value: 'double', label: t('control.cmdDouble') },
  { value: 'step', label: t('control.cmdStep') },
  { value: 'setpoint_normalized', label: t('control.cmdSetNorm') },
  { value: 'setpoint_scaled', label: t('control.cmdSetScaled') },
  { value: 'setpoint_float', label: t('control.cmdSetFloat') },
  { value: 'bitstring', label: t('control.cmdBitstring') },
])

async function send() {
  if (!props.connectionId) return
  errorMsg.value = ''
  sending.value = true
  lastResult.value = null

  try {
    const payload: Record<string, unknown> = {
      connection_id: props.connectionId,
      ioa: ioa.value,
      common_address: ca.value,
      command_type: commandType.value,
      value: currentValueStr.value,
      select: selectMode.value,
      qualifier: qualifier.value,
      cot: cot.value,
    }
    if (isBitstring.value) {
      payload.bitstring = bitstringValue.value >>> 0
    }
    const result = await invoke<ControlResult>('send_control_command', { request: payload })
    lastResult.value = result
    sending.value = false
    // Persist on successful send so the user's last good CA / IOA / type /
    // value combo comes back next time. The dialog stays open so the user
    // can immediately tweak and send again — `lastResult` shows the OK
    // indicator beneath the form for confirmation.
    savePersisted()
    emit('sent')
  } catch (e) {
    errorMsg.value = String(e)
    sending.value = false
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  } else if (e.key === 'Enter') {
    send()
  }
}

// Pull the configured Common Addresses for the current connection so the
// CA field can offer a dropdown instead of forcing free-form numeric
// entry. Cached per-connection for the lifetime of the dialog session;
// re-queried whenever the dialog is opened against a different connection
// (the user might have created or edited a connection in between).
const availableCAs = ref<number[]>([])
async function loadAvailableCAs() {
  if (!props.connectionId) { availableCAs.value = []; return }
  try {
    const conns = await invoke<{ id: string; common_addresses: number[] }[]>('list_connections')
    const conn = conns.find((c) => c.id === props.connectionId)
    availableCAs.value = conn?.common_addresses?.slice() ?? []
  } catch {
    availableCAs.value = []
  }
}
watch(() => [props.visible, props.connectionId] as const, ([v]) => {
  if (v) loadAvailableCAs()
})

// Show the CA as a `<select>` only when the connection has more than one
// configured CA. Single-CA setups stay with the simpler number input.
const useCASelect = computed(() => availableCAs.value.length > 1)

// Special sentinel for "I want to type a CA that's not in the list".
const CUSTOM_CA = -1

// What `<option value>` to show as selected. If the persisted CA isn't in
// the list, we pin the dropdown to "Custom" so the number input appears.
const caSelectValue = computed<number>({
  get: () => availableCAs.value.includes(ca.value) ? ca.value : CUSTOM_CA,
  set: (v: number) => {
    if (v === CUSTOM_CA) {
      // Switching to custom — leave ca.value as-is so the user keeps editing it
      return
    }
    ca.value = v
  },
})
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-backdrop dialog-blur" @mousedown.self="emit('close')" @keydown="handleKeydown">
      <div class="modal-box">
        <div class="modal-title">{{ t('control.title') }}</div>
        <div class="modal-body">
          <div class="form-row">
            <label class="form-label form-label-half">
              {{ t('control.targetCa') }}
              <template v-if="useCASelect">
                <select v-model.number="caSelectValue" class="form-input">
                  <option v-for="opt in availableCAs" :key="opt" :value="opt">CA {{ opt }}</option>
                  <option :value="CUSTOM_CA">{{ t('control.caCustom') }}</option>
                </select>
                <input
                  v-if="caSelectValue === CUSTOM_CA"
                  v-model.number="ca"
                  class="form-input"
                  type="number"
                  min="1"
                  max="65534"
                  style="margin-top: 4px"
                />
              </template>
              <input v-else v-model.number="ca" class="form-input" type="number" min="1" max="65534" />
            </label>
            <label class="form-label form-label-half">
              {{ t('control.ioa') }}
              <input v-model.number="ioa" class="form-input" type="number" min="0" max="16777215" />
            </label>
          </div>

          <label class="form-label">
            {{ t('control.commandType') }}
            <select v-model="commandType" class="form-input">
              <option v-for="ct in commandTypes" :key="ct.value" :value="ct.value">{{ ct.label }}</option>
            </select>
          </label>

          <!-- Single point: toggle -->
          <div v-if="commandType === 'single'" class="ctrl-buttons">
            <button :class="['ctrl-btn', { active: singleValue === 'false' }]" @click="singleValue = 'false'">{{ t('control.optOff') }}</button>
            <button :class="['ctrl-btn', { active: singleValue === 'true' }]" @click="singleValue = 'true'">{{ t('control.optOn') }}</button>
          </div>

          <!-- Double point: 4 buttons -->
          <div v-else-if="commandType === 'double'" class="ctrl-buttons">
            <button :class="['ctrl-btn ctrl-btn-sm', { active: doubleValue === '0' }]" @click="doubleValue = '0'">{{ t('control.optIntermediate') }}</button>
            <button :class="['ctrl-btn ctrl-btn-sm', { active: doubleValue === '1' }]" @click="doubleValue = '1'">{{ t('control.optOpen') }}</button>
            <button :class="['ctrl-btn ctrl-btn-sm', { active: doubleValue === '2' }]" @click="doubleValue = '2'">{{ t('control.optClose') }}</button>
            <button :class="['ctrl-btn ctrl-btn-sm', { active: doubleValue === '3' }]" @click="doubleValue = '3'">{{ t('control.optInvalid') }}</button>
          </div>

          <!-- Step: up/down -->
          <div v-else-if="commandType === 'step'" class="ctrl-buttons">
            <button :class="['ctrl-btn', { active: stepValue === '1' }]" @click="stepValue = '1'">&#9660; {{ t('control.optStepDown') }}</button>
            <button :class="['ctrl-btn', { active: stepValue === '2' }]" @click="stepValue = '2'">&#9650; {{ t('control.optStepUp') }}</button>
          </div>

          <!-- Normalized: raw NVA integer (-32768 ~ 32767), same form as scaled -->
          <label v-else-if="commandType === 'setpoint_normalized'" class="form-label">
            {{ t('control.valueRangeNormalized') }}
            <input v-model="normalizedValue" class="form-input" type="number" min="-32768" max="32767" step="1" />
          </label>

          <!-- Scaled: integer input -->
          <label v-else-if="commandType === 'setpoint_scaled'" class="form-label">
            {{ t('control.valueRangeScaled') }}
            <input v-model="scaledValue" class="form-input" type="number" min="-32768" max="32767" step="1" />
          </label>

          <!-- Float: number input -->
          <label v-else-if="commandType === 'setpoint_float'" class="form-label">
            {{ t('control.valueLabel') }}
            <input v-model="floatValue" class="form-input" type="number" step="0.1" />
          </label>

          <!-- Bitstring: 32-bit unsigned -->
          <div v-else-if="commandType === 'bitstring'" class="bitstring-control">
            <label class="form-label">
              {{ t('control.valueRangeBitstring') }}
              <input v-model.number="bitstringValue" class="form-input" type="number" min="0" :max="0xFFFFFFFF" step="1" />
            </label>
            <div class="bitstring-hex">{{ t('control.bitstringHex') }}: 0x{{ ((bitstringValue >>> 0).toString(16).toUpperCase().padStart(8, '0')) }}</div>
          </div>

          <div class="toggle-row">
            <label class="toggle-label" :class="{ 'is-disabled': isBitstring }">
              <input type="checkbox" v-model="selectMode" class="toggle-checkbox" :disabled="isBitstring" />
              <span>{{ t('control.sboLabel') }}</span>
            </label>
            <span class="toggle-hint">{{ isBitstring ? t('control.bitstringNoSbo') : (selectMode ? t('control.sboTwoStep') : t('control.sboDirect')) }}</span>
          </div>

          <details class="advanced" :open="showAdvanced" @toggle="showAdvanced = ($event.target as HTMLDetailsElement).open">
            <summary class="advanced-summary">{{ t('control.advancedSummary') }}</summary>
            <div class="advanced-body">
              <label class="form-label">
                {{ isBitstring ? t('control.qulqlIgnored') : t('control.qulqlLabel') }}
                <input v-model.number="qualifier" class="form-input" type="number" min="0" max="127" :disabled="isBitstring" />
                <span class="hint">{{ qualifierHint }}</span>
              </label>
              <label class="form-label">
                {{ t('control.cotLabel') }}
                <div class="cot-row">
                  <input v-model.number="cot" class="form-input cot-input" type="number" min="0" max="63" />
                  <select class="form-input cot-preset" @change="cot = Number(($event.target as HTMLSelectElement).value)">
                    <option value="6">{{ t('control.cot6') }}</option>
                    <option value="7">{{ t('control.cot7') }}</option>
                    <option value="8">{{ t('control.cot8') }}</option>
                    <option value="9">{{ t('control.cot9') }}</option>
                    <option value="10">{{ t('control.cot10') }}</option>
                  </select>
                </div>
              </label>
            </div>
          </details>

          <div v-if="errorMsg" class="error-msg">{{ errorMsg }}</div>

          <div v-if="lastResult" class="result-indicator result-ok">
            <span class="result-steps">
              <span v-for="(step, i) in lastResult.steps" :key="i" class="step-dot" :title="step.action">&#9679;</span>
            </span>
            <span class="result-text">OK {{ lastResult.duration_ms }}ms</span>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="emit('close')">{{ t('common.close') }}</button>
          <button class="btn btn-primary" :disabled="sending" @click="send">
            {{ sending ? t('control.sending') : t('control.send') }}
          </button>
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
  background: rgba(0, 0, 0, 0.55);
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
  min-width: 400px;
  max-width: 90vw;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
  margin-bottom: 16px;
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}

.form-label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--c-overlay0);
}

.form-row {
  display: flex;
  gap: 8px;
}

.form-label-half {
  flex: 1;
}

.bitstring-control {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.bitstring-hex {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-blue);
  padding-left: 2px;
}

.advanced {
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  padding: 6px 10px;
}

.advanced-summary {
  cursor: pointer;
  font-size: 12px;
  color: var(--c-text);
  user-select: none;
}

.advanced-summary:hover {
  color: var(--c-blue);
}

.advanced-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-top: 10px;
}

.hint {
  font-size: 10px;
  color: var(--c-overlay0);
  line-height: 1.4;
}

.cot-row {
  display: flex;
  gap: 6px;
}

.cot-input {
  width: 80px;
}

.cot-preset {
  flex: 1;
}

.toggle-label.is-disabled {
  opacity: 0.5;
  cursor: not-allowed;
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

.ctrl-buttons {
  display: flex;
  gap: 6px;
}

.ctrl-btn {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  background: var(--c-surface0);
  color: var(--c-text);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.ctrl-btn:hover {
  background: var(--c-surface1);
}

.ctrl-btn.active {
  background: var(--c-blue);
  color: var(--c-base);
  border-color: var(--c-blue);
  font-weight: 600;
}

.ctrl-btn-sm {
  padding: 6px 8px;
  font-size: 11px;
}

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 0;
}

.toggle-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--c-text);
  cursor: pointer;
}

.toggle-checkbox {
  accent-color: var(--c-blue);
}

.toggle-hint {
  font-size: 10px;
  color: var(--c-overlay0);
}

.error-msg {
  padding: 8px 10px;
  background: rgba(243, 139, 168, 0.15);
  border: 1px solid var(--c-red);
  border-radius: 4px;
  color: var(--c-red);
  font-size: 12px;
  word-break: break-word;
}

.result-indicator {
  padding: 6px 8px;
  border-radius: 4px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 8px;
}

.result-ok {
  background: rgba(166, 227, 161, 0.15);
  border: 1px solid rgba(166, 227, 161, 0.3);
  color: var(--c-green);
}

.result-steps {
  display: flex;
  gap: 4px;
  font-size: 8px;
}

.step-dot {
  color: var(--c-green);
}

.result-text {
  font-family: var(--font-mono);
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
  font-weight: 600;
}

.btn-primary:hover {
  background: var(--c-sapphire);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: default;
}

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}

.btn-secondary:hover {
  background: var(--c-surface2);
}
</style>
