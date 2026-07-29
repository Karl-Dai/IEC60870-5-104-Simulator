<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { RawSendResult } from '../types'
import { useI18n } from '@shared/i18n'

interface Props {
  visible: boolean
  connectionId: string | null
}

const props = defineProps<Props>()
const emit = defineEmits<{ (e: 'close'): void }>()
const { t } = useI18n()

type PreviewState =
  | { kind: 'invalidChars' }
  | { kind: 'empty' }
  | { kind: 'oddDigits'; digits: number }
  | { kind: 'invalidApdu'; bytes: number; firstByte: string }
  | {
      kind: 'frame'
      frameType: 'i' | 's' | 'u'
      declared: number
      expected: number
      actual: number
      valid: boolean
    }

const hexInput = ref('')
const sending = ref(false)
const errorMsg = ref('')
const lastResult = ref<RawSendResult | null>(null)
const previewState = ref<PreviewState | null>(null)

const templates = computed(() => [
  { label: 'STARTDT act', hex: '68 04 07 00 00 00' },
  { label: 'STARTDT con', hex: '68 04 0B 00 00 00' },
  { label: 'STOPDT act',  hex: '68 04 13 00 00 00' },
  { label: 'TESTFR act',  hex: '68 04 43 00 00 00' },
  { label: 'TESTFR con',  hex: '68 04 83 00 00 00' },
  { label: 'S-frame (RSN=0)', hex: '68 04 01 00 00 00' },
  {
    label: t('rawSend.templateGeneralInterrogationAct'),
    hex: '68 0E 00 00 00 00 64 01 06 00 01 00 00 00 00 14',
  },
])

const previewMsg = computed(() => {
  const state = previewState.value
  if (!state) return ''
  switch (state.kind) {
    case 'invalidChars':
      return t('rawSend.invalidChars')
    case 'empty':
      return t('rawSend.empty')
    case 'oddDigits':
      return t('rawSend.oddDigits', { n: state.digits })
    case 'invalidApdu':
      return t('rawSend.invalidApdu', {
        bytes: state.bytes,
        firstByte: state.firstByte,
      })
    case 'frame':
      return t('rawSend.lengthSummary', {
        frameType: t(`rawSend.frame${state.frameType.toUpperCase()}`),
        declared: state.declared,
        expected: state.expected,
        actual: state.actual,
        status: state.valid ? '✓' : '✗',
      })
  }
})

watch(() => props.visible, (v) => {
  if (v) {
    errorMsg.value = ''
    lastResult.value = null
    previewState.value = null
  }
})

function applyTemplate(hex: string) {
  hexInput.value = hex
  errorMsg.value = ''
  preview()
}

const compactHex = computed(() => {
  let out = ''
  for (const c of hexInput.value) {
    if (/[0-9a-fA-F]/.test(c)) out += c
    else if (/\s|,|-|:/.test(c)) continue
    else return null
  }
  return out
})

function preview() {
  const h = compactHex.value
  if (h === null) {
    previewState.value = { kind: 'invalidChars' }
    return
  }
  if (h.length === 0) {
    previewState.value = { kind: 'empty' }
    return
  }
  if (h.length % 2 !== 0) {
    previewState.value = { kind: 'oddDigits', digits: h.length }
    return
  }
  const bytes: number[] = []
  for (let i = 0; i < h.length; i += 2) {
    bytes.push(parseInt(h.slice(i, i + 2), 16))
  }
  if (bytes.length < 6 || bytes[0] !== 0x68) {
    previewState.value = {
      kind: 'invalidApdu',
      bytes: bytes.length,
      firstByte: bytes[0]?.toString(16).toUpperCase().padStart(2, '0') ?? '??',
    }
    return
  }
  const declared = bytes[1]
  const expected = declared + 2
  const ctrl1 = bytes[2]
  let frameType: 'i' | 's' | 'u' = 'i'
  if ((ctrl1 & 0x03) === 0x03) frameType = 'u'
  else if ((ctrl1 & 0x03) === 0x01) frameType = 's'
  const lenOk = expected === bytes.length
  previewState.value = {
    kind: 'frame',
    frameType,
    declared,
    expected,
    actual: bytes.length,
    valid: lenOk,
  }
}

async function send() {
  if (!props.connectionId) {
    errorMsg.value = t('rawSend.noConnection')
    return
  }
  errorMsg.value = ''
  lastResult.value = null
  sending.value = true
  try {
    const result = await invoke<RawSendResult>('send_raw_apdu', {
      request: {
        connection_id: props.connectionId,
        hex_payload: hexInput.value,
      }
    })
    lastResult.value = result
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    sending.value = false
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-backdrop dialog-blur" @mousedown.self="emit('close')" @keydown="handleKeydown">
      <div class="modal-box">
        <div class="modal-title">{{ t('rawSend.title') }}</div>
        <div class="modal-body">
          <div class="hint">{{ t('rawSend.hint') }}</div>

          <label class="form-label">
            {{ t('rawSend.hexLabel') }}
            <textarea v-model="hexInput" @input="preview" class="hex-area" rows="4"
              placeholder="68 04 07 00 00 00" spellcheck="false"></textarea>
          </label>

          <div class="preview-row">
            <button class="btn btn-secondary btn-sm" type="button" @click="preview">{{ t('rawSend.preview') }}</button>
            <span class="preview-msg">{{ previewMsg || '—' }}</span>
          </div>

          <div class="templates">
            <span class="templates-label">{{ t('rawSend.templatesLabel') }}</span>
            <button v-for="template in templates" :key="template.label" type="button"
              class="template-btn" @click="applyTemplate(template.hex)">{{ template.label }}</button>
          </div>

          <div v-if="errorMsg" class="error-msg">{{ errorMsg }}</div>
          <div v-if="lastResult" class="result-ok">
            <div class="result-line"><span class="k">{{ t('rawSend.sent') }}</span><span class="v">{{ t('rawSend.byteCount', { n: lastResult.byte_len, timestamp: lastResult.timestamp }) }}</span></div>
            <div class="result-bytes">{{ lastResult.sent_hex }}</div>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" @click="emit('close')">{{ t('common.close') }}</button>
          <button class="btn btn-primary" :disabled="sending || !connectionId" @click="send">
            {{ sending ? t('rawSend.sending') : t('rawSend.send') }}
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
  min-width: 480px;
  max-width: 90vw;
  /* issue #28:模板按钮换行 + 长报文回显时对话框很高,小视口下标题/按钮必须常驻,
     滚动收进 .modal-body —— 之前连 max-height 都没有,超高内容直接被裁。 */
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-title {
  flex-shrink: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
  margin-bottom: 16px;
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}

.modal-footer {
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}

.hint {
  font-size: 11px;
  color: var(--c-overlay0);
  line-height: 1.5;
}

.form-label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--c-overlay0);
}

.hex-area {
  padding: 8px 10px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-family: var(--font-mono);
  font-size: 12px;
  resize: vertical;
}

.hex-area:focus {
  outline: none;
  border-color: var(--c-blue);
}

.preview-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.preview-msg {
  font-size: 11px;
  color: var(--c-text);
  font-family: var(--font-mono);
}

.templates {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.templates-label {
  font-size: 11px;
  color: var(--c-overlay0);
}

.template-btn {
  padding: 3px 8px;
  font-size: 11px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  color: var(--c-text);
  border-radius: 4px;
  cursor: pointer;
  font-family: var(--font-mono);
}

.template-btn:hover {
  background: var(--c-surface1);
  border-color: var(--c-blue);
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

.result-ok {
  padding: 8px 10px;
  background: rgba(166, 227, 161, 0.12);
  border: 1px solid rgba(166, 227, 161, 0.35);
  border-radius: 4px;
  color: var(--c-green);
  font-size: 11px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.result-line .k { color: var(--c-overlay0); margin-right: 6px; }
.result-line .v { font-family: var(--font-mono); }
.result-bytes {
  font-family: var(--font-mono);
  word-break: break-all;
  color: var(--c-text);
}

.btn {
  padding: 7px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
}

.btn-sm {
  padding: 4px 10px;
  font-size: 11px;
}

.btn-primary {
  background: var(--c-blue);
  color: var(--c-base);
  font-weight: 600;
}

.btn-primary:hover:not(:disabled) { background: var(--c-sapphire); }
.btn-primary:disabled { opacity: 0.5; cursor: default; }

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}

.btn-secondary:hover { background: var(--c-surface2); }
</style>
