<script setup lang="ts">
import { ref, inject, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { formatStartServerError } from '../errors'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const refreshTree = inject<() => void>('refreshTree')!

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void }>()

const bindAddress = ref('0.0.0.0')
const bindSuggestions = ref<string[]>(['0.0.0.0', '127.0.0.1'])
const port = ref('2404')
const initMode = ref('zero')
const count = ref(10)
const useTls = ref(false)
const certFile = ref('')
const keyFile = ref('')
const caFile = ref('')
const requireClientCert = ref(false)

function reset() {
  bindAddress.value = '0.0.0.0'
  port.value = '2404'
  initMode.value = 'zero'
  count.value = 10
  useTls.value = false
  certFile.value = ''
  keyFile.value = ''
  caFile.value = ''
  requireClientCert.value = false
}

// 监听地址建议:0.0.0.0 / 127.0.0.1 / 各网卡 IPv4 地址(issue #28)。
// 输入框仍可自由填任意网卡地址。
async function loadBindSuggestions() {
  try {
    bindSuggestions.value = await invoke<string[]>('list_bind_address_suggestions')
  } catch {
    bindSuggestions.value = ['0.0.0.0', '127.0.0.1']
  }
}

watch(() => props.visible, (v) => { if (v) { reset(); loadBindSuggestions() } })

function close() { emit('update:visible', false) }

async function submit() {
  const p = Number(port.value)
  if (!p || p < 1 || p > 65535) {
    await showAlert(t('errors.invalidPort'))
    return
  }
  close()
  try {
    const c = Number.isFinite(count.value) && count.value >= 0
      ? Math.min(65534, Math.floor(count.value))
      : 10
    const info = await invoke<{ id: string }>('create_server', {
      request: {
        bind_address: bindAddress.value.trim() || undefined,
        port: p,
        init_mode: initMode.value,
        count_per_category: c,
        use_tls: useTls.value || undefined,
        cert_file: certFile.value || undefined,
        key_file: keyFile.value || undefined,
        ca_file: caFile.value || undefined,
        require_client_cert: requireClientCert.value || undefined,
      },
    })
    await invoke('start_server', { id: info.id })
    refreshTree()
  } catch (e) {
    await showAlert(formatStartServerError(e, t))
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-overlay dialog-blur" @mousedown.self="close">
      <div class="modal-box">
        <div class="modal-title">{{ t('newServer.title') }}</div>
        <div class="modal-body">
          <div class="modal-field">
            <label>{{ t('newServer.bindAddressLabel') }}</label>
            <input
              v-model="bindAddress"
              type="text"
              list="bind-address-suggestions"
              placeholder="0.0.0.0"
              @keyup.enter="submit"
            />
            <datalist id="bind-address-suggestions">
              <option v-for="addr in bindSuggestions" :key="addr" :value="addr" />
            </datalist>
            <div class="field-hint">{{ t('newServer.bindAddressHint') }}</div>
          </div>
          <div class="modal-field">
            <label>{{ t('newServer.portLabel') }}</label>
            <input v-model="port" type="number" min="1" max="65535" @keyup.enter="submit" />
          </div>
          <div class="modal-field">
            <label>{{ t('newServer.initMode') }}</label>
            <div class="radio-group">
              <label class="radio-label">
                <input type="radio" v-model="initMode" value="zero" /> {{ t('newServer.initZero') }}
              </label>
              <label class="radio-label">
                <input type="radio" v-model="initMode" value="random" /> {{ t('newServer.initRandom') }}
              </label>
            </div>
          </div>
          <div class="modal-field">
            <label>{{ t('newServer.countPerCategory') }}</label>
            <input v-model.number="count" type="number" min="0" max="65534" @keyup.enter="submit" />
          </div>
          <div class="modal-field">
            <label class="checkbox-label">
              <input type="checkbox" v-model="useTls" /> {{ t('newServer.enableTls') }}
            </label>
          </div>
          <template v-if="useTls">
            <div class="modal-field">
              <label>{{ t('newServer.serverCert') }}</label>
              <input v-model="certFile" type="text" placeholder="/path/to/server.crt" />
            </div>
            <div class="modal-field">
              <label>{{ t('newServer.serverKey') }}</label>
              <input v-model="keyFile" type="text" placeholder="/path/to/server.key" />
            </div>
            <div class="modal-field">
              <label>{{ t('newServer.caFile') }}</label>
              <input v-model="caFile" type="text" placeholder="/path/to/ca.crt" />
            </div>
            <div class="modal-field">
              <label class="checkbox-label">
                <input type="checkbox" v-model="requireClientCert" /> {{ t('newServer.requireClientCert') }}
              </label>
            </div>
          </template>
        </div>
        <div class="modal-actions">
          <button class="modal-btn cancel" @click="close">{{ t('common.cancel') }}</button>
          <button class="modal-btn confirm" @click="submit">{{ t('common.ok') }}</button>
        </div>
      </div>
    </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal-overlay {
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
  min-width: 300px;
  max-width: 90vw;
  /* issue #28:勾选 TLS 后展开到 9 个字段(约 650px),小视口下标题/按钮必须常驻,
     滚动收进 .modal-body —— 之前连 max-height 都没有,超高内容直接被裁且滚不到。 */
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.modal-title {
  flex-shrink: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--c-text);
  margin-bottom: 16px;
}
.modal-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}
.modal-field { margin-bottom: 14px; }
.modal-field label {
  display: block;
  font-size: 12px;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}
.modal-field input[type="number"],
.modal-field input[type="text"] {
  width: 100%;
  padding: 6px 10px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 13px;
  outline: none;
  box-sizing: border-box;
}
.modal-field input[type="number"]:focus,
.modal-field input[type="text"]:focus {
  border-color: var(--c-blue);
}
.field-hint {
  margin-top: 4px;
  font-size: 11px;
  color: var(--c-overlay0);
  line-height: 1.4;
}
.checkbox-label,
.radio-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--c-text);
  cursor: pointer;
}
.checkbox-label input[type="checkbox"],
.radio-label input[type="radio"] {
  accent-color: var(--c-blue);
}
.radio-group {
  display: flex;
  gap: 16px;
}
.modal-actions {
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
}
.modal-btn {
  padding: 6px 16px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}
.modal-btn.cancel {
  background: var(--c-surface0);
  color: var(--c-subtext0);
}
.modal-btn.cancel:hover { background: var(--c-surface1); }
.modal-btn.confirm {
  background: var(--c-blue);
  color: var(--c-base);
  font-weight: 600;
}
.modal-btn.confirm:hover { background: var(--c-sapphire); }
</style>
