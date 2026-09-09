<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '@shared/i18n'
import AppButton from '@shared/components/ui/AppButton.vue'
import type { ClientConnectionInfo } from '../types'

const props = defineProps<{
  visible: boolean
  serverId: string
  serverLabel: string
}>()

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()

const connections = ref<ClientConnectionInfo[]>([])
const loading = ref(false)
const errorText = ref('')
let refreshTimer: number | undefined
let loadEpoch = 0
let pollingActive = false

function stopPolling() {
  pollingActive = false
  loadEpoch++
  if (refreshTimer !== undefined) {
    window.clearTimeout(refreshTimer)
    refreshTimer = undefined
  }
}

async function loadConnections() {
  const serverId = props.serverId
  const epoch = ++loadEpoch
  if (!props.visible || !serverId) return
  loading.value = connections.value.length === 0
  try {
    const rows = await invoke<ClientConnectionInfo[]>('list_client_connections', { serverId })
    if (epoch !== loadEpoch || !props.visible || props.serverId !== serverId) return
    connections.value = rows
    errorText.value = ''
  } catch (error) {
    if (epoch === loadEpoch && props.visible && props.serverId === serverId) {
      errorText.value = String(error)
    }
  } finally {
    if (epoch === loadEpoch) loading.value = false
  }
}

async function refreshAndSchedule() {
  await loadConnections()
  if (!pollingActive || !props.visible || !props.serverId) return
  refreshTimer = window.setTimeout(refreshAndSchedule, 1000)
}

function startPolling() {
  stopPolling()
  connections.value = []
  errorText.value = ''
  pollingActive = true
  void refreshAndSchedule()
}

function close() {
  emit('close')
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && props.visible) close()
}

watch(
  [() => props.visible, () => props.serverId],
  ([visible]) => {
    if (visible) {
      startPolling()
      window.addEventListener('keydown', handleKeydown)
    } else {
      stopPolling()
      window.removeEventListener('keydown', handleKeydown)
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  stopPolling()
  window.removeEventListener('keydown', handleKeydown)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="dialog-pop">
      <div
        v-if="visible"
        class="connections-backdrop dialog-blur"
        @mousedown.self="close"
      >
        <section class="connections-modal" role="dialog" aria-modal="true" :aria-label="t('connections.title')">
          <header class="connections-header">
            <div>
              <h3>{{ t('connections.title') }}</h3>
              <span>{{ serverLabel }}</span>
            </div>
            <AppButton variant="ghost" icon="x" class="connections-close" :aria-label="t('common.close')" @click="close" />
          </header>

          <div class="connections-body">
            <div class="connections-summary">
              <span class="summary-dot" :class="{ active: connections.length > 0 }" />
              {{ t('connections.summary', { n: connections.length }) }}
            </div>

            <p v-if="loading" class="connections-message">{{ t('common.loading') }}</p>
            <p v-else-if="errorText" class="connections-message error">{{ errorText }}</p>
            <div v-else-if="connections.length === 0" class="connections-empty">
              <strong>{{ t('connections.empty') }}</strong>
              <span>{{ t('connections.emptyHint') }}</span>
            </div>
            <table v-else class="connections-table">
              <thead>
                <tr>
                  <th>{{ t('connections.colPeer') }}</th>
                  <th>{{ t('connections.colState') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="connection in connections" :key="connection.peer_address">
                  <td class="peer-address">{{ connection.peer_address }}</td>
                  <td>
                    <span
                      class="connection-state"
                      :class="{ active: connection.data_transfer_active }"
                    >
                      {{ connection.data_transfer_active
                        ? t('connections.stateActive')
                        : t('connections.stateConnected') }}
                    </span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <footer class="connections-footer">
            <AppButton variant="primary" class="connections-done" @click="close">{{ t('common.close') }}</AppButton>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.connections-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1700;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--c-crust) 62%, transparent);
}

.connections-modal {
  width: min(520px, 92vw);
  max-height: min(620px, 86vh);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--c-text);
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 9px;
  box-shadow: 0 18px 50px rgba(0, 0, 0, 0.46);
}

.connections-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 18px 13px;
  border-bottom: 1px solid var(--c-surface0);
}

.connections-header h3 {
  margin: 0 0 4px;
  font-size: 15px;
}

.connections-header span {
  color: var(--c-subtext0);
  font: 11px/1.4 var(--font-mono);
}

.connections-close {
  padding: 2px 4px;
  color: var(--c-overlay1);
}

.connections-body {
  min-height: 160px;
  overflow: auto;
  padding: 14px 18px 18px;
}

.connections-summary {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-bottom: 12px;
  color: var(--c-subtext1);
  font-size: 12px;
}

.summary-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--c-surface2);
}

.summary-dot.active {
  background: var(--c-green);
  box-shadow: 0 0 7px color-mix(in srgb, var(--c-green) 55%, transparent);
}

.connections-message,
.connections-empty {
  min-height: 112px;
  margin: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--c-overlay1);
  font-size: 12px;
}

.connections-message.error {
  color: var(--c-red);
}

.connections-empty {
  flex-direction: column;
  gap: 7px;
  text-align: center;
}

.connections-empty strong {
  color: var(--c-subtext1);
  font-size: 13px;
}

.connections-empty span {
  color: var(--c-overlay0);
}

.connections-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 12px;
}

.connections-table th,
.connections-table td {
  padding: 9px 10px;
  text-align: left;
  border-bottom: 1px solid var(--c-surface0);
}

.connections-table th {
  color: var(--c-overlay1);
  background: var(--c-mantle);
  font-weight: 600;
}

.connections-table th:last-child {
  width: 120px;
}

.peer-address {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--c-text);
  font-family: var(--font-mono);
}

.connection-state {
  display: inline-flex;
  align-items: center;
  padding: 2px 7px;
  color: var(--c-subtext0);
  background: var(--c-surface0);
  border-radius: 9px;
  white-space: nowrap;
}

.connection-state.active {
  color: var(--c-green);
  background: color-mix(in srgb, var(--c-green) 12%, var(--c-surface0));
}

.connections-footer {
  display: flex;
  justify-content: flex-end;
  padding: 10px 18px 14px;
  border-top: 1px solid var(--c-surface0);
}
</style>
