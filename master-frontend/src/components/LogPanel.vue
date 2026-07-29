<script setup lang="ts">
import { ref, computed, inject, onMounted, onUnmounted, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'
import type { LogEntry, ConnectionInfo } from '../types'
import { useI18n } from '@shared/i18n'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { localizeLegacyBackendText } from '@shared/i18n/backendText'

const { t, locale } = useI18n()

interface Props {
  expanded: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'toggle'): void
}>()

const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

function onLogContextMenu(e: MouseEvent, log: LogEntry) {
  if (!log.raw_bytes || log.raw_bytes.length === 0) return
  e.preventDefault()
  openParseFrame(formatRawBytes(log.raw_bytes))
}

const logs = ref<LogEntry[]>([])
// 倒序展示：最新条目在表格顶部，便于查看最近通讯
const displayLogs = computed(() => logs.value.slice().reverse())
// 折叠栏状态点:有报文流过为绿,空为暗灰。
const hasLogs = computed(() => logs.value.length > 0)
const connectionList = ref<{ id: string; label: string }[]>([])
const selectedConnId = ref('')
let refreshTimer: number | null = null
let activeLoggingId: string | null = null

function setBackendLogging(connId: string, enabled: boolean): Promise<void> {
  if (!connId) return Promise.resolve()
  return invoke<void>('set_logging_enabled', { connectionId: connId, enabled })
    .catch(() => { /* ignore */ })
}

async function syncLoggingState() {
  const wanted = props.expanded ? selectedConnId.value || null : null
  if (activeLoggingId === wanted) return
  const prev = activeLoggingId
  activeLoggingId = wanted
  await Promise.all([
    prev && prev !== wanted ? setBackendLogging(prev, false) : Promise.resolve(),
    wanted ? setBackendLogging(wanted, true) : Promise.resolve(),
  ])
}

async function loadConnections() {
  try {
    const conns = await invoke<ConnectionInfo[]>('list_connections')
    connectionList.value = conns.map(c => ({
      id: c.id,
      label: `${c.target_address}:${c.port}`,
    }))
    // Auto-select: prefer the currently selected connection in the tree
    if (selectedConnectionId.value && conns.some(c => c.id === selectedConnectionId.value)) {
      selectedConnId.value = selectedConnectionId.value
    } else if (connectionList.value.length > 0 && !selectedConnId.value) {
      selectedConnId.value = connectionList.value[0].id
    }
  } catch (_e) { /* ignore */ }
}

async function loadLogs() {
  if (!selectedConnId.value) return
  try {
    logs.value = await invoke<LogEntry[]>('get_communication_logs', {
      connectionId: selectedConnId.value,
    })
  } catch (_e) { /* ignore */ }
}

async function clearLogs() {
  if (!selectedConnId.value) return
  try {
    await invoke('clear_communication_logs', { connectionId: selectedConnId.value })
    logs.value = []
  } catch (_e) { /* ignore */ }
}

function formatDetail(log: LogEntry): string {
  if (log.detail_event && log.detail_event.kind) {
    return t(`log.${log.detail_event.kind}`, log.detail_event.payload)
  }
  return localizeLegacyBackendText(log.detail, locale.value, t, 'log.backendDetailFallback')
}

const isExporting = ref(false)

async function exportLogs() {
  if (!selectedConnId.value || logs.value.length === 0) return
  const path = await save({
    filters: [{ name: 'CSV', extensions: ['csv'] }],
    defaultPath: `iec104_master_log_${Date.now()}.csv`,
  })
  if (!path) return
  isExporting.value = true
  try {
    await invoke('save_logs_csv', { connectionId: selectedConnId.value, path })
  } catch (e) {
    await showAlert(`${t('log.exportFailed')}: ${e}`)
  } finally {
    isExporting.value = false
  }
}

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts)
    if (isNaN(date.getTime())) return ts
    return date.toLocaleTimeString(locale.value, {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      fractionalSecondDigits: 3,
    } as Intl.DateTimeFormatOptions)
  } catch {
    return ts
  }
}

function formatDirection(dir: string): string {
  return dir.toUpperCase()
}

const FRAME_KEY_MAP: Record<string, string> = {
  i_frame: 'iFrame',
  s_frame: 'sFrame',
  u_start_act: 'uStartAct',
  u_start_con: 'uStartCon',
  u_stop_act: 'uStopAct',
  u_stop_con: 'uStopCon',
  u_test_act: 'uTestAct',
  u_test_con: 'uTestCon',
  general_interrogation: 'generalInterrogation',
  counter_read: 'counterRead',
  clock_sync: 'clockSync',
  single_command: 'singleCommand',
  double_command: 'doubleCommand',
  setpoint_normalized: 'setpointNormalized',
  setpoint_scaled: 'setpointScaled',
  setpoint_float: 'setpointFloat',
  connection_event: 'connectionEvent',
}

function formatFrameLabel(label: LogEntry['frame_label']): string {
  if (typeof label === 'string') {
    // Serde may serialize unit variants as a bare string (e.g. "s_frame")
    const dictKey = FRAME_KEY_MAP[label]
    return dictKey ? t(`log.frame.${dictKey}`, { value: '' }) : label
  }
  const keys = Object.keys(label)
  if (keys.length === 0) return ''
  const key = keys[0]
  const value = label[key]
  const dictKey = FRAME_KEY_MAP[key]
  return dictKey ? t(`log.frame.${dictKey}`, { value }) : key
}

function extractCot(log: LogEntry): number | null {
  const ev = log.detail_event
  if (ev) {
    const payload = ev.payload as Record<string, unknown> | undefined
    const v = payload?.cot
    if (typeof v === 'number') return v
  }
  const m = /COT=(\d+)/.exec(log.detail || '')
  return m ? parseInt(m[1], 10) : null
}

function formatCause(log: LogEntry): string {
  const cot = extractCot(log)
  if (cot === null) return ''
  const key = `log.cot.${cot}`
  const name = t(key)
  if (name === key) return t('log.cot.unknown', { cot })
  return `${cot} · ${name}`
}

function formatRawBytes(raw: number[] | null): string {
  if (!raw || raw.length === 0) return ''
  return raw.map(b => b.toString(16).toUpperCase().padStart(2, '0')).join(' ')
}

function dirClass(dir: string): string {
  return dir.toLowerCase()
}

function frameLabelClass(label: LogEntry['frame_label']): string {
  const key = typeof label === 'string' ? label : Object.keys(label)[0] || ''
  if (key === 'i_frame') return 'frame-i'
  if (key === 's_frame') return 'frame-s'
  if (key.startsWith('u_')) return 'frame-u'
  return ''
}

function startAutoRefresh() {
  if (refreshTimer) return
  refreshTimer = window.setInterval(() => {
    if (props.expanded) {
      loadConnections()
      loadLogs()
    }
  }, 2000)
}

function stopAutoRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
}

// When the selected connection in the tree changes, auto-select it in log panel
watch(selectedConnectionId, (newId) => {
  if (newId && connectionList.value.some(c => c.id === newId)) {
    selectedConnId.value = newId
  }
})

watch(() => props.expanded, (expanded) => {
  if (expanded) {
    loadConnections()
    syncLoggingState()
    loadLogs()
    startAutoRefresh()
  } else {
    stopAutoRefresh()
    syncLoggingState()
  }
})

watch(selectedConnId, () => {
  syncLoggingState()
  loadLogs()
})

onMounted(async () => {
  await loadConnections()
  await syncLoggingState()
  if (selectedConnId.value) await loadLogs()
  if (props.expanded) startAutoRefresh()
})

onUnmounted(() => {
  stopAutoRefresh()
  if (activeLoggingId) {
    setBackendLogging(activeLoggingId, false)
    activeLoggingId = null
  }
})
</script>

<template>
  <div :class="['log-panel', { expanded }]">
    <div class="log-header" @click="emit('toggle')">
      <span class="log-toggle">{{ expanded ? '\u25BC' : '\u25B2' }}</span>
      <span class="log-status-dot" :class="hasLogs ? 'active' : 'idle'" aria-hidden="true"></span>
      <span class="log-title">{{ t('log.title') }}</span>
      <span v-if="!expanded && logs.length > 0" class="log-count">{{ logs.length }}</span>
      <div class="log-controls" @click.stop>
        <select v-model="selectedConnId" class="conn-select" @change="loadLogs">
          <option v-for="conn in connectionList" :key="conn.id" :value="conn.id">{{ conn.label }}</option>
        </select>
        <button class="log-btn" @click="loadLogs">{{ t('log.refresh') }}</button>
        <button class="log-btn" @click="clearLogs">{{ t('log.clear') }}</button>
        <button class="log-btn" :disabled="isExporting" @click="exportLogs">{{ isExporting ? t('log.exporting') : t('log.export') }}</button>
      </div>
    </div>

    <div v-if="expanded" class="log-body">
      <div v-if="connectionList.length === 0" class="log-empty">{{ t('log.noConnections') }}</div>
      <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
      <table v-else class="log-table">
        <thead>
          <tr>
            <th>{{ t('log.timeCol') }}</th>
            <th>{{ t('log.directionCol') }}</th>
            <th>{{ t('log.frameCol') }}</th>
            <th>{{ t('log.causeCol') }}</th>
            <th>{{ t('log.detailCol') }}</th>
            <th>{{ t('log.rawCol') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(log, idx) in displayLogs" :key="logs.length - 1 - idx"
              :class="{ 'log-row-parsable': !!log.raw_bytes && log.raw_bytes.length > 0 }"
              :title="log.raw_bytes && log.raw_bytes.length ? t('toolbar.parseFrameInLog') : ''"
              @contextmenu="onLogContextMenu($event, log)">
            <td class="col-time">{{ formatTimestamp(log.timestamp) }}</td>
            <td :class="['col-dir', dirClass(log.direction)]">{{ formatDirection(log.direction) }}</td>
            <td :class="['col-frame', frameLabelClass(log.frame_label)]">{{ formatFrameLabel(log.frame_label) }}</td>
            <td class="col-cause">{{ formatCause(log) }}</td>
            <td class="col-detail">{{ formatDetail(log) }}</td>
            <td class="col-raw">{{ formatRawBytes(log.raw_bytes) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.log-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  border-top: 1px solid rgba(137, 180, 250, 0.25);
}

.log-panel:not(.expanded) {
  height: 32px;
}

.log-header {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 8px;
  cursor: pointer;
  flex-shrink: 0;
  background: var(--c-crust);
}

.log-toggle {
  font-size: 10px;
  color: var(--c-overlay0);
  width: 16px;
  text-align: center;
}

.log-title {
  font-size: 12px;
  color: var(--c-overlay0);
}

.log-count {
  font-size: 10px;
  background: var(--c-blue);
  color: var(--c-base);
  padding: 0 6px;
  border-radius: 8px;
  font-weight: 600;
}

.log-controls {
  display: flex;
  gap: 4px;
  margin-left: auto;
}

.conn-select {
  padding: 2px 6px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 11px;
  max-width: 160px;
}

.log-btn {
  padding: 2px 8px;
  background: transparent;
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  color: var(--c-text);
  cursor: pointer;
  font-size: 11px;
}

.log-btn:hover {
  background: var(--c-surface0);
}

.log-body {
  flex: 1;
  overflow-y: auto;
  background: var(--c-crust);
}

.log-empty {
  padding: 24px;
  text-align: center;
  color: var(--c-overlay0);
  font-size: 12px;
}

.log-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

.log-table th,
.log-table td {
  padding: 4px 10px;
  text-align: left;
  border-bottom: 1px solid var(--c-base);
}

.log-table th {
  background: var(--c-base);
  color: var(--c-overlay0);
  font-weight: 500;
  position: sticky;
  top: 0;
}

.col-time {
  font-family: var(--font-mono);
  color: var(--c-overlay0);
  width: 100px;
}

.col-dir {
  font-weight: 600;
  width: 40px;
}

.col-dir.rx { color: var(--c-blue); }
.col-dir.tx { color: var(--c-green); }

.col-frame {
  font-family: var(--font-mono);
  width: 130px;
  white-space: nowrap;
}

.col-frame.frame-u { color: var(--c-mauve); }
.col-frame.frame-i { color: var(--c-sky); }
.col-frame.frame-s { color: var(--c-yellow); }

.col-cause {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-yellow);
  width: 160px;
  white-space: nowrap;
}

.col-detail {
  font-family: var(--font-mono);
}

.col-raw {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-surface2);
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}
.log-status-dot.active { background: var(--c-green); }
.log-status-dot.idle { background: var(--c-overlay0); }
</style>
