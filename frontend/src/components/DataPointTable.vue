<script setup lang="ts">
import { ref, inject, watch, computed, nextTick, onMounted, onUnmounted, shallowRef, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import type { DataPointInfo, IncrementalDataResponse, PointMutationInfo, MutationMode } from '../types'
import DataPointModal from './DataPointModal.vue'
import BatchAddModal from './BatchAddModal.vue'
import BatchWriteModal from './BatchWriteModal.vue'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import EmptyState from '@shared/components/EmptyState.vue'
import QualityIndicator from '@shared/components/QualityIndicator.vue'
import QualityLegend from '@shared/components/QualityLegend.vue'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const emit = defineEmits<{
  (e: 'point-select', points: { ioa: number; asdu_type: string; value: string }[]): void
}>()

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedCategory = inject<Ref<string | null>>('selectedCategory')!
const dataRefreshKey = inject<Ref<number>>('dataRefreshKey')!

// === Core data: plain JS Map + shallowRef (same pattern as master DataTable) ===
function pointKey(ioa: number, asduType: string) { return `${ioa}:${asduType}` }
let dataMap = new Map<string, DataPointInfo>()
const displayPoints = shallowRef<DataPointInfo[]>([])
const categoryCounts = inject<Ref<Map<string, number>>>('categoryCounts')!
let currentServerId: string | null = null
let currentCA: number | null = null
// Incremental polling cursor: list_data_points_since returns only points
// whose update_seq exceeds this. Reset to 0 on station switch.
let lastSeq = 0

// === UI state ===
const selectedRows = ref<DataPointInfo[]>([])
const lastClickedIndex = ref(-1)
const editingCell = ref<{ ioa: number; asduType: string } | null>(null)
const editValue = ref('')
const searchQuery = ref('')
const scrollContainer = ref<HTMLDivElement | null>(null)
const showAddModal = ref(false)
const showBatchModal = ref(false)
const showBatchWriteModal = ref(false)
// 默认写值类型：取当前分类过滤命中的首个点的 asdu_type；无过滤则空（弹窗回退首个可用类型）。
const batchWriteDefaultType = computed(() => {
  if (!selectedCategory.value) return ''
  const p = displayPoints.value.find((pt) => pt.category === selectedCategory.value)
  return p?.asdu_type ?? ''
})
// Keyed by `${ioa}:${asduType}` — the same IOA hosts multiple ASDU types, so
// an IOA-only key would flash every type on that IOA when only one changed.
const changedKeys = ref<Set<string>>(new Set())
const changeTimers = new Map<string, number>()
// 当前 (server, CA) 下正在周期变位的点位 → 变位方式（key=`ioa:asdu_type`）。
const activeMutations = ref<Map<string, MutationMode>>(new Map())
// 右键菜单内的周期/变位参数，组件内记住上次值。
const mutationPeriod = ref(1000)
const mutationMode = ref<MutationMode>('flip')
const mutationStep = ref(1)
const mutationMin = ref(0)
const mutationMax = ref(100)

// 变位方式的图标 / 本地化标签（数据表行内显示）。
function mutationGlyph(mode: MutationMode | undefined) {
  return mode === 'increment' ? '↑' : mode === 'decrement' ? '↓' : '⇅'
}
function mutationModeLabel(mode: MutationMode | undefined) {
  if (mode === 'increment') return t('table.modeIncrement')
  if (mode === 'decrement') return t('table.modeDecrement')
  return t('table.modeFlip')
}
// 仅模拟量 (M_ME_*) 与累计量 (M_IT_*) 支持递增/递减;离散量只翻转。
function pointSupportsStep(asduType: string) {
  return /^M_(ME|IT)_/.test(asduType)
}
// 右键点位时按其类型预填步长/上下限默认值(浮点以当前值为中心 ±100)。
function applyMutationDefaults(point: DataPointInfo) {
  const ty = point.asdu_type
  const v0 = Number.parseFloat(point.value) || 0
  if (ty.startsWith('M_ME_NA')) { mutationStep.value = 0.05; mutationMin.value = -1; mutationMax.value = 1 }
  else if (ty.startsWith('M_ME_NB')) { mutationStep.value = 100; mutationMin.value = -10000; mutationMax.value = 10000 }
  else if (ty.startsWith('M_ME_NC')) { mutationStep.value = 1; mutationMin.value = Math.round((v0 - 100) * 1e3) / 1e3; mutationMax.value = Math.round((v0 + 100) * 1e3) / 1e3 }
  else if (ty.startsWith('M_IT')) { mutationStep.value = 1; mutationMin.value = 0; mutationMax.value = 10000 }
}

// === Virtual scroll (same pattern as master DataTable) ===
const ROW_HEIGHT = 28
const OVERSCAN = 10
const scrollTop = ref(0)
const containerHeight = ref(400)

// === Rebuild display array from dataMap + update category counts ===
function updateDisplay() {
  const arr = Array.from(dataMap.values())
  arr.sort((a, b) => a.ioa - b.ioa)
  displayPoints.value = arr
  // Compute realtime category counts — backend returns Chinese category names directly
  const counts = new Map<string, number>()
  for (const p of arr) {
    counts.set(p.category, (counts.get(p.category) || 0) + 1)
  }
  categoryCounts.value = counts
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

// 用后端返回的完整列表替换 dataMap，避免删除/重建 server 等场景下
// 旧条目残留累加（前端 server_id 复用时 watcher 不触发 reset）。
// Merge one incremental point into dataMap, flashing it if the value moved.
// `flashNew` 控制"首次出现的点"是否高亮:切站后的首批加载,全部点都是新点
// 但并非值变化,不应逐点挂 3s setTimeout(2000 点/类型时会瞬时数千个定时器,
// 拖垮前端)。增量轮询中 flashNew=true,新增点仍会闪。
function mergePoint(p: DataPointInfo, flashNew: boolean) {
  const key = pointKey(p.ioa, p.asdu_type)
  const old = dataMap.get(key)
  if (old ? old.value !== p.value : flashNew) markChanged(key)
  dataMap.set(key, p)
}

let loadInFlight = false
// Incremental fetch: pulls only points changed since `lastSeq` instead of the
// whole (up to 80k-row) table every tick. `changed_since` cannot express
// deletions, so a `total_count` mismatch triggers one full resync from seq 0.
async function loadDataPoints() {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  if (!srvId || ca === null) return
  // Guard against overlapping polls: a slow IPC round-trip must not let the
  // 2s timer (or a watcher) stack a second concurrent fetch.
  if (loadInFlight) return
  loadInFlight = true
  try {
    // 首批加载(切站后 dataMap 空)不给新点挂高亮,避免定时器风暴。
    const initialLoad = dataMap.size === 0
    const resp = await invoke<IncrementalDataResponse>('list_data_points_since', {
      serverId: srvId,
      commonAddress: ca,
      sinceSeq: lastSeq,
    })
    for (const p of resp.points) mergePoint(p, !initialLoad)
    lastSeq = resp.seq
    let changed = resp.points.length > 0

    if (dataMap.size !== resp.total_count) {
      // A point was removed — rebuild from scratch, diffing against the
      // previous cache so unchanged rows do not all flash.
      const prev = dataMap
      // 若上一份缓存本就为空(首批即走 resync),新点同样不闪。
      const flashNew = prev.size > 0
      dataMap = new Map()
      const full = await invoke<IncrementalDataResponse>('list_data_points_since', {
        serverId: srvId,
        commonAddress: ca,
        sinceSeq: 0,
      })
      for (const p of full.points) {
        const key = pointKey(p.ioa, p.asdu_type)
        const old = prev.get(key)
        if (old ? old.value !== p.value : flashNew) markChanged(key)
        dataMap.set(key, p)
      }
      lastSeq = full.seq
      changed = true
    }

    if (changed) updateDisplay()
  } catch (e) {
    console.error('Failed to load data points:', e)
  } finally {
    loadInFlight = false
  }
}

// === Watchers ===
watch([selectedServerId, selectedCA], async ([, ], [, ]) => {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  if (!srvId || ca === null) {
    // Cleared selection
    dataMap = new Map()
    lastSeq = 0
    displayPoints.value = []
    currentServerId = null
    currentCA = null
    changedKeys.value.clear()
    for (const t of changeTimers.values()) clearTimeout(t)
    changeTimers.clear()
    selectedRows.value = []
    emitSelection()
    return
  }
  // Only reset if server or CA actually changed
  if (srvId !== currentServerId || ca !== currentCA) {
    dataMap = new Map()
    lastSeq = 0
    displayPoints.value = []
    currentServerId = srvId
    currentCA = ca
    changedKeys.value.clear()
    for (const t of changeTimers.values()) clearTimeout(t)
    changeTimers.clear()
    selectedRows.value = []
    emitSelection()
  }
  await loadDataPoints()
})

watch(dataRefreshKey, () => {
  if (currentServerId && currentCA !== null) {
    loadDataPoints()
  }
})

// 切换站 / 分类时清空搜索框，避免上一次的关键字残留把新视图过滤成空集
watch([selectedServerId, selectedCA, selectedCategory], () => {
  searchQuery.value = ''
})

// === Auto-polling: refresh data points every 2s to pick up control command changes ===
let pollTimer: ReturnType<typeof setInterval> | null = null

function startPolling() {
  stopPolling()
  pollTimer = setInterval(() => {
    if (currentServerId && currentCA !== null) {
      loadDataPoints()
      refreshActiveMutations()
    }
  }, 2000)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

onMounted(() => { startPolling() })

onUnmounted(() => {
  stopPolling()
  for (const t of changeTimers.values()) clearTimeout(t)
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
})

// 按 asdu_type 前缀判分类，对 reactivity / HMR 错位下后端 category 字段
// 失配也能稳定工作；时标版本 (Tx) 与不带时标 (Nx) 归同一分类。
const CATEGORY_TYPE_PREFIXES: Record<string, string[]> = {
  '单点 (SP)': ['M_SP_'],
  '双点 (DP)': ['M_DP_'],
  '步位置 (ST)': ['M_ST_'],
  '位串 (BO)': ['M_BO_'],
  '归一化 (ME_NA)': ['M_ME_NA_', 'M_ME_TD_'],
  '标度化 (ME_NB)': ['M_ME_NB_', 'M_ME_TE_'],
  '浮点 (ME_NC)': ['M_ME_NC_', 'M_ME_TF_'],
  '累计量 (IT)': ['M_IT_'],
}

// === Filtered points ===
const filteredPoints = computed(() => {
  let pts = displayPoints.value
  if (selectedCategory.value) {
    const prefixes = CATEGORY_TYPE_PREFIXES[selectedCategory.value]
    if (prefixes?.length) {
      pts = pts.filter(p => prefixes.some(pre => p.asdu_type.startsWith(pre)))
    } else {
      pts = pts.filter(p => p.category === selectedCategory.value)
    }
  }
  const q = searchQuery.value.trim()
  if (!q) return pts
  if (/^\d+$/.test(q)) {
    const num = Number(q)
    return pts.filter(p => p.ioa === num || p.ioa.toString().includes(q))
  }
  const lower = q.toLowerCase()
  return pts.filter(p =>
    p.name.toLowerCase().includes(lower)
    || p.asdu_type.toLowerCase().includes(lower)
  )
})

// Virtual scroll state
const totalHeight = computed(() => filteredPoints.value.length * ROW_HEIGHT)
const visibleStart = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN))
const visibleEnd = computed(() => {
  const count = Math.ceil(containerHeight.value / ROW_HEIGHT) + OVERSCAN * 2
  return Math.min(filteredPoints.value.length, visibleStart.value + count)
})
const visibleRows = computed(() => filteredPoints.value.slice(visibleStart.value, visibleEnd.value))
const offsetY = computed(() => visibleStart.value * ROW_HEIGHT)

// Coalesce scroll events into one update per animation frame — the raw event
// fires far more often than the screen refreshes, and each write retriggers
// the virtual-scroll computeds.
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

function isSelected(point: DataPointInfo): boolean {
  return selectedRows.value.some(r => r.ioa === point.ioa)
}

function selectRow(e: MouseEvent, point: DataPointInfo) {
  const list = filteredPoints.value
  const idx = list.indexOf(point)
  const isCtrl = e.ctrlKey || e.metaKey

  if (e.shiftKey && lastClickedIndex.value >= 0) {
    const start = Math.min(lastClickedIndex.value, idx)
    const end = Math.max(lastClickedIndex.value, idx)
    selectedRows.value = list.slice(start, end + 1)
  } else if (isCtrl) {
    if (isSelected(point)) {
      selectedRows.value = selectedRows.value.filter(r => r.ioa !== point.ioa)
    } else {
      selectedRows.value = [...selectedRows.value, point]
    }
    lastClickedIndex.value = idx
  } else {
    selectedRows.value = [point]
    lastClickedIndex.value = idx
  }

  emitSelection()
}

function emitSelection() {
  // 同 IOA 上挂着多种 ASDU 类型 (NA + TB), 必须把 asdu_type 一并传给上层,
  // 否则 ValuePanel 无法定位到准确的那一行。
  const points = selectedRows.value.map(r => ({
    ioa: r.ioa,
    asdu_type: r.asdu_type,
    value: r.value,
  }))
  emit('point-select', points)
}

function handleTableKeydown(e: KeyboardEvent) {
  if (editingCell.value) return

  if ((e.key === 'Delete' || e.key === 'Backspace') && selectedRows.value.length > 0) {
    e.preventDefault()
    deleteSelectedPoints()
    return
  }

  const list = filteredPoints.value
  if (list.length === 0) return

  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    let currentIdx = -1
    if (selectedRows.value.length > 0) {
      const last = selectedRows.value[selectedRows.value.length - 1]
      currentIdx = list.findIndex(r => r.ioa === last.ioa)
    }

    let nextIdx: number
    if (e.key === 'ArrowDown') {
      nextIdx = currentIdx < list.length - 1 ? currentIdx + 1 : currentIdx
    } else {
      nextIdx = currentIdx > 0 ? currentIdx - 1 : 0
    }

    if (nextIdx >= 0 && nextIdx < list.length) {
      selectedRows.value = [list[nextIdx]]
      lastClickedIndex.value = nextIdx
      emitSelection()

      nextTick(() => {
        const container = scrollContainer.value
        if (!container) return
        const rows = container.querySelectorAll('tbody tr')
        if (rows[nextIdx]) {
          rows[nextIdx].scrollIntoView({ block: 'nearest' })
        }
      })
    }
  }
}

function startEdit(point: DataPointInfo) {
  editingCell.value = { ioa: point.ioa, asduType: point.asdu_type }
  editValue.value = point.value
}

async function commitEdit() {
  if (!editingCell.value || !selectedServerId.value || currentCA === null) return
  const { ioa, asduType } = editingCell.value
  const value = editValue.value
  editingCell.value = null

  try {
    await invoke('update_data_point', {
      serverId: selectedServerId.value,
      commonAddress: currentCA,
      ioa,
      asduType,
      value,
    })
    await loadDataPoints()
  } catch (e) {
    await showAlert(String(e))
  }
}

function cancelEdit() {
  editingCell.value = null
}

function handleEditKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    commitEdit()
  } else if (e.key === 'Escape') {
    cancelEdit()
  }
}

function onPointAdded() {
  dataRefreshKey.value++
}

function onBatchWritten() {
  showBatchWriteModal.value = false
  loadDataPoints()
}

// Context menu for delete — acts on the current selection, not just the
// right-clicked row, so multi-select (ctrl/shift) can be batch-deleted.
const contextMenu = ref({ show: false, x: 0, y: 0 })

function showContextMenu(e: MouseEvent, point: DataPointInfo) {
  e.preventDefault()
  // 标准右键行为:右键未选中的行时,先把它设为唯一选中项;
  // 右键已在多选内的行则保留整个选择,以便批量删除。
  if (!isSelected(point)) {
    selectedRows.value = [point]
    lastClickedIndex.value = filteredPoints.value.indexOf(point)
    emitSelection()
  }
  contextMenu.value = { show: true, x: e.clientX, y: e.clientY }
  // 按右键点位类型预填步长/上下限,并对离散量强制翻转模式。
  applyMutationDefaults(point)
  if (!pointSupportsStep(point.asdu_type)) mutationMode.value = 'flip'
  refreshActiveMutations()
}

function closeContextMenu() {
  contextMenu.value.show = false
}

const selectedCount = computed(() => selectedRows.value.length)

// 删除当前选中的所有点位(单选即删一个)。改走批量命令,一次锁内删除;
// 乐观地立即从本地 dataMap 移除并重绘,避免与 2s 轮询的 in-flight 竞态
// 把删除"吞掉"导致看似无效。
async function deleteSelectedPoints() {
  contextMenu.value.show = false
  if (!selectedServerId.value || currentCA === null) return
  const targets = selectedRows.value.map(r => ({ ioa: r.ioa, asdu_type: r.asdu_type }))
  if (targets.length === 0) return
  try {
    await invoke('batch_remove_data_points', {
      serverId: selectedServerId.value,
      commonAddress: currentCA,
      points: targets,
    })
    for (const t of targets) dataMap.delete(pointKey(t.ioa, t.asdu_type))
    selectedRows.value = []
    emitSelection()
    updateDisplay()
    await loadDataPoints()
  } catch (e) {
    await showAlert(String(e))
  }
}

// 拉取当前 (server, CA) 的活跃周期变位集合。
async function refreshActiveMutations() {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  if (!srvId || ca === null) { activeMutations.value = new Map(); return }
  try {
    const list = await invoke<PointMutationInfo[]>('list_point_mutations', {
      serverId: srvId,
      commonAddress: ca,
    })
    activeMutations.value = new Map(list.map(m => [pointKey(m.ioa, m.asdu_type), m.mode]))
  } catch (e) {
    console.error('Failed to load point mutations:', e)
  }
}

// 选中点位里是否有正在变位的（决定是否显示「停止」项）。
const anySelectedMutating = computed(() =>
  selectedRows.value.some(r => activeMutations.value.has(pointKey(r.ioa, r.asdu_type)))
)

// 选区里是否有支持递增/递减的点(决定是否在菜单显示变位方式与步长/上下限)。
const selectionSupportsStep = computed(() =>
  selectedRows.value.some(r => pointSupportsStep(r.asdu_type))
)

async function startMutationForSelection() {
  contextMenu.value.show = false
  const srvId = selectedServerId.value
  if (!srvId || currentCA === null) return
  const period = Math.min(60000, Math.max(50, mutationPeriod.value || 1000))
  const targets = selectedRows.value.map(r => ({ ioa: r.ioa, asdu_type: r.asdu_type }))
  // 离散量不支持递增/递减,后端会回退翻转;此处按各点类型决定实际传入的模式。
  const step = mutationStep.value || 1
  const min = mutationMin.value
  const max = mutationMax.value
  try {
    for (const tgt of targets) {
      const mode: MutationMode = pointSupportsStep(tgt.asdu_type) ? mutationMode.value : 'flip'
      await invoke('start_point_mutation', {
        serverId: srvId,
        commonAddress: currentCA,
        ioa: tgt.ioa,
        asduType: tgt.asdu_type,
        periodMs: period,
        mode,
        step,
        min,
        max,
      })
    }
    await refreshActiveMutations()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function stopMutationForSelection() {
  contextMenu.value.show = false
  const srvId = selectedServerId.value
  if (!srvId || currentCA === null) return
  const targets = selectedRows.value.map(r => ({ ioa: r.ioa, asdu_type: r.asdu_type }))
  try {
    for (const tgt of targets) {
      await invoke('stop_point_mutation', {
        serverId: srvId,
        commonAddress: currentCA,
        ioa: tgt.ioa,
        asduType: tgt.asdu_type,
      })
    }
    await refreshActiveMutations()
  } catch (e) {
    await showAlert(String(e))
  }
}

// Allow parent to directly trigger data load (bypasses async watch timing issues)
defineExpose({ loadData: loadDataPoints })
</script>

<template>
  <div class="data-point-table" @click="closeContextMenu">
    <div class="table-header-bar">
      <span class="table-title">
        {{ selectedCategory ? localizeCategoryLabel(selectedCategory) : t('table.allPoints') }}
      </span>
      <input
        v-model="searchQuery"
        class="search-input"
        type="text"
        :placeholder="t('table.searchPlaceholder')"
      />
      <button
        class="add-btn"
        :disabled="!selectedServerId || currentCA === null"
        @click="showAddModal = true"
        :title="t('table.addPointTitle')"
      >+</button>
      <button
        class="add-btn batch"
        :disabled="!selectedServerId || currentCA === null"
        @click="showBatchModal = true"
        :title="t('table.batchAdd')"
      >{{ t('table.batchAdd') }}</button>
      <button
        class="add-btn batch"
        :disabled="!selectedServerId || currentCA === null || displayPoints.length === 0"
        @click="showBatchWriteModal = true"
        :title="t('batchWrite.title')"
      >{{ t('table.batchWrite') }}</button>
      <span class="table-count">{{ filteredPoints.length }} {{ t('table.countSuffix') }}</span>
    </div>

    <EmptyState
      v-if="!selectedServerId || currentCA === null"
      :title="t('table.chooseStation')"
      :hint="t('table.chooseStationHint')"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <rect x="9" y="3" width="6" height="5" rx="1" />
        <rect x="2.5" y="16" width="6" height="5" rx="1" />
        <rect x="15.5" y="16" width="6" height="5" rx="1" />
        <path d="M12 8v3.5M5.5 16v-2.5a1 1 0 0 1 1-1h11a1 1 0 0 1 1 1V16" />
      </svg>
    </EmptyState>
    <EmptyState
      v-else-if="filteredPoints.length === 0"
      :title="t('table.noPoints')"
      :hint="t('table.noPointsHint')"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="4" width="18" height="16" rx="1.5" />
        <path d="M3 10h18M3 15h18M11 4v16" />
      </svg>
    </EmptyState>

    <div
      v-else
      ref="scrollContainer"
      class="table-scroll-container"
      tabindex="0"
      @scroll="onScroll"
      @keydown="handleTableKeydown"
    >
      <!-- Fixed header -->
      <table class="table">
        <thead>
          <tr>
            <th class="col-ioa">IOA</th>
            <th class="col-type">{{ t('table.asduTypeCol') }}</th>
            <th class="col-name">{{ t('table.nameCol') }}</th>
            <th class="col-value">{{ t('table.valueCol') }}</th>
            <th class="col-quality"><span class="th-quality">{{ t('table.qualityCol') }}<QualityLegend /></span></th>
            <th class="col-timestamp">{{ t('table.timestampCol') }}</th>
          </tr>
        </thead>
      </table>
      <!-- Virtual scroll body -->
      <div v-if="filteredPoints.length > 0" :style="{ height: totalHeight + 'px', position: 'relative' }">
        <table class="table table-body" :style="{ transform: `translateY(${offsetY}px)` }">
          <tbody>
            <tr
              v-for="point in visibleRows"
              :key="point.ioa + ':' + point.asdu_type"
              :class="{
                selected: isSelected(point),
                'value-changed': changedKeys.has(point.ioa + ':' + point.asdu_type),
                mutating: activeMutations.has(point.ioa + ':' + point.asdu_type)
              }"
              @click="selectRow($event, point)"
              @contextmenu.prevent="showContextMenu($event, point)"
            >
              <td class="col-ioa">
                <template v-if="activeMutations.has(point.ioa + ':' + point.asdu_type)">
                  <span class="mut-dot" />
                  <span
                    class="mut-mode"
                    :title="mutationModeLabel(activeMutations.get(point.ioa + ':' + point.asdu_type))"
                  >{{ mutationGlyph(activeMutations.get(point.ioa + ':' + point.asdu_type)) }}</span>
                </template>{{ point.ioa }}
              </td>
              <td class="col-type">{{ point.asdu_type }}</td>
              <td class="col-name">{{ point.name || '-' }}</td>
              <td :class="['col-value', { 'value-highlight': changedKeys.has(point.ioa + ':' + point.asdu_type) }]" @dblclick.stop="startEdit(point)">
                <template v-if="editingCell?.ioa === point.ioa && editingCell?.asduType === point.asdu_type">
                  <input
                    v-model="editValue"
                    class="edit-input"
                    type="text"
                    autofocus
                    @blur="commitEdit"
                    @keydown="handleEditKeydown"
                    @click.stop
                  />
                </template>
                <template v-else>
                  <span class="value-text">{{ point.value }}</span>
                </template>
              </td>
              <td class="col-quality">
                <QualityIndicator
                  :quality="{ ov: point.quality_ov, bl: point.quality_bl, sb: point.quality_sb, nt: point.quality_nt, iv: point.quality_iv }"
                  :show-ov="point.asdu_type.startsWith('M_ME')"
                  :show-help="false"
                  compact
                />
              </td>
              <td class="col-timestamp">{{ point.timestamp || '-' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- Context Menu -->
    <div
      v-if="contextMenu.show"
      class="context-menu"
      :style="{ top: contextMenu.y + 'px', left: contextMenu.x + 'px' }"
      @click.stop
    >
      <label class="context-menu-period" @click.stop>
        <span>{{ t('table.mutationPeriod') }}</span>
        <input
          type="number"
          min="50"
          max="60000"
          v-model.number="mutationPeriod"
          @keydown.enter="startMutationForSelection"
          @click.stop
        />
        <span class="cm-unit">ms</span>
      </label>
      <div v-if="selectionSupportsStep" class="context-menu-mode" @click.stop>
        <span class="cm-mode-label">{{ t('table.mutationMode') }}</span>
        <button :class="{ active: mutationMode === 'flip' }" @click.stop="mutationMode = 'flip'">{{ t('table.modeFlip') }}</button>
        <button :class="{ active: mutationMode === 'increment' }" @click.stop="mutationMode = 'increment'">{{ t('table.modeIncrement') }}</button>
        <button :class="{ active: mutationMode === 'decrement' }" @click.stop="mutationMode = 'decrement'">{{ t('table.modeDecrement') }}</button>
      </div>
      <template v-if="selectionSupportsStep && mutationMode !== 'flip'">
        <label class="context-menu-period" @click.stop>
          <span>{{ t('table.mutationStep') }}</span>
          <input type="number" v-model.number="mutationStep" @keydown.enter="startMutationForSelection" @click.stop />
        </label>
        <label class="context-menu-period" @click.stop>
          <span>{{ t('table.mutationMin') }}</span>
          <input type="number" v-model.number="mutationMin" @keydown.enter="startMutationForSelection" @click.stop />
        </label>
        <label class="context-menu-period" @click.stop>
          <span>{{ t('table.mutationMax') }}</span>
          <input type="number" v-model.number="mutationMax" @keydown.enter="startMutationForSelection" @click.stop />
        </label>
      </template>
      <div class="context-menu-item" @click="startMutationForSelection">
        {{ selectedCount > 1 ? `${t('table.startMutation')} (${selectedCount})` : t('table.startMutation') }}
      </div>
      <div v-if="anySelectedMutating" class="context-menu-item" @click="stopMutationForSelection">
        {{ t('table.stopMutation') }}
      </div>
      <div class="context-menu-sep" />
      <div class="context-menu-item danger" @click="deleteSelectedPoints">
        {{ selectedCount > 1 ? `${t('table.deletePoint')} (${selectedCount})` : t('table.deletePoint') }}
      </div>
    </div>

    <!-- Add Data Point Modal -->
    <DataPointModal
      :visible="showAddModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      @close="showAddModal = false"
      @added="onPointAdded"
    />

    <!-- Batch Add Modal -->
    <BatchAddModal
      :visible="showBatchModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :existing-points="showBatchModal ? displayPoints : []"
      @close="showBatchModal = false"
      @added="onPointAdded"
    />

    <!-- Batch Write Modal -->
    <BatchWriteModal
      :visible="showBatchWriteModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :existing-points="showBatchWriteModal ? displayPoints : []"
      :default-type="batchWriteDefaultType"
      @close="showBatchWriteModal = false"
      @written="onBatchWritten"
    />
  </div>
</template>

<style scoped>
.data-point-table {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.table-header-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--c-surface0);
  flex-shrink: 0;
}

.table-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--c-text);
  white-space: nowrap;
}

.search-input {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-text);
  font-size: 12px;
  outline: none;
}

.search-input:focus {
  border-color: var(--c-blue);
}

.search-input::placeholder {
  color: var(--c-overlay0);
}

.add-btn {
  padding: 2px 8px;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
  color: var(--c-green);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  line-height: 1;
}

.add-btn.batch {
  font-size: 11px;
  font-weight: 400;
}

.add-btn:hover:not(:disabled) {
  background: var(--c-surface1);
}

.add-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.table-count {
  font-size: 11px;
  color: var(--c-overlay0);
  white-space: nowrap;
}

.table-scroll-container {
  flex: 1;
  overflow-y: auto;
  outline: none;
}

.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  table-layout: fixed;
}

.table thead {
  position: sticky;
  top: 0;
  z-index: 1;
}

.table th {
  background: var(--c-base);
  color: var(--c-overlay0);
  font-weight: 500;
  text-align: left;
  padding: 6px 10px;
  border-bottom: 1px solid var(--c-surface0);
  position: sticky;
  top: 0;
}

.table td {
  padding: 5px 10px;
  border-bottom: 1px solid var(--c-base);
  cursor: pointer;
}

.table tbody tr:hover {
  background: var(--c-base);
}

.table tbody tr.selected {
  background: var(--c-blue);
  color: var(--c-base);
}

.table tbody tr.value-changed {
  background: rgba(250, 179, 135, 0.15);
}

.col-ioa {
  font-family: var(--font-mono);
  width: 70px;
  color: var(--c-blue);
}

.table tbody tr.selected .col-ioa {
  color: var(--c-base);
}

.col-type {
  width: 100px;
}

.col-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-value {
  width: 120px;
  font-family: var(--font-mono);
  transition: color 0.3s;
}

.value-text {
  font-family: var(--font-mono);
}

.col-value.value-highlight {
  color: var(--c-peach);
  font-weight: 700;
}

.th-quality {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.col-quality {
  width: 96px;
  font-weight: 600;
  font-size: 11px;
}

.col-timestamp {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--c-overlay0);
  width: 100px;
}

.table tbody tr.selected .col-timestamp {
  color: var(--c-surface1);
}

.edit-input {
  width: 90px;
  padding: 2px 6px;
  background: var(--c-base);
  border: 1px solid var(--c-blue);
  border-radius: 3px;
  color: var(--c-text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
}

/* Context Menu */
.context-menu {
  position: fixed;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  z-index: 999;
  min-width: 140px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.context-menu-item {
  padding: 8px 14px;
  font-size: 13px;
  color: var(--c-text);
  cursor: pointer;
  border-radius: 6px;
}

.context-menu-item:hover {
  background: var(--c-surface0);
}

.context-menu-item.danger {
  color: var(--c-red);
}

.context-menu-item.danger:hover {
  background: #3d2a30;
}

.context-menu-period {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  font-size: 12px;
  color: var(--c-subtext0);
}
.context-menu-period input {
  width: 64px;
  height: 22px;
  padding: 0 6px;
  background: var(--c-base);
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  color: var(--c-text);
  font: 500 12px/1 ui-monospace, "SF Mono", Menlo, monospace;
  text-align: right;
}
.context-menu-period input:focus {
  outline: none;
  border-color: var(--c-blue);
}
.cm-unit {
  color: var(--c-overlay0);
  font-size: 11px;
}
.context-menu-sep {
  height: 1px;
  margin: 4px 0;
  background: var(--c-surface0);
}
.mut-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  margin-right: 5px;
  border-radius: 50%;
  background: var(--c-green);
  vertical-align: middle;
  animation: mut-pulse 1s ease-in-out infinite;
}
@keyframes mut-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}
/* 变位方式标识(数据表行内,紧跟脉冲点)。 */
.mut-mode {
  display: inline-block;
  margin-right: 4px;
  font-size: 11px;
  font-weight: 700;
  color: var(--c-green);
  vertical-align: middle;
}
/* 右键菜单内的变位方式切换。 */
.context-menu-mode {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 12px;
  font-size: 12px;
}
.cm-mode-label {
  margin-right: 2px;
  color: var(--c-subtext0);
}
.context-menu-mode button {
  flex: 1;
  height: 22px;
  padding: 0 4px;
  background: var(--c-base);
  border: 1px solid var(--c-surface0);
  border-radius: 4px;
  color: var(--c-subtext0);
  font-size: 11px;
  cursor: pointer;
}
.context-menu-mode button:hover {
  border-color: var(--c-blue);
}
.context-menu-mode button.active {
  background: var(--c-blue);
  border-color: var(--c-blue);
  color: var(--c-base);
  font-weight: 600;
}
</style>
