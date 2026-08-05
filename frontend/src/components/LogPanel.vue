<script setup lang="ts">
import { ref, shallowRef, computed, inject, watch, nextTick, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'
import type { LogEntry } from '../types'
import { useI18n } from '@shared/i18n'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import { localizeLegacyBackendText } from '@shared/i18n/backendText'
import { useResizableColumns } from '@shared/composables/useResizableColumns'
import {
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

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

function onLogContextMenu(e: MouseEvent, log: LogEntry) {
  if (!log.raw_bytes || log.raw_bytes.length === 0) return
  e.preventDefault()
  openParseFrame(formatRawBytes(log.raw_bytes))
}

// shallowRef: 日志条目可达数千行，deep ref 在每次 invoke 全替换时
// 会重建所有 Proxy，触发 v-for diff 全量重渲染（视觉上一闪一闪）。
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

function formatDetail(log: LogEntry): string {
  if (log.detail_event && log.detail_event.kind) {
    return t(`log.${log.detail_event.kind}`, log.detail_event.payload)
  }
  return localizeLegacyBackendText(log.detail, locale.value, t, 'log.backendDetailFallback')
}

function formatTimestamp(timestamp: string): string {
  return formatLogTimestamp(timestamp, locale.value)
}

function formatDirection(direction: string): string {
  return direction.toUpperCase()
}

function formatFrameLabel(log: LogEntry): string {
  const wire = describeWireFrame(log.raw_bytes)
  if (wire?.kind === 'i') return wire.typeId ? `I: ${wire.typeId}` : 'I'
  if (wire?.kind === 's') return 'S'
  if (wire?.kind === 'u') {
    const value = wire.variant?.slice(2).replaceAll('_', ' ').toUpperCase()
    return value ? `U: ${value}` : 'U'
  }
  const label = log.frame_label
  if (typeof label === 'string') return label
  const entries = Object.entries(label)
  if (entries.length === 0) return '-'
  const [key, value] = entries[0]
  return value ? `${key.replace('_frame', '').toUpperCase()}: ${value}` : key
}

function searchableFields(log: LogEntry): string[] {
  return [
    formatTimestamp(log.timestamp),
    log.timestamp,
    formatDirection(log.direction),
    formatFrameLabel(log),
    frameSearchText(log.frame_label, log.raw_bytes),
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

// 倒序：最新条目浮到顶部，与主站 LogPanel 行为对齐。时间戳等内容组成的
// key 在轮询返回全新对象时仍保持稳定，避免可见行无谓重建。
const displayLogs = computed(() => {
  const arr = filteredLogs.value
  const n = arr.length
  const out: { log: LogEntry; key: string }[] = new Array(n)
  const occurrences = new Map<string, number>()
  for (let i = 0; i < n; i++) {
    const log = arr[n - 1 - i]
    const identity = logIdentity(log)
    const occurrence = occurrences.get(identity) ?? 0
    occurrences.set(identity, occurrence + 1)
    out[i] = { log, key: `${identity}\u0000${occurrence}` }
  }
  return out
})
// 折叠栏状态点:有报文流过为绿,空为暗灰。
const hasLogs = computed(() => logs.value.length > 0)

// === 虚拟滚动(与 DataPointTable 同构)===
// 日志上限 10000 条,一次性渲染 40000 个 DOM 节点会卡;仅渲染可视窗口的行。
// 依赖固定行高:模板/样式强制单行(nowrap + ellipsis + table-layout:fixed)。
// 25 = line-height 16 + 上下 padding 各 4 + 底边框 1(须与实际渲染行高一致,
// 否则累积漂移导致滚动错位)。
const ROW_HEIGHT = 25
const OVERSCAN = 12
const scrollContainer = ref<HTMLDivElement | null>(null)
const scrollTop = ref(0)
const containerHeight = ref(300)
const SCROLL_AWAY_THRESHOLD = 2

type LogColumn = 'time' | 'direction' | 'frame' | 'detail'
const {
  widths: columnWidths,
  tableWidth,
  startResize,
  resizeWithKeyboard,
} = useResizableColumns<LogColumn>(
  { time: 120, direction: 72, frame: 180, detail: 600 },
  { time: 105, direction: 58, frame: 100, detail: 160 },
)

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

// 把滚动事件合并到每帧一次,避免频繁触发虚拟滚动 computed。
let scrollRaf = 0
function onScroll(e: Event) {
  const el = e.target as HTMLElement
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

// 展开面板后测量滚动容器高度(初次为 fallback 值)。
function measureContainer() {
  const el = scrollContainer.value
  if (el) containerHeight.value = el.clientHeight
}
const isLoading = ref(false)
const error = ref<string | null>(null)
let refreshTimer: number | null = null
let loadGeneration = 0
let clearingServerId: string | null = null
let activeLogLoad: { serverId: string; generation: number; promise: Promise<void> } | null = null

async function performLogLoad(serverId: string, generation: number) {
  // Keep the current rows mounted during polling so a slow backend call does
  // not collapse the table or disturb the user's viewport.
  isLoading.value = logs.value.length === 0
  try {
    const next = await invoke<LogEntry[]>('get_communication_logs', {
      serverId,
    })
    if (generation !== loadGeneration || selectedServerId.value !== serverId) return
    // 仅在确有新条目时替换 ref，避免 polling 时无变化也触发表格 v-for diff
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
  } catch (e) {
    if (generation === loadGeneration) error.value = String(e)
  }
  if (generation === loadGeneration) isLoading.value = false
}

function loadLogs(): Promise<void> {
  const serverId = selectedServerId.value
  if (!serverId) {
    loadGeneration++
    logs.value = []
    return Promise.resolve()
  }
  if (clearingServerId === serverId) return Promise.resolve()
  const generation = loadGeneration
  if (activeLogLoad?.serverId === serverId && activeLogLoad.generation === generation) {
    return activeLogLoad.promise
  }
  const promise = performLogLoad(serverId, generation)
  activeLogLoad = { serverId, generation, promise }
  void promise.finally(() => {
    if (activeLogLoad?.promise === promise) activeLogLoad = null
  })
  return promise
}

async function clearLogs() {
  const serverId = selectedServerId.value
  if (!serverId || clearingServerId === serverId) return
  clearingServerId = serverId
  loadGeneration++
  isLoading.value = false
  try {
    await invoke('clear_communication_logs', {
      serverId,
    })
    if (selectedServerId.value === serverId) {
      loadGeneration++
      logs.value = []
      scrollToLatest()
    }
  } catch (e) {
    if (selectedServerId.value === serverId) error.value = String(e)
  } finally {
    if (clearingServerId === serverId) clearingServerId = null
  }
}

const isExporting = ref(false)

async function exportLogs() {
  if (!selectedServerId.value || isExporting.value) return
  const serverId = selectedServerId.value
  const entries = hasActiveFilters.value ? filteredLogs.value.slice() : undefined
  const path = await save({
    filters: [{ name: 'CSV', extensions: ['csv'] }],
    defaultPath: `iec104_log_${Date.now()}.csv`,
  })
  if (!path) return

  isExporting.value = true
  try {
    const args: {
      serverId: string
      path: string
      entries?: LogEntry[]
    } = {
      serverId,
      path,
    }
    // With no active view state the backend remains authoritative, including
    // entries that may have arrived after the last UI poll.
    if (entries !== undefined) args.entries = entries
    await invoke('save_logs_csv', args)
  } catch (e) {
    await showAlert(`${t('log.exportFailed')}: ${e}`)
  } finally {
    isExporting.value = false
  }
}

function toggleExpanded() {
  emit('toggle')
}

function startAutoRefresh() {
  if (refreshTimer) return
  refreshTimer = window.setInterval(() => {
    if (props.expanded && selectedServerId.value) {
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

watch(() => props.expanded, async (expanded) => {
  if (expanded) {
    if (selectedServerId.value) await loadLogs()
    startAutoRefresh()
    await nextTick()
    measureContainer() // 面板刚展开,量取滚动容器可视高度
  } else {
    stopAutoRefresh()
  }
})

watch(selectedServerId, async (nextId, previousId) => {
  if (nextId !== previousId) {
    loadGeneration++
    logs.value = []
    isLoading.value = false
    autoFollow.value = true
    scrollToLatest()
  }
  if (props.expanded && nextId) {
    await loadLogs()
  } else {
    logs.value = []
  }
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
  if (props.expanded && selectedServerId.value) {
    await loadLogs()
    startAutoRefresh()
    await nextTick()
    measureContainer()
  }
})

onUnmounted(() => {
  loadGeneration++
  stopAutoRefresh()
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
})
</script>

<template>
  <div :class="['log-panel', { expanded }]">
    <div class="log-header" @click="toggleExpanded">
      <span class="log-toggle">{{ expanded ? '\u25BC' : '\u25B2' }}</span>
      <span class="log-status-dot" :class="hasLogs ? 'active' : 'idle'" aria-hidden="true"></span>
      <span class="log-title">{{ t('log.title') }}</span>
      <div class="log-controls" @click.stop>
        <button class="log-btn" @click="loadLogs" :title="t('log.titleRefresh')">{{ t('log.refresh') }}</button>
        <button class="log-btn" @click="clearLogs" :title="t('log.titleClear')">{{ t('log.clear') }}</button>
        <button
          class="log-btn"
          @click="exportLogs"
          :disabled="!selectedServerId || isExporting"
          :title="t('log.titleExport')"
        >{{ isExporting ? t('log.exporting') : t('log.export') }}</button>
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
        <div v-if="isLoading" class="log-loading">{{ t('log.loading') }}</div>
        <div v-else-if="!selectedServerId" class="log-empty">{{ t('log.chooseServer') }}</div>
        <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
        <div v-else-if="filteredLogs.length === 0" class="log-empty">{{ t('log.noMatches') }}</div>
        <table v-else class="log-table" :style="{ width: tableWidth + 'px' }">
          <colgroup>
            <col class="col-time" :style="{ width: columnWidths.time + 'px' }" />
            <col class="col-dir" :style="{ width: columnWidths.direction + 'px' }" />
            <col class="col-frame" :style="{ width: columnWidths.frame + 'px' }" />
            <col class="col-detail" :style="{ width: columnWidths.detail + 'px' }" />
          </colgroup>
          <thead>
            <tr>
              <th>{{ t('log.timeCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.timeCol') })" @pointerdown="startResize('time', $event)" @keydown="resizeWithKeyboard('time', $event)"></span></th>
              <th>{{ t('log.directionCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.directionCol') })" @pointerdown="startResize('direction', $event)" @keydown="resizeWithKeyboard('direction', $event)"></span></th>
              <th>{{ t('log.frameCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.frameCol') })" @pointerdown="startResize('frame', $event)" @keydown="resizeWithKeyboard('frame', $event)"></span></th>
              <th>{{ t('log.detailCol') }}<span class="column-resizer" role="separator" tabindex="0" :aria-label="t('log.resizeColumn', { column: t('log.detailCol') })" @pointerdown="startResize('detail', $event)" @keydown="resizeWithKeyboard('detail', $event)"></span></th>
            </tr>
          </thead>
          <tbody>
            <!-- 虚拟滚动:上/下 spacer 行撑出完整滚动高度,仅渲染可视窗口的行。 -->
            <tr v-if="offsetY > 0" class="log-spacer" aria-hidden="true">
              <td colspan="4" :style="{ height: offsetY + 'px', padding: 0 }"></td>
            </tr>
            <tr v-for="row in visibleRows" :key="row.key"
                :class="{ 'log-row-parsable': !!row.log.raw_bytes && row.log.raw_bytes.length > 0 }"
                :title="row.log.raw_bytes && row.log.raw_bytes.length ? t('toolbar.parseFrameInLog') : ''"
                @contextmenu="onLogContextMenu($event, row.log)">
              <td class="col-time">{{ formatTimestamp(row.log.timestamp) }}</td>
              <td :class="['col-dir', row.log.direction.toLowerCase()]">{{ formatDirection(row.log.direction) }}</td>
              <td class="col-frame">{{ formatFrameLabel(row.log) }}</td>
              <td class="col-detail" :title="formatDetail(row.log)">{{ formatDetail(row.log) }}</td>
            </tr>
            <tr v-if="bottomSpacer > 0" class="log-spacer" aria-hidden="true">
              <td colspan="4" :style="{ height: bottomSpacer + 'px', padding: 0 }"></td>
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
  transition: height 0.2s ease;
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

.log-controls {
  display: flex;
  gap: 4px;
  margin-left: auto;
  overflow-x: auto;
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

.log-loading,
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
  font-family: var(--font-mono);
  /* 固定布局:配合虚拟滚动锁定列宽与单行行高,防止可视行内容差异导致列抖动。 */
  table-layout: fixed;
}

.log-table th,
.log-table td {
  padding: 4px 10px;
  text-align: left;
  border-bottom: 1px solid var(--c-base);
  /* 单行 + 溢出省略:保证每行等高(虚拟滚动 ROW_HEIGHT 前提)。 */
  line-height: 16px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 虚拟滚动上/下占位行:纯撑高,无边框/交互。 */
.log-spacer td {
  border-bottom: none;
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

.col-time {
  color: var(--c-overlay0);
}

.col-dir {
  font-weight: 600;
}

.col-dir.rx {
  color: var(--c-green);
}

.col-dir.tx {
  color: var(--c-blue);
}

.col-frame {
  color: var(--c-text);
}

.col-detail {
  color: var(--c-subtext0);
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
