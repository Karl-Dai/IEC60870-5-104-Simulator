<script setup lang="ts">
import { ref, reactive, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import { useRemoteParams } from '../composables/useRemoteParams'
import RemoteParamsForm from './RemoteParamsForm.vue'
import type { ProtocolTimingConfig, RemoteOperationConfig, ServerInfo } from '../types'

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

function cloneTiming(value: ProtocolTimingConfig): ProtocolTimingConfig {
  return { ...value }
}

function cloneOps(value: RemoteOperationConfig): RemoteOperationConfig {
  return JSON.parse(JSON.stringify(value))
}

// 连接参数(地址:端口)的已保存快照,空串表示"还没回读过" —— 见下方 transportDirty
const transportBaseline = ref('')
const transportLoading = ref(false)
const transportReady = ref(false)
const paramsReady = ref(false)
let transportLoadEpoch = 0
let modalSession = 0
const transportLoadError = ref<string | null>(null)
const saveError = ref<string | null>(null)

watch([() => props.visible, () => props.serverId], () => {
  // 同 serverId 重开也属于新会话。同步递增可保证父组件强制换目标后，
  // 旧 Promise 的 continuation 在任何时序下都无法写入新弹窗错误。
  modalSession++
  paramsReady.value = false
  transportReady.value = false
  transportLoadError.value = null
  saveError.value = null
}, { flush: 'sync' })

// 独立的 serverId ref —— 不污染 App 的全局 selectedServerId
const localServerId = ref<string | null>(props.serverId)
watch(() => props.serverId, v => {
  transportLoadEpoch++
  localServerId.value = v
  // 换服务器:上一台的传输快照失效,清空基线让 loadTransport 必定回读
  transportBaseline.value = ''
  if (props.visible) loadTransport()
})

const { timing, ops, loading, lastError, load, applyTiming, applyOps } =
  useRemoteParams(localServerId)
const hasLoadError = computed(() =>
  lastError.value !== null || transportLoadError.value !== null
)
const displayError = computed(() =>
  saveError.value ?? transportLoadError.value ?? lastError.value
)

watch(loading, (isLoading) => {
  if (isLoading) {
    paramsReady.value = false
  } else {
    // useRemoteParams 已用 serverId + load epoch 过滤迟到响应；只有当前可见
    // 会话的最新读取成功结束，参数快照才允许被保存。
    paramsReady.value = Boolean(
      props.visible
      && localServerId.value
      && !lastError.value
    )
  }
}, { immediate: true })

// —— 连接参数(监听地址 / 端口)——
// 传输配置原本仅创建时可设;这里允许停止状态下直接改端口,免去删除重建。
const transport = reactive({ bindAddress: '', port: 0 })
const serverState = ref('')
const isRunning = computed(() => serverState.value === 'Running')
const transportDirty = computed(() =>
  transportBaseline.value !== '' &&
  `${transport.bindAddress}:${transport.port}` !== transportBaseline.value
)

async function loadTransport() {
  const id = props.serverId
  const epoch = ++transportLoadEpoch
  const session = modalSession
  transportReady.value = false
  transportLoadError.value = null
  if (!id) {
    transportLoading.value = false
    return false
  }
  transportLoading.value = true
  try {
    const servers = await invoke<ServerInfo[]>('list_servers')
    const s = servers.find(x => x.id === id)
    if (
      epoch !== transportLoadEpoch
      || session !== modalSession
      || !props.visible
      || props.serverId !== id
    ) return false
    if (!s) {
      transportLoadError.value = `Server not found: ${id}`
      return false
    }
    {
      // 每次打开都取后端最新值。本弹窗是「取消 / 保存」语义(没有 Discard、
      // 也没有 dirty 指示),保留草稿会让「取消」不再取消 —— 用户放弃的端口改动
      // 会在下次保存时被静默写回后端(issue #28 审查)。抽屉那侧有显式 Discard,
      // 才按「关闭保留草稿」处理。
      serverState.value = s.state
      transport.bindAddress = s.bind_address
      transport.port = s.port
      transportBaseline.value = `${s.bind_address}:${s.port}`
    }
    transportReady.value = true
    return true
  } catch (e) {
    if (
      epoch === transportLoadEpoch
      && session === modalSession
      && props.visible
      && props.serverId === id
    ) {
      transportLoadError.value = String(e)
    }
    return false
  } finally {
    if (
      epoch === transportLoadEpoch
      && session === modalSession
      && props.serverId === id
    ) {
      transportLoading.value = false
    }
  }
}

const isSaving = ref(false)

async function handleSave() {
  if (
    !localServerId.value
    || isSaving.value
    || loading.value
    || transportLoading.value
    || !paramsReady.value
    || !transportReady.value
    || hasLoadError.value
  ) return
  const session = modalSession
  const serverId = localServerId.value
  const timingToSave = cloneTiming(timing.value)
  const opsToSave = cloneOps(ops.value)
  const transportToSave = {
    bindAddress: transport.bindAddress,
    port: transport.port,
  }
  const transportChanged = transportDirty.value
  isSaving.value = true
  saveError.value = null

  const reportSaveError = (error: string | null) => {
    if (
      error
      && session === modalSession
      && props.visible
      && localServerId.value === serverId
    ) {
      saveError.value = error
    }
  }

  try {
    // 先落地传输配置改动(仅当确有改动)。运行中由后端拒绝,前端也提前拦一次。
    if (transportChanged) {
      if (isRunning.value) {
        reportSaveError(t('remoteParams.stopBeforeEdit'))
        return
      }
      try {
        await invoke('update_server_transport', {
          request: {
            server_id: serverId,
            bind_address: transportToSave.bindAddress,
            port: transportToSave.port,
          },
        })
        if (session === modalSession && localServerId.value === serverId) {
          transportBaseline.value = `${transportToSave.bindAddress}:${transportToSave.port}`
        }
      } catch (e) {
        reportSaveError(String(e))
        return
      }
    }
    const timingResult = await applyTiming(serverId, timingToSave)
    if (!timingResult.ok) {
      reportSaveError(timingResult.error)
      return
    }
    const opsResult = await applyOps(serverId, opsToSave)
    if (!opsResult.ok) {
      reportSaveError(opsResult.error)
      return
    }
    // 父组件仍可能在保存期间强制隐藏/换目标。旧保存会话可以完成自己的落库，
    // 但绝不能关闭或刷新后来打开的另一台服务器弹窗。
    if (session !== modalSession || !props.visible || localServerId.value !== serverId) return
    emit('saved')
    emit('close')
  } finally {
    isSaving.value = false
  }
}

function close() {
  if (isSaving.value) return
  emit('close')
}

function handleBackdropClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-backdrop')) {
    close()
  }
}

function handleEsc(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.visible) close()
}

watch(() => props.visible, (v) => {
  if (v) {
    loadTransport()
    // 同一服务器二次打开时 localServerId 不变,composable 的 watch 不会重载;
    // 期间参数可能已被抽屉(RemoteParamsDrawer)改过,故每次打开都回读后端(issue #28)。
    load()
    window.addEventListener('keydown', handleEsc)
  } else {
    transportLoadEpoch++
    transportLoading.value = false
    window.removeEventListener('keydown', handleEsc)
  }
}, { immediate: true })
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
            <button class="btn-close" :disabled="isSaving" @click="close">×</button>
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
                  <input v-model="transport.bindAddress" :disabled="isRunning || isSaving || transportLoading" placeholder="0.0.0.0" />
                </label>
                <label class="rp-conn-field">
                  <span>{{ t('remoteParams.port') }}</span>
                  <input type="number" min="1" max="65535" v-model.number="transport.port" :disabled="isRunning || isSaving || transportLoading" />
                </label>
              </div>
              <p v-if="isRunning" class="rp-conn-hint">{{ t('remoteParams.runningHint') }}</p>
            </section>

            <div v-if="loading" class="muted">{{ t('runtimeParams.loading') }}</div>
            <RemoteParamsForm v-else :timing="timing" :ops="ops" />
            <p v-if="displayError" class="error">{{ displayError }}</p>
          </div>

          <div class="modal-footer">
            <button class="btn btn-secondary" @click="close" :disabled="isSaving">
              {{ t('runtimeParams.cancel') }}
            </button>
            <button
              class="btn btn-primary"
              @click="handleSave"
              :disabled="isSaving || loading || transportLoading || !paramsReady || !transportReady || hasLoadError"
            >
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
