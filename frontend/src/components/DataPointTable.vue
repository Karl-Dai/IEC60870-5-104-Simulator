<script setup lang="ts">
import { ref, inject, watch, computed, nextTick, onMounted, onUnmounted, shallowRef, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert } from '@shared/composables/useDialog'
import type {
  DataPointInfo,
  DataPointValueSnapshot,
  IncrementalDataResponse,
  PointMutationInfo,
  PointMutationRow,
  MutationMode,
} from '../types'
import { formatDataPointValue } from '@shared/utils/dataPointValue'
import DataPointModal from './DataPointModal.vue'
import BatchAddModal from './BatchAddModal.vue'
import BatchWriteModal from './BatchWriteModal.vue'
import BatchControlOptionsModal from './BatchControlOptionsModal.vue'
import BatchTypeMigrationModal from './BatchTypeMigrationModal.vue'
import SimulationSettingsDrawer from './SimulationSettingsDrawer.vue'
import { findAsduTypeOption, formatAsduTypeWithId } from '../constants/asduTypes'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import EmptyState from '@shared/components/EmptyState.vue'
import QualityIndicator from '@shared/components/QualityIndicator.vue'
import QualityLegend from '@shared/components/QualityLegend.vue'

const { t } = useI18n()
const { showAlert } = inject<{ showAlert: typeof ShowAlert }>(dialogKey)!

const emit = defineEmits<{
  (e: 'point-select', points: { ioa: number; asdu_type: string; category: string; value: string }[]): void
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
// 每次切换 server / CA 都递增。所有异步轮询响应必须携带并核对该 epoch，
// 防止旧站点的迟到响应污染新站点。
let selectionEpoch = 0
let componentUnmounted = false
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
const showEditModal = ref(false)
const editingPointDefinition = ref<DataPointInfo | null>(null)
const showBatchModal = ref(false)
const showBatchWriteModal = ref(false)
const showBatchControlModal = ref(false)
const showBatchTypeModal = ref(false)
const showSimulationDrawer = ref(false)
type SortKey = 'ioa' | 'asdu_type' | 'name' | 'value'
const sortKey = ref<SortKey>('ioa')
const sortDirection = ref<'asc' | 'desc'>('asc')
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
// 当前 (server, CA) 下正在周期变位的点位 → 变位方式及完整任务参数。
const activeMutations = ref<Map<string, MutationMode>>(new Map())
const activeMutationDetails = ref<Map<string, PointMutationInfo>>(new Map())

// 变位方式的图标 / 本地化标签（数据表行内显示）。
function mutationGlyph(mode: MutationMode | undefined) {
  return mode === 'increment' ? '↑' : mode === 'decrement' ? '↓' : '⇅'
}
function mutationModeLabel(mode: MutationMode | undefined) {
  if (mode === 'increment') return t('table.modeIncrement')
  if (mode === 'decrement') return t('table.modeDecrement')
  return t('table.modeFlip')
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
  // Compute realtime category counts — backend returns snake_case category keys
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
// Merge one slow-poll point into a target map, flashing it if the value moved.
// `flashNew` 控制"首次出现的点"是否高亮:切站后的首批加载,全部点都是新点
// 但并非值变化,不应逐点挂 3s setTimeout(2000 点/类型时会瞬时数千个定时器,
// 拖垮前端)。增量轮询中 flashNew=true,新增点仍会闪。
//
// 活动变位点的 value / quality / timestamp 由更快的 targeted poll 持有；
// 2s 慢响应即使后到也只能刷新 name/category/mapping 等静态定义，不能回滚动态值。
function mergeSlowPoint(
  target: Map<string, DataPointInfo>,
  p: DataPointInfo,
  old: DataPointInfo | undefined,
  flashNew: boolean,
) {
  const key = pointKey(p.ioa, p.asdu_type)
  if (old && activeMutations.value.has(key)) {
    target.set(key, {
      ...p,
      value: old.value,
      quality_ov: old.quality_ov,
      quality_bl: old.quality_bl,
      quality_sb: old.quality_sb,
      quality_nt: old.quality_nt,
      quality_iv: old.quality_iv,
      timestamp: old.timestamp,
    })
    return
  }
  if (old ? old.value !== p.value : flashNew) markChanged(key)
  target.set(key, p)
}

let loadInFlight = false
let loadPending = false
// Incremental fetch: pulls only points changed since `lastSeq` instead of the
// whole (up to 80k-row) table every tick. `changed_since` cannot express
// deletions, so a `total_count` mismatch triggers one full resync from seq 0.
async function loadDataPoints() {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  const epoch = selectionEpoch
  if (!srvId || ca === null || !isCurrentSelection(srvId, ca, epoch)) return
  // Guard against overlapping polls: a slow IPC round-trip must not let the
  // 2s timer (or a watcher) stack a second concurrent fetch. A trigger that
  // arrives while busy is remembered: after the current request settles we
  // always reload whichever server / CA is current at that moment.
  if (loadInFlight) {
    loadPending = true
    return
  }
  loadInFlight = true
  try {
    // 首批加载(切站后 dataMap 空)不给新点挂高亮,避免定时器风暴。
    const initialLoad = dataMap.size === 0
    const resp = await invoke<IncrementalDataResponse>('list_data_points_since', {
      serverId: srvId,
      commonAddress: ca,
      sinceSeq: lastSeq,
    })
    if (!isCurrentSelection(srvId, ca, epoch)) return
    for (const p of resp.points) {
      const key = pointKey(p.ioa, p.asdu_type)
      mergeSlowPoint(dataMap, p, dataMap.get(key), !initialLoad)
    }
    lastSeq = resp.seq
    let changed = resp.points.length > 0

    if (dataMap.size !== resp.total_count) {
      // A point was removed — rebuild from scratch, diffing against the
      // previous cache so unchanged rows do not all flash.
      const prev = dataMap
      // 若上一份缓存本就为空(首批即走 resync),新点同样不闪。
      const flashNew = prev.size > 0
      const full = await invoke<IncrementalDataResponse>('list_data_points_since', {
        serverId: srvId,
        commonAddress: ca,
        sinceSeq: 0,
      })
      if (!isCurrentSelection(srvId, ca, epoch)) return
      const next = new Map<string, DataPointInfo>()
      for (const p of full.points) {
        const key = pointKey(p.ioa, p.asdu_type)
        // prev 在 full await 期间仍是当前 dataMap；targeted poll 若先返回，
        // 会把最新动态字段写进这里，重建时据此保留。
        mergeSlowPoint(next, p, prev.get(key), flashNew)
      }
      dataMap = next
      lastSeq = full.seq
      changed = true
    }

    if (changed) updateDisplay()
  } catch (e) {
    if (isCurrentSelection(srvId, ca, epoch)) {
      console.error('Failed to load data points:', e)
    }
  } finally {
    loadInFlight = false
    if (loadPending) {
      loadPending = false
      void loadDataPoints()
    }
  }
}

// === Watchers ===
watch([selectedServerId, selectedCA], async ([, ], [, ]) => {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  if (!srvId || ca === null) {
    // Cleared selection
    selectionEpoch++
    clearActiveMutationState()
    dataMap = new Map()
    lastSeq = 0
    displayPoints.value = []
    categoryCounts.value = new Map()
    currentServerId = null
    currentCA = null
    changedKeys.value.clear()
    for (const t of changeTimers.values()) clearTimeout(t)
    changeTimers.clear()
    selectedRows.value = []
    showSimulationDrawer.value = false
    emitSelection()
    return
  }
  // Only reset if server or CA actually changed
  if (srvId !== currentServerId || ca !== currentCA) {
    selectionEpoch++
    clearActiveMutationState()
    dataMap = new Map()
    lastSeq = 0
    displayPoints.value = []
    categoryCounts.value = new Map()
    currentServerId = srvId
    currentCA = ca
    changedKeys.value.clear()
    for (const t of changeTimers.values()) clearTimeout(t)
    changeTimers.clear()
    selectedRows.value = []
    showSimulationDrawer.value = false
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
  componentUnmounted = true
  selectionEpoch++
  stopPolling()
  clearActiveMutationState()
  for (const t of changeTimers.values()) clearTimeout(t)
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
})

// 按 asdu_type 前缀判分类，对 reactivity / HMR 错位下后端 category 字段
// 失配也能稳定工作；键为后端 snake_case 分类键(与 ConnectionTree CATEGORIES 一致),
// 时标版本 (Tx) 与不带时标 (Nx) 归同一分类。
const CATEGORY_TYPE_PREFIXES: Record<string, string[]> = {
  single_point: ['M_SP_'],
  double_point: ['M_DP_'],
  step_position: ['M_ST_'],
  bitstring: ['M_BO_'],
  normalized_measured: ['M_ME_NA_', 'M_ME_ND_', 'M_ME_TA_', 'M_ME_TD_'],
  scaled_measured: ['M_ME_NB_', 'M_ME_TB_', 'M_ME_TE_'],
  float_measured: ['M_ME_NC_', 'M_ME_TC_', 'M_ME_TF_'],
  integrated_totals: ['M_IT_'],
  single_command: ['C_SC_'],
  double_command: ['C_DC_'],
  step_command: ['C_RC_'],
  bitstring_command: ['C_BO_'],
  normalized_setpoint: ['C_SE_NA_', 'C_SE_TA_'],
  scaled_setpoint: ['C_SE_NB_', 'C_SE_TB_'],
  float_setpoint: ['C_SE_NC_', 'C_SE_TC_'],
}

// === 「变位同步上送 TB」在类型列的可视化(issue #28:Not Sync Type ID?)===
// 开关不会改点位自身的 TypeID(NA 帧照发),只是变位时额外追加一帧 TB;
// 表格在 NA 类型后补一个 "+TB" 徽标,让派生行为可见,消除"没同步"的误解。
// NA 显示名 → [同步分类键, TB 显示名, TB TypeID],与 slave.rs SyncTbByCategory 一致。
const SYNC_TB_DERIVE: Record<string, [string, string, number]> = {
  'M_SP_NA_1': ['sp', 'M_SP_TB_1', 30],
  'M_DP_NA_1': ['dp', 'M_DP_TB_1', 31],
  'M_ST_NA_1': ['st', 'M_ST_TB_1', 32],
  'M_BO_NA_1': ['bo', 'M_BO_TB_1', 33],
  'M_ME_NA_1': ['me_na', 'M_ME_TD_1', 34],
  'M_ME_NB_1': ['me_nb', 'M_ME_TE_1', 35],
  'M_ME_NC_1': ['me_nc', 'M_ME_TF_1', 36],
}
const syncTbFlags = ref<Record<string, boolean>>({})
let syncTbLoadEpoch = 0

async function refreshSyncTbFlags() {
  const srvId = selectedServerId.value
  const epoch = ++syncTbLoadEpoch
  if (!srvId) { syncTbFlags.value = {}; return }
  try {
    const ops = await invoke<{ sync_tb_by_category?: Record<string, boolean> }>(
      'get_remote_operation_config', { serverId: srvId },
    )
    if (epoch === syncTbLoadEpoch && selectedServerId.value === srvId) {
      syncTbFlags.value = ops?.sync_tb_by_category ?? {}
    }
  } catch {
    if (epoch === syncTbLoadEpoch && selectedServerId.value === srvId) {
      syncTbFlags.value = {}
    }
  }
}

watch(selectedServerId, () => { refreshSyncTbFlags() }, { immediate: true })
watch(dataRefreshKey, () => { refreshSyncTbFlags() })

// 点位若因 sync-TB 开关会派生 TB 帧,返回 "M_SP_TB_1 (30)";
// 同 IOA 已显式存在 TB 点时派生被抑制(R1 规则,与后端 should_derive_tb 一致)。
function derivedTbLabel(point: DataPointInfo): string | null {
  const entry = SYNC_TB_DERIVE[point.asdu_type]
  if (!entry) return null
  const [flagKey, tbName, tbId] = entry
  if (!syncTbFlags.value[flagKey]) return null
  if (dataMap.has(pointKey(point.ioa, tbName))) return null
  return `${tbName} (${tbId})`
}

// === 同 CASDU 内跨类型重复 IOA 检测(issue #28)===
// 规则:同方向(监视/控制)且不同分类的点共用同一 IOA 视为冲突。
// - NA/TA/TB 同分类变体共用 IOA 是同一信号的不同传输格式,不算;
// - 控制点与监视点同 IOA 是合法配对(兼容自动映射),不算。
// 仅警示不阻断:标红 + ⚠ 提示,用户可直接编辑该点修正 IOA。
const duplicateIoaMap = computed<Map<string, string>>(() => {
  const groups = new Map<string, DataPointInfo[]>()
  for (const p of displayPoints.value) {
    const dir = p.asdu_type.startsWith('C_') ? 'c' : 'm'
    const key = `${dir}:${p.ioa}`
    let arr = groups.get(key)
    if (!arr) groups.set(key, arr = [])
    arr.push(p)
  }
  const flagged = new Map<string, string>()
  for (const pts of groups.values()) {
    if (new Set(pts.map(p => p.category)).size < 2) continue
    for (const p of pts) {
      const others = pts
        .filter(o => o.category !== p.category)
        .map(o => formatAsduTypeWithId(o.asdu_type))
      flagged.set(pointKey(p.ioa, p.asdu_type), others.join(', '))
    }
  }
  return flagged
})

function duplicateIoaTypes(point: DataPointInfo): string | undefined {
  return duplicateIoaMap.value.get(pointKey(point.ioa, point.asdu_type))
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
  if (/^\d+$/.test(q)) {
    const num = Number(q)
    pts = pts.filter(p => p.ioa === num || p.ioa.toString().includes(q))
  } else if (q) {
    const lower = q.toLowerCase()
    pts = pts.filter(p =>
      p.name.toLowerCase().includes(lower)
      || p.asdu_type.toLowerCase().includes(lower)
    )
  }

  const direction = sortDirection.value === 'asc' ? 1 : -1
  return pts
    .map((point, index) => ({ point, index }))
    .sort((left, right) => {
      const a = left.point[sortKey.value]
      const b = right.point[sortKey.value]
      let compared: number
      if (sortKey.value === 'ioa') {
        compared = Number(a) - Number(b)
      } else if (sortKey.value === 'value') {
        const aNumber = Number(a)
        const bNumber = Number(b)
        compared = Number.isFinite(aNumber) && Number.isFinite(bNumber)
          ? aNumber - bNumber
          : String(a).localeCompare(String(b), undefined, { numeric: true })
      } else {
        compared = String(a).localeCompare(String(b), undefined, { numeric: true })
      }
      return compared === 0 ? left.index - right.index : compared * direction
    })
    .map(({ point }) => point)
})

function toggleSort(key: SortKey) {
  if (sortKey.value === key) {
    sortDirection.value = sortDirection.value === 'asc' ? 'desc' : 'asc'
  } else {
    sortKey.value = key
    sortDirection.value = 'asc'
  }
}

function sortGlyph(key: SortKey) {
  if (sortKey.value !== key) return ''
  return sortDirection.value === 'asc' ? '▲' : '▼'
}

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
  const key = pointKey(point.ioa, point.asdu_type)
  return selectedRows.value.some(r => pointKey(r.ioa, r.asdu_type) === key)
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
      const key = pointKey(point.ioa, point.asdu_type)
      selectedRows.value = selectedRows.value.filter(
        r => pointKey(r.ioa, r.asdu_type) !== key,
      )
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

function toggleRowSelection(point: DataPointInfo) {
  const key = pointKey(point.ioa, point.asdu_type)
  if (isSelected(point)) {
    selectedRows.value = selectedRows.value.filter(
      row => pointKey(row.ioa, row.asdu_type) !== key,
    )
  } else {
    selectedRows.value = [...selectedRows.value, point]
  }
  lastClickedIndex.value = filteredPoints.value.indexOf(point)
  emitSelection()
}

function selectFilteredPoints() {
  selectedRows.value = [...filteredPoints.value]
  lastClickedIndex.value = selectedRows.value.length - 1
  emitSelection()
}

function invertFilteredSelection() {
  const selected = new Set(selectedRows.value.map(row => pointKey(row.ioa, row.asdu_type)))
  selectedRows.value = filteredPoints.value.filter(
    point => !selected.has(pointKey(point.ioa, point.asdu_type)),
  )
  lastClickedIndex.value = selectedRows.value.length - 1
  emitSelection()
}

function clearSelection() {
  selectedRows.value = []
  lastClickedIndex.value = -1
  emitSelection()
}

function emitSelection() {
  // 同 IOA 上挂着多种 ASDU 类型 (NA + TB), 必须把 asdu_type 一并传给上层,
  // 否则 ValuePanel 无法定位到准确的那一行。
  const points = selectedRows.value.map(r => ({
    ioa: r.ioa,
    asdu_type: r.asdu_type,
    category: r.category,
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
      currentIdx = list.findIndex(
        r => pointKey(r.ioa, r.asdu_type) === pointKey(last.ioa, last.asdu_type),
      )
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

// 编辑可能改了 IOA(点位在后端换键);增量协议表达不了"旧键消失",
// 强制从 seq=0 全量重建,避免旧 IOA 行残留。
async function onPointEdited(target?: { ioa: number; asdu_type: string }) {
  const selectedKeys = new Set(
    selectedRows.value.map(point => pointKey(point.ioa, point.asdu_type)),
  )
  dataMap = new Map()
  lastSeq = 0
  await loadDataPoints()
  const targetType = target ? findAsduTypeOption(target.asdu_type)?.value : undefined
  selectedRows.value = target
    ? displayPoints.value.filter(point =>
        point.ioa === target.ioa
        && findAsduTypeOption(point.asdu_type)?.value === targetType
      )
    : displayPoints.value.filter(point =>
        selectedKeys.has(pointKey(point.ioa, point.asdu_type))
      )
  emitSelection()
  dataRefreshKey.value++
}

async function handlePointEdited(target: { ioa: number; asdu_type: string }) {
  showEditModal.value = false
  editingPointDefinition.value = null
  await onPointEdited(target)
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
}

function closeContextMenu() {
  contextMenu.value.show = false
}

function editSelectedPoint() {
  const point = selectedRows.value[0]
  if (!point || selectedRows.value.length !== 1) return
  contextMenu.value.show = false
  editingPointDefinition.value = point
  showEditModal.value = true
}

const selectedCount = computed(() => selectedRows.value.length)

// 选区内可批量配置 QU/QL、S/E 的控制点(位串命令不携带这些字段)。
const selectedControlPoints = computed(() =>
  selectedRows.value.filter(r => r.asdu_type.startsWith('C_') && !r.asdu_type.startsWith('C_BO'))
)
const selectedMonitorPoints = computed(() =>
  selectedRows.value.filter(r => !r.asdu_type.startsWith('C_'))
)
const canBatchMigrateType = computed(() => {
  if (selectedMonitorPoints.value.length === 0
    || selectedMonitorPoints.value.length !== selectedRows.value.length) return false
  return new Set(selectedMonitorPoints.value.map(point => point.category)).size === 1
})
const canOpenBatchSettings = computed(() =>
  canBatchMigrateType.value
  || (
    selectedControlPoints.value.length > 0
    && selectedControlPoints.value.length === selectedRows.value.length
  )
)

function openBatchControlOptions() {
  contextMenu.value.show = false
  if (selectedControlPoints.value.length === 0) return
  showBatchControlModal.value = true
}

function openBatchTypeMigration() {
  contextMenu.value.show = false
  if (!canBatchMigrateType.value) return
  showBatchTypeModal.value = true
}

function openBatchSettings() {
  if (canBatchMigrateType.value) {
    openBatchTypeMigration()
  } else if (selectedControlPoints.value.length === selectedRows.value.length) {
    openBatchControlOptions()
  }
}

async function onControlOptionsApplied() {
  showBatchControlModal.value = false
  const selectedKeys = new Set(
    selectedRows.value.map(point => pointKey(point.ioa, point.asdu_type)),
  )
  dataMap = new Map()
  lastSeq = 0
  await loadDataPoints()
  selectedRows.value = displayPoints.value.filter(
    point => selectedKeys.has(pointKey(point.ioa, point.asdu_type)),
  )
  emitSelection()
  dataRefreshKey.value++
}

async function onTypeMigrationApplied() {
  showBatchTypeModal.value = false
  await onPointEdited()
}

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

interface ActiveValuePollConfig {
  serverId: string
  commonAddress: number
  selectionEpoch: number
  delayMs: number
  signature: string
  points: Array<{ ioa: number; asdu_type: string }>
  keys: Set<string>
}

let activeValuePollTimer: ReturnType<typeof setTimeout> | null = null
let activeValuePollEpoch = 0
let activeValueInFlightEpoch: number | null = null
let activeValuePollConfig: ActiveValuePollConfig | null = null
let activeMutationListRequest = 0

function isCurrentSelection(serverId: string, commonAddress: number, epoch: number) {
  return !componentUnmounted
    && epoch === selectionEpoch
    && selectedServerId.value === serverId
    && selectedCA.value === commonAddress
    && currentServerId === serverId
    && currentCA === commonAddress
}

function stopActiveValuePolling() {
  activeValuePollEpoch++
  if (activeValuePollTimer !== null) {
    clearTimeout(activeValuePollTimer)
    activeValuePollTimer = null
  }
  activeValuePollConfig = null
}

function clearActiveMutationState() {
  // 让已经发出的 list_point_mutations 响应失效。
  activeMutationListRequest++
  activeMutations.value = new Map()
  activeMutationDetails.value = new Map()
  stopActiveValuePolling()
}

function activeValuePollIsCurrent(config: ActiveValuePollConfig, pollEpoch: number) {
  return pollEpoch === activeValuePollEpoch
    && activeValuePollConfig === config
    && isCurrentSelection(config.serverId, config.commonAddress, config.selectionEpoch)
}

function scheduleActiveValuePoll(pollEpoch: number) {
  const config = activeValuePollConfig
  if (!config || !activeValuePollIsCurrent(config, pollEpoch) || activeValuePollTimer !== null) return
  activeValuePollTimer = setTimeout(() => {
    activeValuePollTimer = null
    void pollActivePointValues(pollEpoch)
  }, config.delayMs)
}

function snapshotDiffers(old: DataPointInfo, next: DataPointValueSnapshot) {
  return old.value !== next.value
    || old.quality_ov !== next.quality_ov
    || old.quality_bl !== next.quality_bl
    || old.quality_sb !== next.quality_sb
    || old.quality_nt !== next.quality_nt
    || old.quality_iv !== next.quality_iv
    || old.timestamp !== next.timestamp
}

async function pollActivePointValues(pollEpoch: number) {
  const config = activeValuePollConfig
  if (!config || !activeValuePollIsCurrent(config, pollEpoch)) return
  // 递归 setTimeout 本身已避免重叠；该 guard 也覆盖未来的手动触发。
  if (activeValueInFlightEpoch === pollEpoch) return
  activeValueInFlightEpoch = pollEpoch
  try {
    const snapshots = await invoke<DataPointValueSnapshot[]>('get_data_point_values', {
      serverId: config.serverId,
      commonAddress: config.commonAddress,
      points: config.points,
    })
    if (!activeValuePollIsCurrent(config, pollEpoch)) return

    let changed = false
    for (const snapshot of Array.isArray(snapshots) ? snapshots : []) {
      const key = pointKey(snapshot.ioa, snapshot.asdu_type)
      if (!config.keys.has(key)) continue
      const old = dataMap.get(key)
      if (!old || !snapshotDiffers(old, snapshot)) continue
      if (old.value !== snapshot.value) markChanged(key)
      // 轻量快照只覆盖动态字段，点名、分类、映射和遥控参数等静态定义沿用缓存。
      dataMap.set(key, { ...old, ...snapshot })
      changed = true
    }
    if (changed) updateDisplay()
  } catch (e) {
    if (activeValuePollIsCurrent(config, pollEpoch)) {
      console.error('Failed to load active point values:', e)
    }
  } finally {
    if (activeValueInFlightEpoch === pollEpoch) activeValueInFlightEpoch = null
    if (activeValuePollIsCurrent(config, pollEpoch)) scheduleActiveValuePoll(pollEpoch)
  }
}

function configureActiveValuePolling(
  serverId: string,
  commonAddress: number,
  epoch: number,
  list: PointMutationInfo[],
) {
  if (!isCurrentSelection(serverId, commonAddress, epoch)) return
  const byKey = new Map<string, PointMutationInfo>()
  for (const item of list) byKey.set(pointKey(item.ioa, item.asdu_type), item)
  const items = Array.from(byKey.values()).sort((a, b) =>
    pointKey(a.ioa, a.asdu_type).localeCompare(pointKey(b.ioa, b.asdu_type)),
  )
  if (items.length === 0) {
    if (activeValuePollConfig || activeValuePollTimer !== null || activeValueInFlightEpoch !== null) {
      stopActiveValuePolling()
    }
    return
  }

  const minPeriod = Math.min(...items.map((item) =>
    Number.isFinite(item.period_ms) ? Math.max(50, item.period_ms) : 2000,
  ))
  const delayMs = Math.min(500, Math.max(50, minPeriod / 4))
  const signature = `${serverId}:${commonAddress}:${epoch}:`
    + items.map((item) => `${pointKey(item.ioa, item.asdu_type)}@${item.period_ms}`).join('|')
  if (activeValuePollConfig?.signature === signature) return

  stopActiveValuePolling()
  activeValuePollConfig = {
    serverId,
    commonAddress,
    selectionEpoch: epoch,
    delayMs,
    signature,
    points: items.map(({ ioa, asdu_type }) => ({ ioa, asdu_type })),
    keys: new Set(items.map((item) => pointKey(item.ioa, item.asdu_type))),
  }
  scheduleActiveValuePoll(activeValuePollEpoch)
}

// 拉取当前 (server, CA) 的活跃周期变位集合。
async function refreshActiveMutations() {
  const srvId = selectedServerId.value
  const ca = selectedCA.value
  const epoch = selectionEpoch
  if (!srvId || ca === null || !isCurrentSelection(srvId, ca, epoch)) {
    clearActiveMutationState()
    return
  }
  const request = ++activeMutationListRequest
  try {
    const response = await invoke<PointMutationInfo[]>('list_point_mutations', {
      serverId: srvId,
      commonAddress: ca,
    })
    if (request !== activeMutationListRequest || !isCurrentSelection(srvId, ca, epoch)) return
    const list = Array.isArray(response) ? response : []
    activeMutationDetails.value = new Map(
      list.map(m => [pointKey(m.ioa, m.asdu_type), m]),
    )
    activeMutations.value = new Map(list.map(m => [pointKey(m.ioa, m.asdu_type), m.mode]))
    configureActiveValuePolling(srvId, ca, epoch, list)
  } catch (e) {
    if (request === activeMutationListRequest && isCurrentSelection(srvId, ca, epoch)) {
      console.error('Failed to load point mutations:', e)
    }
  }
}

const activeMutationRows = computed<PointMutationRow[]>(() => {
  // displayPoints 的引用随 targeted poll 更新；读取它让当前值变化触发本 computed。
  void displayPoints.value
  return Array.from(activeMutationDetails.value.values())
    .map(info => ({
      ...info,
      value: dataMap.get(pointKey(info.ioa, info.asdu_type))?.value ?? '-',
    }))
    .sort((a, b) => a.ioa - b.ioa || a.asdu_type.localeCompare(b.asdu_type))
})

async function openSimulationSettings() {
  contextMenu.value.show = false
  if (!selectedServerId.value || currentCA === null) return
  await refreshActiveMutations()
  showSimulationDrawer.value = true
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
      <div class="selection-actions">
        <button
          class="selection-btn"
          :disabled="filteredPoints.length === 0"
          @click="selectFilteredPoints"
        >{{ t('table.selectFiltered') }}</button>
        <button
          class="selection-btn"
          :disabled="filteredPoints.length === 0"
          @click="invertFilteredSelection"
        >{{ t('table.invertFiltered') }}</button>
        <button
          class="selection-btn"
          :disabled="selectedCount === 0"
          @click="clearSelection"
        >{{ t('table.clearSelection') }}</button>
      </div>
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
      <button
        class="add-btn batch settings"
        :disabled="!canOpenBatchSettings"
        @click="openBatchSettings"
        :title="t('table.batchSettings')"
      >{{ t('table.batchSettings') }}</button>
      <button
        class="add-btn batch simulation"
        :disabled="!selectedServerId || currentCA === null"
        @click="openSimulationSettings"
        :title="t('simulationSettings.title')"
      >{{ t('simulationSettings.open') }}</button>
      <span v-if="selectedCount > 0" class="selected-count">
        {{ t('table.selectedCount', { count: selectedCount }) }}
      </span>
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
            <th class="col-select" />
            <th class="col-ioa sortable" @click="toggleSort('ioa')">
              IOA <span class="sort-glyph">{{ sortGlyph('ioa') }}</span>
            </th>
            <th class="col-type sortable" @click="toggleSort('asdu_type')">
              {{ t('table.asduTypeCol') }} <span class="sort-glyph">{{ sortGlyph('asdu_type') }}</span>
            </th>
            <th class="col-name sortable" @click="toggleSort('name')">
              {{ t('table.nameCol') }} <span class="sort-glyph">{{ sortGlyph('name') }}</span>
            </th>
            <th class="col-value sortable" @click="toggleSort('value')">
              {{ t('table.valueCol') }} <span class="sort-glyph">{{ sortGlyph('value') }}</span>
            </th>
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
              <td class="col-select">
                <input
                  type="checkbox"
                  :checked="isSelected(point)"
                  :aria-label="`${point.ioa} ${point.asdu_type}`"
                  @click.stop="toggleRowSelection(point)"
                />
              </td>
              <td :class="['col-ioa', { 'ioa-dup': duplicateIoaTypes(point) }]">
                <template v-if="activeMutations.has(point.ioa + ':' + point.asdu_type)">
                  <span class="mut-dot" />
                  <span
                    class="mut-mode"
                    :title="mutationModeLabel(activeMutations.get(point.ioa + ':' + point.asdu_type))"
                  >{{ mutationGlyph(activeMutations.get(point.ioa + ':' + point.asdu_type)) }}</span>
                </template>{{ point.ioa }}<span
                  v-if="duplicateIoaTypes(point)"
                  class="dup-ioa-badge"
                  :title="t('table.dupIoaTitle', { ioa: point.ioa, types: duplicateIoaTypes(point)! })"
                >⚠</span>
              </td>
              <td class="col-type">
                {{ formatAsduTypeWithId(point.asdu_type) }}
                <span
                  v-if="derivedTbLabel(point)"
                  class="tb-badge"
                  :title="t('table.derivedTbTitle', { tb: derivedTbLabel(point)! })"
                >+{{ derivedTbLabel(point) }}</span>
              </td>
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
                  <span class="value-text">{{ formatDataPointValue(point, t) }}</span>
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
      <div class="context-menu-item" @click="openSimulationSettings">
        {{ t('simulationSettings.title') }}
      </div>
      <div class="context-menu-sep" />
      <div v-if="selectedCount === 1" class="context-menu-item" @click="editSelectedPoint">
        {{ t('table.editPoint') }}
      </div>
      <div v-if="selectedControlPoints.length > 0" class="context-menu-item" @click="openBatchControlOptions">
        {{ `${t('table.batchControlOptions')} (${selectedControlPoints.length})` }}
      </div>
      <div v-if="canBatchMigrateType" class="context-menu-item" @click="openBatchTypeMigration">
        {{ `${t('table.batchTypeMigration')} (${selectedMonitorPoints.length})` }}
      </div>
      <div class="context-menu-item danger" @click="deleteSelectedPoints">
        {{ selectedCount > 1 ? `${t('table.deletePoint')} (${selectedCount})` : t('table.deletePoint') }}
      </div>
    </div>

    <SimulationSettingsDrawer
      :visible="showSimulationDrawer"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :selected-points="selectedRows"
      :active-rows="activeMutationRows"
      @close="showSimulationDrawer = false"
      @changed="refreshActiveMutations"
    />

    <!-- Add Data Point Modal -->
    <DataPointModal
      :visible="showAddModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :category="selectedCategory"
      :existing-points="showAddModal ? displayPoints : []"
      @close="showAddModal = false"
      @added="onPointAdded"
    />

    <DataPointModal
      :visible="showEditModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :point="editingPointDefinition"
      :existing-points="showEditModal ? displayPoints : []"
      @close="showEditModal = false; editingPointDefinition = null"
      @added="handlePointEdited"
    />

    <!-- Batch Add Modal -->
    <BatchAddModal
      :visible="showBatchModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :category="selectedCategory"
      :existing-points="showBatchModal ? displayPoints : []"
      @close="showBatchModal = false"
      @added="onPointAdded"
    />

    <!-- 批量设置控制点 QU/QL 与 S/E -->
    <BatchControlOptionsModal
      :visible="showBatchControlModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :points="selectedControlPoints"
      @close="showBatchControlModal = false"
      @applied="onControlOptionsApplied"
    />

    <BatchTypeMigrationModal
      :visible="showBatchTypeModal"
      :server-id="selectedServerId ?? ''"
      :common-address="currentCA ?? 0"
      :points="selectedMonitorPoints"
      @close="showBatchTypeModal = false"
      @applied="onTypeMigrationApplied"
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

.selection-actions {
  display: flex;
  gap: 4px;
}

.selection-btn {
  padding: 3px 6px;
  color: var(--c-overlay1);
  font-size: 10px;
  white-space: nowrap;
  cursor: pointer;
  background: var(--c-surface0);
  border: 1px solid var(--c-surface1);
  border-radius: 4px;
}

.selection-btn:hover:not(:disabled) {
  color: var(--c-text);
  background: var(--c-surface1);
}

.selection-btn:disabled {
  cursor: not-allowed;
  opacity: 0.4;
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

.add-btn.simulation {
  color: var(--c-sapphire);
}

.add-btn.settings {
  color: var(--c-mauve);
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

.selected-count {
  color: var(--c-sapphire);
  font-size: 11px;
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

.table th.sortable {
  cursor: pointer;
  user-select: none;
}

.table th.sortable:hover {
  color: var(--c-text);
}

.sort-glyph {
  color: var(--c-blue);
  font-size: 8px;
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

.col-select {
  width: 28px;
  padding-right: 0 !important;
  padding-left: 8px !important;
  text-align: center;
}

.col-select input {
  margin: 0;
  accent-color: var(--c-blue);
  cursor: pointer;
}

.table tbody tr.selected .col-ioa {
  color: var(--c-base);
}

.col-type {
  width: 168px;
  white-space: nowrap;
}

.tb-badge {
  display: inline-block;
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--c-surface1);
  color: var(--c-sapphire);
  font-size: 10px;
  line-height: 16px;
  cursor: help;
}

/* 同 CASDU 跨类型重复 IOA:标红 + ⚠(issue #28,仅警示不阻断) */
.col-ioa.ioa-dup {
  color: var(--c-red);
}

.dup-ioa-badge {
  margin-left: 4px;
  color: var(--c-red);
  font-size: 10px;
  cursor: help;
}

/* 选中态:行底色是 --c-blue,上面 `tr.selected .col-ioa` 会把 IOA 压成 --c-base,
   红字直接消失(而"点中行→改 IOA"正是修冲突的操作路径)。这里反过来把冲突
   单元格整块刷成红底 + 深色字 —— 特异性高于选中态规则,深色主题下对比最强。 */
.table tbody tr.selected .col-ioa.ioa-dup {
  background: var(--c-red);
  color: var(--c-crust);
}

.table tbody tr.selected .col-ioa.ioa-dup .dup-ioa-badge {
  color: var(--c-crust);
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
</style>
