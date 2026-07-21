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

const port = ref('2404')
const bindAddress = ref('0.0.0.0')
// 本机可选监听地址(0.0.0.0 / 回环 / 各网卡 IPv4),弹窗打开时从后端枚举;
// 输入框配 datalist,既可下拉选择也可手动输入。
const bindOptions = ref<string[]>(['0.0.0.0', '127.0.0.1'])
const initMode = ref('zero')
const count = ref(10)
const useTls = ref(false)
const certFile = ref('')
const keyFile = ref('')
const caFile = ref('')
const requireClientCert = ref(false)

function reset() {
  port.value = '2404'
  bindAddress.value = '0.0.0.0'
  initMode.value = 'zero'
  count.value = 10
  useTls.value = false
  certFile.value = ''
  keyFile.value = ''
  caFile.value = ''
  requireClientCert.value = false
}

async function loadBindOptions() {
  try {
    bindOptions.value = await invoke<string[]>('list_bind_addresses')
  } catch {
    // 枚举失败时保留默认(通配 + 回环),不阻塞创建流程
  }
}

watch(() => props.visible, (v) => {
  if (v) {
    reset()
    loadBindOptions()
  }
})

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
        <div class="modal-field">
          <label>{{ t('newServer.bindAddress') }}</label>
          <input v-model="bindAddress" type="text" list="bind-addr-options" placeholder="0.0.0.0" @keyup.enter="submit" />
          <datalist id="bind-addr-options">
            <option v-for="addr in bindOptions" :key="addr" :value="addr" />
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
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}
.modal-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--c-text);
  margin-bottom: 16px;
}
.modal-field { margin-bottom: 14px; }
.modal-field label {
  display: block;
  font-size: 12px;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}
.field-hint {
  margin-top: 4px;
  font-size: 11px;
  color: var(--c-overlay0);
  line-height: 1.4;
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
