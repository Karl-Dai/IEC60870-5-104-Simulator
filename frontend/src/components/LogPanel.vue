<script setup lang="ts">
import { ref, shallowRef, computed, inject, watch, nextTick, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LogEntry } from '../types'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()

interface Props {
  expanded: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'toggle'): void
}>()

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!

function rawBytesHex(raw: number[] | null | undefined): string {
  if (!raw) return ''
  return raw.map(b => b.toString(16).toUpperCase().padStart(2, '0')).join(' ')
}

function onLogContextMenu(e: MouseEvent, log: LogEntry) {
  if (!log.raw_bytes || log.raw_bytes.length === 0) return
  e.preventDefault()
  openParseFrame(rawBytesHex(log.raw_bytes))
}

// shallowRef: 日志条目可达数千行，deep ref 在每次 invoke 全替换时
// 会重建所有 Proxy，触发 v-for diff 全量重渲染（视觉上一闪一闪）。
const logs = shallowRef<LogEntry[]>([])
// 倒序：最新条目浮到顶部，与主站 LogPanel 行为对齐。每项携带稳定 key
// (= 原数组正向下标),供虚拟滚动切片后仍能稳定 diff。
const displayLogs = computed(() => {
  const arr = logs.value
  const n = arr.length
  const out: { log: LogEntry; key: number }[] = new Array(n)
  for (let i = 0; i < n; i++) out[i] = { log: arr[n - 1 - i], key: n - 1 - i }
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

const totalHeight = computed(() => displayLogs.value.length * ROW_HEIGHT)
const visibleStart = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN))
const visibleEnd = computed(() => {
  const count = Math.ceil(containerHeight.value / ROW_HEIGHT) + OVERSCAN * 2
  return Math.min(displayLogs.value.length, visibleStart.value + count)
})
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
  })
}

// 展开面板后测量滚动容器高度(初次为 fallback 值)。
function measureContainer() {
  const el = scrollContainer.value
  if (el) containerHeight.value = el.clientHeight
}
const isLoading = ref(false)
const error = ref<string | null>(null)
let refreshTimer: number | null = null

async function loadLogs() {
  if (!selectedServerId.value) {
    logs.value = []
    return
  }
  isLoading.value = true
  try {
    const next = await invoke<LogEntry[]>('get_communication_logs', {
      serverId: selectedServerId.value,
    })
    // 仅在确有新条目时替换 ref，避免 polling 时无变化也触发表格 v-for diff
    const prev = logs.value
    const sameLen = prev.length === next.length
    const sameTail = sameLen && prev.length > 0
      && prev[prev.length - 1].timestamp === next[next.length - 1].timestamp
    if (!sameLen || !sameTail) {
      logs.value = next
    }
  } catch (e) {
    error.value = String(e)
  }
  isLoading.value = false
}

async function clearLogs() {
  if (!selectedServerId.value) return
  try {
    await invoke('clear_communication_logs', {
      serverId: selectedServerId.value,
    })
    logs.value = []
  } catch (e) {
    error.value = String(e)
  }
}

function formatDetail(log: LogEntry): string {
  if (log.detail_event && log.detail_event.kind) {
    return t(`log.${log.detail_event.kind}`, log.detail_event.payload)
  }
  return log.detail
}

function csvEscape(s: string): string {
  return s.replace(/"/g, '""')
}

function exportLogs() {
  if (!selectedServerId.value || logs.value.length === 0) return
  const lines: string[] = []
  lines.push([
    t('log.timeCol'), t('log.directionCol'), t('log.frameCol'), t('log.detailCol'),
  ].map(h => `"${csvEscape(h)}"`).join(','))
  for (const log of logs.value) {
    lines.push([
      `"${csvEscape(formatTimestamp(log.timestamp))}"`,
      `"${csvEscape(log.direction)}"`,
      `"${csvEscape(formatFrameLabel(log.frame_label))}"`,
      `"${csvEscape(formatDetail(log))}"`,
    ].join(','))
  }
  const csv = '﻿' + lines.join('\r\n')
  const blob = new Blob([csv], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `iec104_log_${Date.now()}.csv`
  a.click()
  URL.revokeObjectURL(url)
}

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts)
    return date.toLocaleTimeString()
  } catch {
    return ts
  }
}

function formatFrameLabel(label: { [key: string]: string } | string): string {
  if (typeof label === 'string') return label
  // label is an object like { "I": "..." } or { "S": "" } or { "U": "STARTDT_ACT" }
  const entries = Object.entries(label)
  if (entries.length === 0) return '-'
  const [key, value] = entries[0]
  return value ? `${key}: ${value}` : key
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

watch(selectedServerId, async () => {
  if (props.expanded && selectedServerId.value) {
    await loadLogs()
  } else {
    logs.value = []
  }
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
        <button class="log-btn" @click="exportLogs" :title="t('log.titleExport')">{{ t('log.export') }}</button>
      </div>
    </div>

    <div v-if="expanded" ref="scrollContainer" class="log-body" @scroll="onScroll">
      <div v-if="isLoading" class="log-loading">{{ t('log.loading') }}</div>
      <div v-else-if="!selectedServerId" class="log-empty">{{ t('log.chooseServer') }}</div>
      <div v-else-if="logs.length === 0" class="log-empty">{{ t('log.noLogs') }}</div>
      <table v-else class="log-table">
        <thead>
          <tr>
            <th>{{ t('log.timeCol') }}</th>
            <th>{{ t('log.directionCol') }}</th>
            <th>{{ t('log.frameCol') }}</th>
            <th>{{ t('log.detailCol') }}</th>
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
            <td :class="['col-dir', row.log.direction.toLowerCase()]">{{ row.log.direction }}</td>
            <td class="col-frame">{{ formatFrameLabel(row.log.frame_label) }}</td>
            <td class="col-detail" :title="formatDetail(row.log)">{{ formatDetail(row.log) }}</td>
          </tr>
          <tr v-if="bottomSpacer > 0" class="log-spacer" aria-hidden="true">
            <td colspan="4" :style="{ height: bottomSpacer + 'px', padding: 0 }"></td>
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

.log-controls {
  display: flex;
  gap: 4px;
  margin-left: auto;
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
}

.col-time {
  color: var(--c-overlay0);
  width: 80px;
}

.col-dir {
  font-weight: 600;
  width: 40px;
}

.col-dir.rx {
  color: var(--c-green);
}

.col-dir.tx {
  color: var(--c-blue);
}

.col-frame {
  width: 120px;
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
</style>
