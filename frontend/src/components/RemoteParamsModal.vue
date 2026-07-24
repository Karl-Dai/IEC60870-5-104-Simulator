<script setup lang="ts">
import { ref, reactive, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { useRemoteParams } from '../composables/useRemoteParams'
import RemoteParamsForm from './RemoteParamsForm.vue'
import type { ServerInfo } from '../types'

const { t } = useI18n()

interface Props {
  visible: boolean
  serverId: string | null
  serverLabel?: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  saved: []
}>()

// 独立的 serverId ref —— 不污染 App 的全局 selectedServerId
const localServerId = ref<string | null>(props.serverId)
watch(() => props.serverId, v => { localServerId.value = v })

const { timing, ops, loading, lastError, load, applyTiming, applyOps } =
  useRemoteParams(localServerId)

// —— 连接参数(监听地址 / 端口)——
// 传输配置原本仅创建时可设;这里允许停止状态下直接改端口,免去删除重建。
const transport = reactive({ bindAddress: '', port: 0 })
const serverState = ref('')
const isRunning = computed(() => serverState.value === 'Running')
let transportBaseline = ''

async function loadTransport() {
  const id = props.serverId
  if (!id) return
  try {
    const servers = await invoke<ServerInfo[]>('list_servers')
    const s = servers.find(x => x.id === id)
    if (s) {
      transport.bindAddress = s.bind_address
      transport.port = s.port
      serverState.value = s.state
      transportBaseline = `${s.bind_address}:${s.port}`
    }
  } catch (e) {
    lastError.value = String(e)
  }
}

const isSaving = ref(false)

async function handleSave() {
  if (!localServerId.value) return
  isSaving.value = true
  lastError.value = null
  try {
    // 先落地传输配置改动(仅当确有改动)。运行中由后端拒绝,前端也提前拦一次。
    const changed = `${transport.bindAddress}:${transport.port}` !== transportBaseline
    if (changed) {
      if (isRunning.value) {
        lastError.value = t('remoteParams.stopBeforeEdit')
        return
      }
      try {
        await invoke('update_server_transport', {
          request: {
            server_id: localServerId.value,
            bind_address: transport.bindAddress,
            port: transport.port,
          },
        })
        transportBaseline = `${transport.bindAddress}:${transport.port}`
      } catch (e) {
        lastError.value = String(e)
        return
      }
    }
    await applyTiming()
    if (lastError.value) return
    await applyOps()
    if (lastError.value) return
    emit('saved')
    emit('close')
  } finally {
    isSaving.value = false
  }
}

function handleBackdropClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-backdrop')) {
    emit('close')
  }
}

function handleEsc(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.visible) emit('close')
}

watch(() => props.visible, (v) => {
  if (v) {
    loadTransport()
    // 同一服务器二次打开时 localServerId 不变,composable 的 watch 不会重载;
    // 期间参数可能已被抽屉(RemoteParamsDrawer)改过,强制回读后端(issue #28)。
    load()
    window.addEventListener('keydown', handleEsc)
  } else {
    window.removeEventListener('keydown', handleEsc)
    isSaving.value = false
  }
})
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div v-if="visible" class="modal-backdrop dialog-blur" @click="handleBackdropClick">
        <div class="modal">
          <div class="modal-header">
            <span class="modal-title">
              {{ t('runtimeParams.title') }}
              <span v-if="serverLabel" class="modal-subtitle">— {{ serverLabel }}</span>
            </span>
            <button class="btn-close" @click="emit('close')">×</button>
          </div>

          <div class="modal-body">
            <section class="rp-conn">
              <header class="rp-conn-head">
                <h4>{{ t('remoteParams.connParams') }}</h4>
                <span class="rp-conn-sub">{{ t('remoteParams.connParamsSub') }}</span>
              </header>
              <div class="rp-conn-grid">
                <label class="rp-conn-field">
                  <span>{{ t('remoteParams.bindAddress') }}</span>
                  <input v-model="transport.bindAddress" :disabled="isRunning" placeholder="0.0.0.0" />
                </label>
                <label class="rp-conn-field">
                  <span>{{ t('remoteParams.port') }}</span>
                  <input type="number" min="1" max="65535" v-model.number="transport.port" :disabled="isRunning" />
                </label>
              </div>
              <p v-if="isRunning" class="rp-conn-hint">{{ t('remoteParams.runningHint') }}</p>
            </section>

            <div v-if="loading" class="muted">{{ t('runtimeParams.loading') }}</div>
            <RemoteParamsForm v-else :timing="timing" :ops="ops" />
            <p v-if="lastError" class="error">{{ lastError }}</p>
          </div>

          <div class="modal-footer">
            <button class="btn btn-secondary" @click="emit('close')" :disabled="isSaving">
              {{ t('runtimeParams.cancel') }}
            </button>
            <button class="btn btn-primary" @click="handleSave" :disabled="isSaving || loading">
              {{ isSaving ? t('runtimeParams.saving') : t('runtimeParams.save') }}
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
  z-index: 2000;
}

.modal {
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  width: 520px;
  max-width: 92vw;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 14px 18px;
  border-bottom: 1px solid var(--c-surface0);
}

.modal-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--c-text);
}

.modal-subtitle {
  font-weight: 400;
  font-size: 13px;
  color: var(--c-subtext0);
  margin-left: 6px;
}

.btn-close {
  background: none;
  border: none;
  color: var(--c-overlay0);
  font-size: 20px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
}

.btn-close:hover {
  color: var(--c-text);
}

.modal-body {
  padding: 14px 18px;
  overflow-y: auto;
  color: var(--c-text);
  font-size: 12px;
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 18px;
  border-top: 1px solid var(--c-surface0);
}

.btn {
  padding: 7px 18px;
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

.btn-primary:hover { background: var(--c-sapphire); }
.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

.btn-secondary {
  background: var(--c-surface1);
  color: var(--c-text);
}

.btn-secondary:hover { background: var(--c-surface2); }
.btn-secondary:disabled { opacity: 0.5; cursor: not-allowed; }

.muted { color: var(--c-subtext0); font-size: 12px; }

/* —— 连接参数(地址 / 端口)—— */
.rp-conn {
  padding-bottom: 12px;
  margin-bottom: 4px;
  border-bottom: 1px solid var(--c-surface0);
}
.rp-conn-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 10px;
}
.rp-conn-head h4 {
  margin: 0;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--c-text);
}
.rp-conn-sub {
  font-size: 11px;
  color: var(--c-overlay0);
}
.rp-conn-sub::before {
  content: "·";
  margin-right: 6px;
  color: var(--c-surface2);
}
.rp-conn-grid {
  display: grid;
  grid-template-columns: 1fr 120px;
  gap: 10px;
}
.rp-conn-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.rp-conn-field > span {
  font-size: 11px;
  color: var(--c-subtext0);
}
.rp-conn-field input {
  padding: 6px 8px;
  background: var(--c-mantle);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
  font-family: var(--font-mono);
}
.rp-conn-field input:focus {
  outline: none;
  border-color: var(--c-blue);
}
.rp-conn-field input:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.rp-conn-hint {
  margin: 8px 0 0;
  font-size: 11px;
  color: var(--c-peach, var(--c-subtext0));
}

.error {
  margin-top: 10px;
  padding: 6px 8px;
  font-size: 11.5px;
  color: var(--c-red);
  background: color-mix(in srgb, var(--c-red) 12%, transparent);
  border-left: 2px solid var(--c-red);
  border-radius: 3px;
}

</style>
