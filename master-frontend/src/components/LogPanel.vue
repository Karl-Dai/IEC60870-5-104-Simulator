<script setup lang="ts">
import { ref, shallowRef, computed, inject, nextTick, onMounted, onUnmounted, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'
import type { LogEntry, ConnectionInfo } from '../types'
import { useI18n } from '@shared/i18n'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { localizeLegacyBackendText } from '@shared/i18n/backendText'
import { useResizableColumns } from '@shared/composables/useResizableColumns'
import {
  describeFrame,
  describeWireFrame,
  formatLogTimestamp,
  formatRawBytes,
  frameSearchText,
  frameTypeIds,
  logIdentity,
  matchesDirection,
  matchesFrameFilter,
  matchesSearch,
  type FrameFilter,
} from '@shared/logging/logView'

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

const logs = shallowRef<LogEntry[]>([])
const directionFilter = ref('all')
const frameFilter = ref<FrameFilter>('all')
const searchQuery = ref('')
const autoFollow = ref(true)
const hasActiveFilters = computed(() =>
  directionFilter.value !== 'all' || frameFilter.value !== 'all' || searchQuery.value.trim() !== '')
const availableFrameTypes = computed(() => frameTypeIds(logs.value))
const frameTypeOptions = computed(() => {
  const types = availableFrameTypes.value
  if (!frameFilter.value.startsWith('type:')) return types
  const selectedType = frameFilter.value.slice(5)
  return types.includes(selectedType)
    ? types
    : [...types, selectedType].sort((a, b) => a.localeCompare(b))
})
// 折叠栏状态点:有报文流过为绿,空为暗灰。
const hasLogs = computed(() => logs.value.length > 0)
const connectionList = ref<{ id: string; label: string }[]>([])
const selectedConnId = ref('')
let refreshTimer: number | null = null
let activeLoggingId: string | null = null
let desiredLoggingId: string | null = null
let loggingSyncQueue: Promise<void> = Promise.resolve()
let loadGeneration = 0
let clearingConnectionId: string | null = null
let activeLogLoad: { connectionId: string; generation: number; promise: Promise<void> } | null = null
let connectionListGeneration = 0
let activeConnectionListLoad: { generation: number; promise: Promise<void> } | null = null

function setBackendLogging(connId: string, enabled: boolean): Promise<void> {
  if (!connId) return Promise.resolve()
  return invoke<void>('set_logging_enabled', { connectionId: connId, enabled })
    .catch(() => { /* ignore */ })
}

async function reconcileLoggingState() {
  while (activeLoggingId !== desiredLoggingId) {
    if (activeLoggingId) {
      const previous = activeLoggingId
      await setBackendLogging(previous, false)
      activeLoggingId = null
      continue
    }
    const wanted = desiredLoggingId
    if (wanted) {
      await setBackendLogging(wanted, true)
      activeLoggingId = wanted
    }
  }
}

function queueLoggingState(wanted: string | null): Promise<void> {
  desiredLoggingId = wanted
  loggingSyncQueue = loggingSyncQueue.then(reconcileLoggingState, reconcileLoggingState)
  return loggingSyncQueue
}

function syncLoggingState(): Promise<void> {
  return queueLoggingState(props.expanded ? selectedConnId.value || null : null)
}

async function performConnectionListLoad(generation: number) {
  try {
    const conns = await invoke<ConnectionInfo[]>('list_connections')
    if (generation !== connectionListGeneration) return
    connectionList.value = conns.map(c => ({
      id: c.id,
      label: `${c.target_address}:${c.port}`,
    }))
    // Auto-select: prefer the currently selected connection in the tree
    if (selectedConnectionId.value && conns.some(c => c.id === selectedConnectionId.value)) {
      selectedConnId.value = selectedConnectionId.value
    } else if (!conns.some(c => c.id === selectedConnId.value)) {
      selectedConnId.value = connectionList.value[0]?.id ?? ''
    }
  } catch (_e) { /* ignore */ }
}

function loadConnections(): Promise<void> {
  const generation = connectionListGeneration
  if (activeConnectionListLoad?.generation === generation) {
    return activeConnectionListLoad.promise
  }
  const promise = performConnectionListLoad(generation)
  activeConnectionListLoad = { generation, promise }
  void promise.finally(() => {
    if (activeConnectionListLoad?.promise === promise) activeConnectionListLoad = null
  })
  return promise
}

async function performLogLoad(connectionId: string, generation: number) {
  try {
    const next = await invoke<LogEntry[]>('get_communication_logs', {
      connectionId,
    })
    if (generation !== loadGeneration || selectedConnId.value !== connectionId) return
    const prev = logs.value
    const sameLen = prev.length === next.length
    const sameBounds = sameLen && (prev.length === 0 || (
      logIdentity(prev[0]) === logIdentity(next[0])
      && logIdentity(prev[prev.length - 1]) === logIdentity(next[next.length - 1])
    ))
    if (!sameBounds) {
      const anchor = autoFollow.value ? null : captureViewportAnchor()
      logs.value = next
      await nextTick()
      if (autoFollow.value) scrollToLatest()
      else restoreViewportAnchor(anchor)
    }
  } catch (_e) { /* ignore */ }
}

function loadLogs(): Promise<void> {
  const connectionId = selectedConnId.value
  if (!connectionId) {
    loadGeneration++
    return Promise.resolve()
  }
  if (clearingConnectionId === connectionId) return Promise.resolve()
  const generation = loadGeneration
  if (activeLogLoad?.connectionId === connectionId && activeLogLoad.generation === generation) {
    return activeLogLoad.promise
  }
  const promise = performLogLoad(connectionId, generation)
  activeLogLoad = { connectionId, generation, promise }
  void promise.finally(() => {
    if (activeLogLoad?.promise === promise) activeLogLoad = null
  })
  return promise
}

async function clearLogs() {
  const connectionId = selectedConnId.value
  if (!connectionId || clearingConnectionId === connectionId) return
  clearingConnectionId = connectionId
  loadGeneration++
  try {
    await invoke('clear_communication_logs', { connectionId })
    if (selectedConnId.value === connectionId) {
      loadGeneration++
      logs.value = []
      scrollToLatest()
    }
  } catch (_e) { /* ignore */ } finally {
    if (clearingConnectionId === connectionId) clearingConnectionId = null
  }
}

function formatDetail(log: LogEntry): string {
  if (log.detail_event && log.detail_event.kind) {
    return t(`log.${log.detail_event.kind}`, log.detail_event.payload)
  }
  return localizeLegacyBackendText(log.detail, locale.value, t, 'log.backendDetailFallback')
}

const isExporting = ref(false)

async function exportLogs() {
  if (!selectedConnId.value || isExporting.value) return
  const connectionId = selectedConnId.value
  const entries = hasActiveFilters.value ? filteredLogs.value.slice() : undefined
  const path = await save({
    filters: [{ name: 'CSV', extensions: ['csv'] }],
    defaultPath: `iec104_master_log_${Date.now()}.csv`,
  })
  if (!path) return
  isExporting.value = true
  try {
    const args: {
      connectionId: string
      path: string
      entries?: LogEntry[]
    } = { connectionId, path }
    if (entries !== undefined) args.entries = entries
    await invoke('save_logs_csv', args)
  } catch (e) {
    await showAlert(`${t('log.exportFailed')}: ${e}`)
  } finally {
    isExporting.value = false
  }
}

function formatTimestamp(ts: string): string {
  return formatLogTimestamp(ts, locale.value)
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
  counter_interrogation: 'counterInterrogation',
  clock_sync: 'clockSync',
  single_command: 'singleCommand',
  double_command: 'doubleCommand',
  step_command: 'stepCommand',
  setpoint_normalized: 'setpointNormalized',
  setpoint_scaled: 'setpointScaled',
  setpoint_float: 'setpointFloat',
  bitstring: 'bitstring',
  raw_apdu: 'rawApdu',
  connection_event: 'connectionEvent',
}

function formatFrameLabel(log: LogEntry): string {
  const wire = describeWireFrame(log.raw_bytes)
  if (wire?.kind === 'i') {
    return wire.typeId ? t('log.frame.iFrame', { value: wire.typeId }) : 'I'
  }
  if (wire?.kind === 's') return t('log.frame.sFrame', { value: '' })
  if (wire?.kind === 'u') {
    const dictKey = wire.variant ? FRAME_KEY_MAP[wire.variant] : undefined
    return dictKey ? t(`log.frame.${dictKey}`, { value: '' }) : t('log.frame.uFrame')
  }

  const label = log.frame_label
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

function dirClass(dir: string): string {
  return dir.toLowerCase()
}

function frameLabelClass(log: LogEntry): string {
  const kind = describeFrame(log.frame_label, log.raw_bytes).kind
  return kind === 'other' ? '' : `frame-${kind}`
}

function searchableFields(log: LogEntry): string[] {
  return [
    formatTimestamp(log.timestamp),
    log.timestamp,
    formatDirection(log.direction),
    formatFrameLabel(log),
    frameSearchText(log.frame_label, log.raw_bytes),
    formatCause(log),
    formatDetail(log),
    log.detail,
    formatRawBytes(log.raw_bytes),
  ]
}

const filteredLogs = computed(() => {
  const query = searchQuery.value.trim()
  return logs.value.filter(log =>
    matchesDirection(log.direction, directionFilter.value)
    && matchesFrameFilter(log.frame_label, frameFilter.value, log.raw_bytes)
    && (query === '' || matchesSearch(searchableFields(log), query)))
})

const displayLogs = computed(() => {
  const arr = filteredLogs.value
  const rows: { log: LogEntry; key: string }[] = new Array(arr.length)
  const occurrences = new Map<string, number>()
  for (let i = 0; i < arr.length; i++) {
    const log = arr[arr.length - 1 - i]
    const identity = logIdentity(log)
    const occurrence = occurrences.get(identity) ?? 0
    occurrences.set(identity, occurrence + 1)
    rows[i] = { log, key: `${identity}\u0000${occurrence}` }
  }
  return rows
})

// Both applications use the same fixed-height virtual list. Apart from
// avoiding 10k-row DOM trees, it makes scroll anchoring deterministic while
// logs continue arriving at the top.
const ROW_HEIGHT = 25
const OVERSCAN = 12
const SCROLL_AWAY_THRESHOLD = 2
const scrollContainer = ref<HTMLDivElement | null>(null)
const scrollTop = ref(0)
const containerHeight = ref(300)
const totalHeight = computed(() => displayLogs.value.length * ROW_HEIGHT)
const visibleCapacity = computed(() => Math.ceil(containerHeight.value / ROW_HEIGHT) + OVERSCAN * 2)
const visibleStart = computed(() => {
  const requested = Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN)
  const lastFullWindowStart = Math.max(0, displayLogs.value.length - visibleCapacity.value)
  return Math.min(requested, lastFullWindowStart)
})
const visibleEnd = computed(() =>
  Math.min(displayLogs.value.length, visibleStart.value + visibleCapacity.value))
const visibleRows = computed(() => displayLogs.value.slice(visibleStart.value, visibleEnd.value))
const offsetY = computed(() => visibleStart.value * ROW_HEIGHT)
const bottomSpacer = computed(() =>
  Math.max(0, totalHeight.value - offsetY.value - visibleRows.value.length * ROW_HEIGHT))

type LogColumn = 'time' | 'direction' | 'frame' | 'cause' | 'detail' | 'raw'
const {
  widths: columnWidths,
  tableWidth,
  startResize,
  resizeWithKeyboard,
} = useResizableColumns<LogColumn>(
  { time: 120, direction: 72, frame: 180, cause: 180, detail: 420, raw: 260 },
  { time: 105, direction: 58, frame: 100, cause: 100, detail: 160, raw: 120 },
)

let scrollRaf = 0
function onScroll(event: Event) {
  const el = event.target as HTMLElement
  if (scrollRaf) return
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    scrollTop.value = el.scrollTop
    containerHeight.value = el.clientHeight
    if (autoFollow.value && el.scrollTop > SCROLL_AWAY_THRESHOLD) {
      autoFollow.value = false
    }
  })
}

interface ViewportAnchor {
  identity: string
  occurrence: number
  offset: number
}

function captureViewportAnchor(): ViewportAnchor | null {
  const el = scrollContainer.value
  if (!el || displayLogs.value.length === 0) return null
  const index = Math.min(displayLogs.value.length - 1, Math.floor(el.scrollTop / ROW_HEIGHT))
  const identity = logIdentity(displayLogs.value[index].log)
  let occurrence = 0
  for (let i = 0; i < index; i++) {
    if (logIdentity(displayLogs.value[i].log) === identity) occurrence++
  }
  return { identity, occurrence, offset: el.scrollTop - index * ROW_HEIGHT }
}

function restoreViewportAnchor(anchor: ViewportAnchor | null) {
  if (!anchor || !scrollContainer.value) return
  let occurrence = 0
  const index = displayLogs.value.findIndex(row => {
    if (logIdentity(row.log) !== anchor.identity) return false
    if (occurrence++ !== anchor.occurrence) return false
    return true
  })
  if (index < 0) return
  const nextTop = index * ROW_HEIGHT + anchor.offset
  scrollContainer.value.scrollTop = nextTop
  scrollTop.value = nextTop
}

function scrollToLatest() {
  void nextTick(() => {
    if (!scrollContainer.value) return
    scrollContainer.value.scrollTop = 0
    scrollTop.value = 0
  })
}

function toggleAutoFollow() {
  autoFollow.value = !autoFollow.value
  if (autoFollow.value) scrollToLatest()
}

function measureContainer() {
  if (scrollContainer.value) containerHeight.value = scrollContainer.value.clientHeight
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

watch(() => props.expanded, async (expanded) => {
  if (expanded) {
    loadConnections()
    syncLoggingState()
    await loadLogs()
    startAutoRefresh()
    await nextTick()
    measureContainer()
  } else {
    stopAutoRefresh()
    syncLoggingState()
  }
})

watch(selectedConnId, () => {
  loadGeneration++
  logs.value = []
  autoFollow.value = true
  scrollToLatest()
  syncLoggingState()
  loadLogs()
})

watch([directionFilter, frameFilter, searchQuery], () => {
  void nextTick(() => {
    const el = scrollContainer.value
    if (!el) return
    if (autoFollow.value) {
      el.scrollTop = 0
      scrollTop.value = 0
      return
    }
    const maxTop = Math.max(0, totalHeight.value - el.clientHeight)
    if (el.scrollTop > maxTop) {
      el.scrollTop = maxTop
      scrollTop.value = maxTop
    }
  })
})

onMounted(async () => {
  await loadConnections()
  await syncLoggingState()
  if (selectedConnId.value) await loadLogs()
  if (props.expanded) {
    startAutoRefresh()
    await nextTick()
    measureContainer()
  }
})

onUnmounted(() => {
  loadGeneration++
  connectionListGeneration++
  stopAutoRefresh()
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
  void queueLoggingState(null)
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
        <select v-model="selectedConnId" class="conn-select">
          <option v-for="conn in connectionList" :key="conn.id" :value="conn.id">{{ conn.label }}</option>
        </select>
        <button class="log-btn" @click="loadLogs">{{ t('log.refresh') }}</button>
        <button class="log-btn" @click="clearLogs">{{ t('log.clear') }}</button>
        <button class="log-btn" :disabled="isExporting" @click="exportLogs">{{ isExporting ? t('log.exporting') : t('log.export') }}</button>
      </div>
    </div>

    <div v-if="expanded" class="log-content">
      <div class="log-filters" @click.stop>
        <button
          class="auto-follow-btn"
          :class="{ active: autoFollow }"
          :aria-pressed="autoFollow"
          @click="toggleAutoFollow"
        >{{ t('log.autoScroll') }}</button>
        <label class="filter-field">
          <span>{{ t('log.directionCol') }}</span>
          <select v-model="directionFilter" class="filter-select dir-filter">
            <option value="all">{{ t('log.allDirections') }}</option>
            <option value="rx">RX</option>
            <option value="tx">TX</option>
          </select>
        </label>
        <label class="filter-field">
          <span>{{ t('log.frameCol') }}</span>
          <select v-model="frameFilter" class="filter-select frame-filter">
            <option value="all">{{ t('log.allFrames') }}</option>
            <option value="kind:i">{{ t('log.iFrames') }}</option>
            <option value="kind:s">{{ t('log.sFrames') }}</option>
            <option value="kind:u">{{ t('log.uFrames') }}</option>
            <option v-for="typeId in frameTypeOptions" :key="typeId" :value="`type:${typeId}`">{{ typeId }}</option>
          </select>
        </label>
        <input
          v-model="searchQuery"
          class="log-search"
          type="search"
          :placeholder="t('log.searchPlaceholder')"
          :aria-label="t('log.searchPlaceholder')"
        />
        <span class="filter-count">{{ t('log.filteredCount', { visible: filteredLogs.length, total: logs.length }) }}</span>
      </div>

      <div ref="scrollContainer" class="log-body" @scroll="onScroll">
        <div v-if="connectionList.length === 0" class="log-empty">{{ t('log.noConnections') }}</div>
        <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
        <div v-else-if="filteredLogs.length === 0" class="log-empty">{{ t('log.noMatches') }}</div>
        <table v-else class="log-table" :style="{ width: tableWidth + 'px' }">
          <colgroup>
            <col class="col-time" :style="{ width: columnWidths.time + 'px' }" />
            <col class="col-dir" :style="{ width: columnWidths.direction + 'px' }" />
            <col class="col-frame" :style="{ width: columnWidths.frame + 'px' }" />
            <col class="col-cause" :style="{ width: columnWidths.cause + 'px' }" />
            <col class="col-detail" :style="{ width: columnWidths.detail + 'px' }" />
            <col class="col-raw" :style="{ width: columnWidths.raw + 'px' }" />
          </colgroup>
          <thead>
            <tr>
              <th>{{ t('log.timeCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.timeCol') })" @pointerdown="startResize('time', $event)" @keydown="resizeWithKeyboard('time', $event)"></span></th>
              <th>{{ t('log.directionCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.directionCol') })" @pointerdown="startResize('direction', $event)" @keydown="resizeWithKeyboard('direction', $event)"></span></th>
              <th>{{ t('log.frameCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.frameCol') })" @pointerdown="startResize('frame', $event)" @keydown="resizeWithKeyboard('frame', $event)"></span></th>
              <th>{{ t('log.causeCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.causeCol') })" @pointerdown="startResize('cause', $event)" @keydown="resizeWithKeyboard('cause', $event)"></span></th>
              <th>{{ t('log.detailCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.detailCol') })" @pointerdown="startResize('detail', $event)" @keydown="resizeWithKeyboard('detail', $event)"></span></th>
              <th>{{ t('log.rawCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.rawCol') })" @pointerdown="startResize('raw', $event)" @keydown="resizeWithKeyboard('raw', $event)"></span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="offsetY > 0" class="log-spacer" aria-hidden="true">
              <td colspan="6" :style="{ height: offsetY + 'px', padding: 0 }"></td>
            </tr>
            <tr v-for="row in visibleRows" :key="row.key"
                :class="{ 'log-row-parsable': !!row.log.raw_bytes && row.log.raw_bytes.length > 0 }"
                :title="row.log.raw_bytes && row.log.raw_bytes.length ? t('toolbar.parseFrameInLog') : ''"
                @contextmenu="onLogContextMenu($event, row.log)">
              <td class="col-time">{{ formatTimestamp(row.log.timestamp) }}</td>
              <td :class="['col-dir', dirClass(row.log.direction)]">{{ formatDirection(row.log.direction) }}</td>
              <td :class="['col-frame', frameLabelClass(row.log)]">{{ formatFrameLabel(row.log) }}</td>
              <td class="col-cause">{{ formatCause(row.log) }}</td>
              <td class="col-detail" :title="formatDetail(row.log)">{{ formatDetail(row.log) }}</td>
              <td class="col-raw" :title="formatRawBytes(row.log.raw_bytes)">{{ formatRawBytes(row.log.raw_bytes) }}</td>
            </tr>
            <tr v-if="bottomSpacer > 0" class="log-spacer" aria-hidden="true">
              <td colspan="6" :style="{ height: bottomSpacer + 'px', padding: 0 }"></td>
            </tr>
          </tbody>
        </table>
      </div>
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
  min-width: 0;
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
  white-space: nowrap;
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
  overflow-x: auto;
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

.log-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.log-content {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.log-filters {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 6px 8px;
  border-bottom: 1px solid var(--c-base);
  background: var(--c-mantle);
  flex-shrink: 0;
}

.auto-follow-btn,
.filter-select,
.log-search {
  min-height: 26px;
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  background: var(--c-surface0);
  color: var(--c-text);
  font-size: 11px;
}

.auto-follow-btn {
  padding: 2px 9px;
  cursor: pointer;
}

.auto-follow-btn.active {
  color: var(--c-base);
  border-color: var(--c-green);
  background: var(--c-green);
}

.filter-field {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--c-overlay0);
  font-size: 11px;
  white-space: nowrap;
}

.filter-select {
  padding: 2px 24px 2px 6px;
  max-width: 190px;
}

.log-search {
  flex: 1 1 180px;
  min-width: 120px;
  padding: 3px 8px;
}

.filter-count {
  margin-left: auto;
  color: var(--c-overlay0);
  font: 11px var(--font-mono);
  white-space: nowrap;
}

.log-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--c-crust);
}

.log-empty {
  padding: 24px;
  text-align: center;
  color: var(--c-overlay0);
  font-size: 12px;
}

.log-table {
  border-collapse: collapse;
  font-size: 12px;
  font-family: var(--font-mono);
  table-layout: fixed;
}

.log-table th,
.log-table td {
  padding: 4px 10px;
  text-align: left;
  border-bottom: 1px solid var(--c-base);
  line-height: 16px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.log-table th {
  background: var(--c-base);
  color: var(--c-overlay0);
  font-weight: 500;
  position: sticky;
  top: 0;
  z-index: 1;
  overflow: visible;
}

.column-resizer {
  position: absolute;
  top: 0;
  right: -4px;
  width: 9px;
  height: 100%;
  cursor: col-resize;
  touch-action: none;
  z-index: 2;
}

.column-resizer::after {
  content: '';
  position: absolute;
  top: 20%;
  bottom: 20%;
  left: 4px;
  width: 1px;
  background: var(--c-surface1);
}

.column-resizer:hover::after,
.column-resizer:focus-visible::after {
  background: var(--c-blue);
}

.log-spacer td {
  border-bottom: none;
}

.col-time {
  color: var(--c-overlay0);
}

.col-dir {
  font-weight: 600;
}

.col-dir.rx { color: var(--c-blue); }
.col-dir.tx { color: var(--c-green); }

.col-frame {
}

.col-frame.frame-u { color: var(--c-mauve); }
.col-frame.frame-i { color: var(--c-sky); }
.col-frame.frame-s { color: var(--c-yellow); }

.col-cause {
  font-size: 11px;
  color: var(--c-yellow);
}

.col-raw {
  font-size: 11px;
  color: var(--c-surface2);
}

.log-status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}
.log-status-dot.active { background: var(--c-green); }
.log-status-dot.idle { background: var(--c-overlay0); }

@media (max-width: 680px) {
  .log-header {
    gap: 4px;
  }

  .conn-select {
    max-width: 100px;
  }

  .log-btn {
    padding-inline: 5px;
  }

  .log-filters {
    gap: 5px;
    padding: 5px 6px;
  }

  .filter-count {
    margin-left: 0;
  }
}
</style>
