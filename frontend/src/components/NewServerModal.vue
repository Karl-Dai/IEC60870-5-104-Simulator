<script setup lang="ts">
import { ref, inject, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import AppButton from '@shared/components/ui/AppButton.vue'
import AppCheckbox from '@shared/components/ui/AppCheckbox.vue'
import AppInput from '@shared/components/ui/AppInput.vue'
import FilePathInput from '@shared/components/FilePathInput.vue'
import { formatStartServerError } from '../errors'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!
const refreshTree = inject<() => void>('refreshTree')!

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void }>()

const bindAddress = ref('0.0.0.0')
const bindSuggestions = ref<string[]>(['0.0.0.0', '127.0.0.1'])
const port = ref('2404')
const commonAddress = ref(1)
const stationName = ref('')
const initMode = ref('zero')
// issue #28:默认 0 = 空配置。旧默认 10 会给全部监视类型预填 IOA 1..10,
// 制造大量同 CASDU 跨类型重复 IOA;实际站点应按需批量创建。
const count = ref(0)
const useTls = ref(false)
const certFile = ref('')
const keyFile = ref('')
const caFile = ref('')
const requireClientCert = ref(false)
const pending = ref(false)
const errorText = ref('')

function reset() {
  bindAddress.value = '0.0.0.0'
  port.value = '2404'
  commonAddress.value = 1
  stationName.value = ''
  initMode.value = 'zero'
  count.value = 0
  useTls.value = false
  certFile.value = ''
  keyFile.value = ''
  caFile.value = ''
  requireClientCert.value = false
  errorText.value = ''
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

function close() { if (!pending.value) emit('update:visible', false) }

async function submit() {
  if (pending.value) return
  const p = Number(port.value)
  if (!Number.isInteger(p) || p < 1 || p > 65535) {
    await showAlert(t('errors.invalidPort'))
    return
  }
  const ca = Number(commonAddress.value)
  if (!Number.isInteger(ca) || ca < 1 || ca > 65534) {
    await showAlert(t('errors.invalidCa'))
    return
  }
  pending.value = true
  errorText.value = ''
  try {
    const c = Number.isFinite(count.value) && count.value >= 0
      ? Math.min(65534, Math.floor(count.value))
      : 0
    await invoke('create_and_start_server', {
      request: {
        bind_address: bindAddress.value.trim() || undefined,
        port: p,
        common_address: ca,
        station_name: stationName.value.trim(),
        init_mode: initMode.value,
        count_per_category: c,
        use_tls: useTls.value || undefined,
        cert_file: certFile.value || undefined,
        key_file: keyFile.value || undefined,
        ca_file: caFile.value || undefined,
        require_client_cert: requireClientCert.value || undefined,
      },
    })
    refreshTree()
    emit('update:visible', false)
  } catch (e) {
    errorText.value = formatStartServerError(e, t)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
    <div v-if="visible" class="modal-overlay dialog-blur">
      <div class="modal-box" role="dialog" aria-modal="true" aria-labelledby="new-server-title" :aria-busy="pending">
        <div id="new-server-title" class="modal-title">{{ t('newServer.title') }}</div>
        <fieldset class="modal-body" :disabled="pending">
          <div class="modal-field">
            <label for="new-server-bind">{{ t('newServer.bindAddressLabel') }}</label>
            <AppInput
              id="new-server-bind"
              v-model="bindAddress"
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
            <label for="new-server-port">{{ t('newServer.portLabel') }}</label>
            <input id="new-server-port" v-model="port" type="number" min="1" max="65535" @keyup.enter="submit" />
          </div>
          <div class="modal-field">
            <label for="new-server-ca">{{ t('newServer.commonAddressLabel') }}</label>
            <input id="new-server-ca" v-model.number="commonAddress" type="number" min="1" max="65534" step="1" @keyup.enter="submit" />
          </div>
          <div class="modal-field">
            <label for="new-server-station">{{ t('newServer.stationNameLabel') }}</label>
            <AppInput
              id="new-server-station"
              v-model="stationName"
              :placeholder="t('newServer.stationNamePlaceholder')"
              @keyup.enter="submit"
            />
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
            <label for="new-server-count">{{ t('newServer.countPerCategory') }}</label>
            <input id="new-server-count" v-model.number="count" type="number" min="0" max="65534" @keyup.enter="submit" />
            <div class="field-hint">{{ t('newServer.countHint') }}</div>
          </div>
          <div class="modal-field">
            <AppCheckbox v-model="useTls" class="checkbox-label">
              {{ t('newServer.enableTls') }}
            </AppCheckbox>
          </div>
          <template v-if="useTls">
            <FilePathInput
              v-model="certFile"
              class="modal-field"
              :label="t('newServer.serverCert')"
              placeholder="/path/to/server.crt"
              kind="certificate"
            />
            <FilePathInput
              v-model="keyFile"
              class="modal-field"
              :label="t('newServer.serverKey')"
              placeholder="/path/to/server.key"
              kind="private-key"
            />
            <FilePathInput
              v-model="caFile"
              class="modal-field"
              :label="requireClientCert ? t('serverSettings.caRequiredLabel') : t('newServer.caFile')"
              placeholder="/path/to/ca.crt"
              kind="certificate"
            />
            <div class="modal-field">
              <AppCheckbox v-model="requireClientCert" class="checkbox-label">
                {{ t('newServer.requireClientCert') }}
              </AppCheckbox>
            </div>
          </template>
        </fieldset>
        <div v-if="errorText" class="submit-error" role="alert">
          <strong>{{ t('newServer.failureKept') }}</strong>
          <p>{{ errorText }}</p>
        </div>
        <div class="modal-actions">
          <AppButton class="modal-btn cancel" :disabled="pending" @click="close">{{ t('common.cancel') }}</AppButton>
          <AppButton variant="primary" class="modal-btn confirm" :disabled="pending" @click="submit">{{ pending ? t('newServer.creating') : errorText ? t('newServer.retry') : t('newServer.createAndStart') }}</AppButton>
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
  border: 0;
  padding: 0;
  margin: 0;
  min-width: 0;
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}
.submit-error { margin-top: 12px; padding: 10px; color: var(--danger); background: color-mix(in srgb, var(--danger) 10%, transparent); border-radius: 5px; font-size: 12px; max-height: 150px; overflow-y: auto; }
.submit-error p { margin: 6px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.modal-btn:disabled { cursor: wait; }
.modal-field { margin-bottom: 14px; }
.modal-field label {
  display: block;
  font-size: 12px;
  color: var(--c-subtext0);
  margin-bottom: 6px;
}
/* 数字输入仍是原生控件,样式对齐 AppInput */
.modal-field input[type="number"] {
  width: 100%;
  padding: 6px var(--space-2);
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-family: inherit;
  font-size: var(--text-md);
  outline: none;
  box-sizing: border-box;
}
.modal-field input[type="number"]:focus {
  border-color: var(--accent);
}
.field-hint {
  margin-top: 4px;
  font-size: 11px;
  color: var(--c-overlay0);
  line-height: 1.4;
}
.radio-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--c-text);
  cursor: pointer;
}

.radio-label input[type="radio"] {
  accent-color: var(--accent);
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
</style>
