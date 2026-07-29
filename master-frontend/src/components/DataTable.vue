<script setup lang="ts">
import { ref, inject, watch, onMounted, onUnmounted, computed, shallowRef, nextTick, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ReceivedDataPointInfo, IncrementalDataResponse, CommandType, ControlResult, ChangedCategoriesMap, CategoryCountsMap } from '../types'
import { getControlConfig, asduHasTimestamp } from '../types'
import ControlDialog from './ControlDialog.vue'
import QualityIndicator from '@shared/components/QualityIndicator.vue'
import QualityLegend from '@shared/components/QualityLegend.vue'
import DoublePointLegend from './DoublePointLegend.vue'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import { formatDataPointValue, normalizeDoublePointCode } from '@shared/utils/dataPointValue'

const { t } = useI18n()

const emit = defineEmits<{
  (e: 'point-select', points: ReceivedDataPointInfo[]): void
}>()

const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedCategory = inject<Ref<string | null>>('selectedCategory')!
const dataRefreshKey = inject<Ref<number>>('dataRefreshKey')!
const changedCategories = inject<Ref<ChangedCategoriesMap>>('changedCategories')!
const categoryCounts = inject<Ref<CategoryCountsMap>>('categoryCounts')!

// Composite key: same IOA can carry different ASDU types AND can collide
// across CAs on the same connection. Including CA prevents distinct
// stations' identical IOAs from clobbering each other in the local cache.
const pointKey = (p: { ioa: number; asdu_type: string; common_address: number }) =>
  `${p.common_address}|${p.ioa}|${p.asdu_type}`

// === Core data: plain JS Map (not reactive) + shallow ref for display array ===
let dataMap = new Map<string, ReceivedDataPointInfo>()
const displayPoints = shallowRef<ReceivedDataPointInfo[]>([])
let lastSeq = 0
let currentConnId: string | null = null

// === UI state ===
const selectedKeys = ref<Set<string>>(new Set())
const lastClickedIndex = ref(-1)
const searchFilter = ref('')
const changedKeys = ref<Set<string>>(new Set())
const changeTimers = new Map<string, number>()

// === Virtual scroll ===
const ROW_HEIGHT = 28
const OVERSCAN = 10
const scrollTop = ref(0)
const containerHeight = ref(400)
const scrollContainer = ref<HTMLElement | null>(null)

let pollTimer: number | null = null

// === Rebuild display array from dataMap + update category counts ===
function updateDisplay() {
  const arr = Array.from(dataMap.values())
  arr.sort((a, b) => {
    if (a.common_address !== b.common_address) return a.common_address - b.common_address
    return a.ioa - b.ioa
  })
  displayPoints.value = arr
  if (!currentConnId) return
  // Compute per-CA per-category counts so the tree can show "CA 1 → 单点
  // (123)" instead of summing across stations.
  const byCa = new Map<number, Map<string, number>>()
  for (const p of arr) {
    let perCat = byCa.get(p.common_address)
    if (!perCat) { perCat = new Map(); byCa.set(p.common_address, perCat) }
    perCat.set(p.category, (perCat.get(p.category) || 0) + 1)
  }
  const next = new Map(categoryCounts.value)
  next.set(currentConnId, byCa)
  categoryCounts.value = next
}

// === Fetch: always merge, never replace ===
async function fetchData() {
  const connId = selectedConnectionId.value
  if (!connId) return
  try {
    const resp = await invoke<IncrementalDataResponse>('get_received_data_since', {
      id: connId,
      sinceSeq: lastSeq,
    })
    if (resp.points.length > 0) {
      // 按 CA 分组记录本批次有变化的 category，否则 CA=1 收到一条变位会让
      // CA=2/3 同名 category 节点也跟着 flash 黄。
      const catsByCa = new Map<number, Set<string>>()
      for (const p of resp.points) {
        const k = pointKey(p)
        const old = dataMap.get(k)
        if (!old || old.value !== p.value) {
          markChanged(k)
          let s = catsByCa.get(p.common_address)
          if (!s) { s = new Set(); catsByCa.set(p.common_address, s) }
          s.add(p.category)
        }
        dataMap.set(k, p)
      }
      updateDisplay()
      if (catsByCa.size > 0 && currentConnId) {
        const existing = changedCategories.value.get(currentConnId) ?? new Map<number, Set<string>>()
        const merged = new Map(existing)
        for (const [ca, cats] of catsByCa) {
          const prev = merged.get(ca) ?? new Set<string>()
          const ns = new Set(prev)
          for (const c of cats) ns.add(c)
          merged.set(ca, ns)
        }
        const nextMap = new Map(changedCategories.value)
        nextMap.set(currentConnId, merged)
        changedCategories.value = nextMap
      }
    }
    lastSeq = resp.seq
  } catch (e) {
    console.warn('fetchData error:', e)
  }
}

function markChanged(key: string) {
  changedKeys.value.add(key)
  const prev = changeTimers.get(key)
  if (prev) clearTimeout(prev)
  changeTimers.set(key, window.setTimeout(() => {
    changedKeys.value.delete(key)
    changeTimers.delete(key)
  }, 3000))
}

// === Poll control ===
function startPoll() {
  stopPoll()
  pollTimer = window.setInterval(fetchData, 1000)
}

function stopPoll() {
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null }
}

// === Only reset when connection truly changes ===
function initConnection(connId: string) {
  stopPoll()
  dataMap = new Map()
  displayPoints.value = []
  lastSeq = 0
  changedKeys.value.clear()
  for (const t of changeTimers.values()) clearTimeout(t)
  changeTimers.clear()
  selectedKeys.value.clear()
  emit('point-select', [])
  currentConnId = connId
  fetchData().then(startPoll)
}

onMounted(() => {
  if (selectedConnectionId.value) {
    currentConnId = selectedConnectionId.value
    fetchData().then(startPoll)
  }
})

onUnmounted(() => {
  stopPoll()
  for (const t of changeTimers.values()) clearTimeout(t)
})

watch(selectedConnectionId, (newId) => {
  if (newId === currentConnId) return
  if (!newId) {
    stopPoll()
    currentConnId = null
    dataMap = new Map()
    displayPoints.value = []
    return
  }
  initConnection(newId)
})

// GI / counter read just triggers an extra fetch — no reset
watch(dataRefreshKey, fetchData)

// 切换 CA/分类/搜索 → 列表内容整体替换,必须把滚动归零。否则在大列表(如 CA1
// 单点 5278 条)滚到下方后切到小列表(CA2 单点 21 条),残留的 scrollTop 让
// visibleStart 越过新列表末尾,slice 返回空 → 表格全空白(计数却仍显示 21)。
watch([selectedCA, selectedCategory, searchFilter], () => {
  scrollTop.value = 0
  nextTick(() => { if (scrollContainer.value) scrollContainer.value.scrollTop = 0 })
})

// === Filtered + virtual scroll ===
const filteredPoints = computed(() => {
  let pts = displayPoints.value
  // selectedCA === null → show every station (legacy single-CA behaviour
  // and "click connection node directly" both end up here).
  if (selectedCA.value !== null) {
    pts = pts.filter(p => p.common_address === selectedCA.value)
  }
  if (selectedCategory.value) {
    pts = pts.filter(p => p.category === selectedCategory.value)
  }
  if (searchFilter.value) {
    const q = searchFilter.value.toLowerCase()
    pts = pts.filter(p =>
      p.ioa.toString().includes(q) ||
      p.asdu_type.toLowerCase().includes(q) ||
      p.value.toLowerCase().includes(q)
    )
  }
  return pts
})

const totalHeight = computed(() => filteredPoints.value.length * ROW_HEIGHT)
const visibleStart = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN))
const visibleEnd = computed(() => {
  const count = Math.ceil(containerHeight.value / ROW_HEIGHT) + OVERSCAN * 2
  return Math.min(filteredPoints.value.length, visibleStart.value + count)
})
const visibleRows = computed(() => filteredPoints.value.slice(visibleStart.value, visibleEnd.value))
const offsetY = computed(() => visibleStart.value * ROW_HEIGHT)

function onScroll(e: Event) {
  const el = e.target as HTMLElement
  scrollTop.value = el.scrollTop
  containerHeight.value = el.clientHeight
}

// === Row interaction ===
function handleRowClick(localIdx: number, event: MouseEvent) {
  const globalIdx = visibleStart.value + localIdx
  const point = filteredPoints.value[globalIdx]
  if (!point) return
  const k = pointKey(point)
  if (event.ctrlKey || event.metaKey) {
    const s = new Set(selectedKeys.value)
    s.has(k) ? s.delete(k) : s.add(k)
    selectedKeys.value = s
  } else if (event.shiftKey && lastClickedIndex.value >= 0) {
    const s = new Set(selectedKeys.value)
    const a = Math.min(lastClickedIndex.value, globalIdx)
    const b = Math.max(lastClickedIndex.value, globalIdx)
    for (let i = a; i <= b; i++) {
      const p = filteredPoints.value[i]
      if (p) s.add(pointKey(p))
    }
    selectedKeys.value = s
  } else {
    selectedKeys.value = new Set([k])
  }
  lastClickedIndex.value = globalIdx
  const selected = Array.from(selectedKeys.value).map(key => dataMap.get(key)!).filter(Boolean)
  emit('point-select', selected)
}

const categoryTitle = computed(() =>
  selectedCategory.value ? localizeCategoryLabel(selectedCategory.value) : t('table.allData')
)
const totalCount = computed(() => displayPoints.value.length)
const filteredCount = computed(() => filteredPoints.value.length)
const pointCountLabel = computed(() => {
  const suffix = t('table.countSuffix')
  if (!selectedCategory.value && !searchFilter.value) return `${totalCount.value}${suffix ? ' ' + suffix : ''}`
  return `${filteredCount.value} / ${totalCount.value}${suffix ? ' ' + suffix : ''}`
})

// === Right-click context menu ===
const contextMenu = ref<{ visible: boolean; x: number; y: number; point: ReceivedDataPointInfo | null }>({
  visible: false, x: 0, y: 0, point: null
})

const ctxControlConfig = computed(() => {
  if (!contextMenu.value.point) return null
  return getControlConfig(contextMenu.value.point.category)
})

function handleRowContextMenu(localIdx: number, event: MouseEvent) {
  event.preventDefault()
  const globalIdx = visibleStart.value + localIdx
  const point = filteredPoints.value[globalIdx]
  if (!point) return
  contextMenu.value = { visible: true, x: event.clientX, y: event.clientY, point }
}

function hideContextMenu() {
  contextMenu.value.visible = false
}

async function ctxSendCommand(value: string, selectMode: boolean = false) {
  const point = contextMenu.value.point
  if (!point || !selectedConnectionId.value || !ctxControlConfig.value) return
  hideContextMenu()
  try {
    // The point now carries its own CA (the station that sent it), so the
    // control command targets the correct station even in multi-CA setups.
    await invoke<ControlResult>('send_control_command', {
      request: {
        connection_id: selectedConnectionId.value,
        ioa: point.ioa,
        common_address: point.common_address,
        command_type: ctxControlConfig.value.commandType,
        value: value,
        select: selectMode,
      }
    })
  } catch (e) {
    console.warn('Control command failed:', e)
  }
}

function ctxCopy(text: string) {
  navigator.clipboard.writeText(text)
  hideContextMenu()
}

// ControlDialog state (for free-control entry)
const showControlDialog = ref(false)
const controlDialogIoa = ref<number | null>(null)
const controlDialogCA = ref<number>(1)
const controlDialogType = ref<CommandType | null>(null)

function ctxOpenControlDialog() {
  const point = contextMenu.value.point
  if (!point) return
  controlDialogIoa.value = point.ioa
  controlDialogCA.value = point.common_address
  const config = ctxControlConfig.value
  controlDialogType.value = config?.commandType ?? null
  hideContextMenu()
  showControlDialog.value = true
}

// Helper: check if option matches current value for marking
// optValue is the string from CONTROL_CONFIG options; cv is the point's value string
function isCtxActiveOption(optValue: string): boolean {
  const point = contextMenu.value.point
  const cv = point?.value?.toLowerCase() ?? ''
  // Single point: value='true' => 'on', value='false' => 'off'
  if (optValue === 'true') return cv === 'on'
  if (optValue === 'false') return cv === 'off'
  // Double point: compare normalized protocol codes while retaining support
  // for values produced by older localized backends.
  const isDoublePoint = point?.category === 'double_point'
    || point?.asdu_type.startsWith('M_DP_')
    || point?.asdu_type.startsWith('C_DC_')
  if (isDoublePoint && (optValue === '0' || optValue === '1' || optValue === '2' || optValue === '3')) {
    return normalizeDoublePointCode(cv) === optValue
  }
  return false
}
</script>

<template>
  <div class="data-table-container" @click="hideContextMenu">
    <div v-if="!selectedConnectionId" class="empty-state">{{ t('table.chooseConnection') }}</div>
    <template v-else>
      <div class="table-header">
        <span class="header-title">{{ categoryTitle }}</span>
        <input v-model="searchFilter" class="search-input" type="text" :placeholder="t('table.searchPlaceholder')" />
        <span class="point-count">{{ pointCountLabel }}</span>
      </div>

      <div ref="scrollContainer" class="table-scroll" @scroll="onScroll">
        <!-- Fixed header -->
        <table class="table">
          <thead>
            <tr>
              <th class="col-ioa">IOA</th>
              <th class="col-type">{{ t('table.type') }}</th>
              <th class="col-value">{{ t('table.value') }}</th>
              <th class="col-quality"><span class="th-quality">{{ t('table.quality') }}<QualityLegend /></span></th>
              <th class="col-timestamp">{{ t('table.timestamp') }}</th>
            </tr>
          </thead>
        </table>
        <!-- Virtual scroll body -->
        <div v-if="filteredPoints.length > 0" :style="{ height: totalHeight + 'px', position: 'relative' }">
          <table class="table table-body" :style="{ transform: `translateY(${offsetY}px)` }">
            <tbody>
              <tr
                v-for="(point, i) in visibleRows"
                :key="pointKey(point)"
                :class="{ selected: selectedKeys.has(pointKey(point)), 'value-changed': changedKeys.has(pointKey(point)) }"
                @click="handleRowClick(i, $event)"
                @contextmenu="handleRowContextMenu(i, $event)"
              >
                <td class="col-ioa">{{ point.ioa }}</td>
                <td class="col-type">{{ point.asdu_type }} · {{ point.asdu_type_id }}</td>
                <td :class="['col-value', { 'value-highlight': changedKeys.has(pointKey(point)) }]"><span class="value-text">{{ formatDataPointValue(point, t) }}</span><DoublePointLegend v-if="point.asdu_type.startsWith('M_DP')" /></td>
                <td class="col-quality">
                  <QualityIndicator
                    :quality="{ ov: point.quality_ov, bl: point.quality_bl, sb: point.quality_sb, nt: point.quality_nt, iv: point.quality_iv }"
                    :show-ov="point.asdu_type.startsWith('M_ME')"
                    :show-help="false"
                    compact
                  />
                </td>
                <td class="col-timestamp">{{ asduHasTimestamp(point.asdu_type) ? (point.timestamp ?? '-') : '' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
        <div v-else class="empty-hint-inline">{{ t('table.noDataHint') }}</div>
      </div>
    </template>

    <!-- Right-click context menu -->
    <div v-if="contextMenu.visible" class="context-menu" :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }" @click.stop>
      <template v-if="ctxControlConfig && ctxControlConfig.options">
        <!-- Direct execute options for discrete types -->
        <div
          v-for="opt in ctxControlConfig.options"
          :key="opt.value"
          :class="['ctx-item', { 'ctx-active': isCtxActiveOption(opt.value) }]"
          @click="ctxSendCommand(opt.value)"
        >
          {{ isCtxActiveOption(opt.value) ? '&#9679; ' : '&#9675; ' }}{{ opt.label }}
        </div>
        <div class="ctx-divider"></div>
        <!-- SbO options for discrete types -->
        <div
          v-for="opt in ctxControlConfig.options"
          :key="'sbo-' + opt.value"
          class="ctx-item ctx-sub"
          @click="ctxSendCommand(opt.value, true)"
        >
          SbO: {{ opt.label }}
        </div>
        <div class="ctx-divider"></div>
      </template>
      <template v-else-if="ctxControlConfig">
        <!-- Setpoint types: open dialog -->
        <div class="ctx-item" @click="ctxOpenControlDialog">{{ t('table.setpoint') }}</div>
        <div class="ctx-divider"></div>
      </template>
      <!-- Copy actions (always available) -->
      <div class="ctx-item" @click="ctxCopy(String(contextMenu.point?.ioa ?? ''))">{{ t('table.copyIoa') }}</div>
      <div class="ctx-item" @click="ctxCopy(contextMenu.point?.value ?? '')">{{ t('table.copyValue') }}</div>
      <template v-if="ctxControlConfig">
        <div class="ctx-divider"></div>
        <div class="ctx-item" @click="ctxOpenControlDialog">{{ t('table.freeControl') }}</div>
      </template>
    </div>

    <!-- ControlDialog for free control / setpoint entry. Stays open after a
         successful send so the user can iterate; close via the dialog's
         own button or backdrop click. -->
    <ControlDialog
      :visible="showControlDialog"
      :connection-id="selectedConnectionId"
      :common-address="controlDialogCA"
      :prefill-ioa="controlDialogIoa"
      :prefill-command-type="controlDialogType"
      @close="showControlDialog = false"
    />
  </div>
</template>

<style scoped>
.data-table-container { display: flex; flex-direction: column; height: 100%; }
.empty-state { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--c-overlay0); font-size: 13px; }
.empty-hint-inline { padding: 40px; text-align: center; color: var(--c-overlay0); font-size: 13px; }

.table-header {
  display: flex; align-items: center; gap: 8px; padding: 6px 10px;
  border-bottom: 1px solid var(--c-surface0); flex-shrink: 0; background: var(--c-base);
}
.header-title { font-size: 12px; font-weight: 600; color: var(--c-blue); white-space: nowrap; }
.search-input {
  flex: 1; max-width: 200px; padding: 3px 8px; background: var(--c-surface0);
  border: 1px solid var(--c-surface1); border-radius: 4px; color: var(--c-text); font-size: 12px; margin-left: auto;
}
.search-input:focus { outline: none; border-color: var(--c-blue); }
.point-count { font-size: 11px; color: var(--c-overlay0); white-space: nowrap; }

.table-scroll { flex: 1; overflow-y: auto; }
.table { width: 100%; border-collapse: collapse; font-size: 12px; table-layout: fixed; }
.table thead { position: sticky; top: 0; z-index: 2; }
.table th {
  background: var(--c-base); color: var(--c-overlay0); font-weight: 500;
  padding: 6px 10px; text-align: left; border-bottom: 1px solid var(--c-surface0);
}
.table-body { position: absolute; top: 0; left: 0; width: 100%; }
.table tbody tr { cursor: pointer; height: 28px; }
.table tbody tr:hover { background: var(--c-base); }
.table tbody tr.selected { background: var(--c-blue) !important; color: var(--c-base); }
.table tbody tr.selected td { color: var(--c-base) !important; }
.table tbody tr.value-changed { background: rgba(250, 179, 135, 0.15); }
.table td {
  padding: 4px 10px; border-bottom: 1px solid var(--c-base);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}

.col-ioa { font-family: var(--font-mono); width: 80px; color: var(--c-blue); }
.col-type { font-family: var(--font-mono); width: 148px; }
.col-value { font-family: var(--font-mono); transition: color 0.3s; }
.col-value.value-highlight { color: var(--c-peach); font-weight: 700; }
.col-quality { width: 96px; font-weight: 600; font-size: 11px; }
.th-quality { display: inline-flex; align-items: center; gap: 4px; }
.col-quality.quality-ok { color: var(--c-green); }
.col-quality.quality-iv { color: var(--c-red); }
.col-timestamp { font-family: var(--font-mono); width: 120px; color: var(--c-overlay0); }

/* Context menu */
.context-menu {
  position: fixed;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  padding: 4px 0;
  z-index: 999;
  min-width: 150px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.ctx-item {
  padding: 6px 14px;
  cursor: pointer;
  font-size: 12px;
  color: var(--c-text);
  white-space: nowrap;
}

.ctx-item:hover {
  background: var(--c-surface0);
}

.ctx-active {
  font-weight: 600;
  color: var(--c-blue);
}

.ctx-sub {
  color: var(--c-overlay0);
  font-size: 11px;
}

.ctx-divider {
  height: 1px;
  background: var(--c-surface0);
  margin: 4px 0;
}
</style>
