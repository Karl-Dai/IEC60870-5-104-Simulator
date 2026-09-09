<script setup lang="ts">
import { ref, shallowRef, computed, provide, onMounted, onUnmounted, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import Toolbar from './components/Toolbar.vue'
import ConnectionTree from './components/ConnectionTree.vue'
import DataTable from './components/DataTable.vue'
import ValuePanel from './components/ValuePanel.vue'
import LogPanel from './components/LogPanel.vue'
import AppDialog from '@shared/components/AppDialog.vue'
import UpdateDialog from '@shared/components/UpdateDialog.vue'
import ParseFrameDialog from '@shared/components/ParseFrameDialog.vue'
import Splitter from '@shared/components/Splitter.vue'
import { showAlert, showConfirm, showPrompt, dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import { formatCorrections, type TimingCorrection } from '@shared/timing'
import type { ReceivedDataPointInfo, ChangedCategoriesMap, CategoryCountsMap } from './types'

// Shared state
const selectedConnectionId = ref<string | null>(null)
const selectedConnectionState = ref<string>('Disconnected')
// Multi-CA: which Common Address inside the selected connection is the user
// looking at? `null` means "all CAs combined" (legacy single-CA behaviour).
const selectedCA = ref<number | null>(null)
const selectedCategory = ref<string | null>(null)
// shallowRef: 选中可达 15k+ 行（Ctrl+A）；deep ref 会在切换连接清空时卡几百 ms。
const selectedPoints = shallowRef<ReceivedDataPointInfo[]>([])
const logExpanded = ref(false)
// A loaded configuration replaces the whole workspace. Remounting every
// workspace-bound view drops local cursors, log selections and pending-view
// state; late async responses can then only update the discarded instances.
const workspaceEpoch = ref(0)

const TREE_W_KEY = 'iec104master.layout.treeWidth'
const PANEL_W_KEY = 'iec104master.layout.panelWidth'
const LOG_H_KEY = 'iec104.logPanel.height'
const TREE_MIN = 180, TREE_MAX = 480, TREE_DEFAULT = 240
const PANEL_MIN = 220, PANEL_MAX = 600, PANEL_DEFAULT = 280
const LOG_MIN = 80, LOG_DEFAULT = 220

function readSavedSize(key: string, fallback: number, min: number, max: number): number {
  try {
    const value = parseInt(localStorage.getItem(key) || '', 10)
    if (Number.isFinite(value) && value >= min && value <= max) return value
  } catch { /* ignore */ }
  return fallback
}

const treeWidth = ref(readSavedSize(TREE_W_KEY, TREE_DEFAULT, TREE_MIN, TREE_MAX))
const panelWidth = ref(readSavedSize(PANEL_W_KEY, PANEL_DEFAULT, PANEL_MIN, PANEL_MAX))
const logHeight = ref(readSavedSize(LOG_H_KEY, LOG_DEFAULT, LOG_MIN, 100000))
const logMax = computed(() => Math.max(LOG_MIN, Math.floor(window.innerHeight * 0.7)))

watch(treeWidth, value => {
  try { localStorage.setItem(TREE_W_KEY, String(Math.round(value))) } catch { /* ignore */ }
})
watch(panelWidth, value => {
  try { localStorage.setItem(PANEL_W_KEY, String(Math.round(value))) } catch { /* ignore */ }
})
watch(logHeight, value => {
  try { localStorage.setItem(LOG_H_KEY, String(Math.round(value))) } catch { /* ignore */ }
})

const gridRows = computed(() => {
  if (!logExpanded.value) return '42px 1fr 0 32px'
  return `42px 1fr 4px ${logHeight.value}px`
})

// Provide shared state to children
provide('selectedConnectionId', selectedConnectionId)
provide('selectedConnectionState', selectedConnectionState)
provide('selectedCA', selectedCA)
provide('selectedCategory', selectedCategory)
provide('selectedPoints', selectedPoints)

// Tree refresh trigger
const treeRefreshKey = ref(0)
provide('treeRefreshKey', treeRefreshKey)

// 80ms 防抖：连续 connection-state 事件（disconnect→delete→reconnect）合并为一次重载。
let refreshTreePending: number | null = null
function refreshTree() {
  if (refreshTreePending !== null) return
  refreshTreePending = window.setTimeout(() => {
    refreshTreePending = null
    treeRefreshKey.value++
  }, 80)
}
provide('refreshTree', refreshTree)

// Data refresh trigger
const dataRefreshKey = ref(0)
provide('dataRefreshKey', dataRefreshKey)

function refreshData() {
  dataRefreshKey.value++
}
provide('refreshData', refreshData)

// Tree flash effect, keyed by (connId, ca, category) — CA 维度避免 CA=1 收到
// 的变位让 CA=2/3 同名 category 节点也跟着闪黄。
const changedCategories = ref<ChangedCategoriesMap>(new Map())
provide('changedCategories', changedCategories)

// Realtime category counts (DataTable writes, ConnectionTree reads).
const categoryCounts = ref<CategoryCountsMap>(new Map())
provide('categoryCounts', categoryCounts)

provide(dialogKey, { showAlert, showConfirm, showPrompt })
const { t } = useI18n()

// Frame parser dialog (opened from Toolbar button or LogPanel right-click)
const parseFrameVisible = ref(false)
const parseFramePrefill = ref<string>('')
function openParseFrame(prefill?: string) {
  parseFramePrefill.value = prefill ?? ''
  parseFrameVisible.value = true
}
provide('openParseFrame', openParseFrame)

// Toolbar 暴露 openEditConnection(id) 给右键菜单使用。Toolbar 与 ConnectionTree
// 是兄弟,Vue provide 不能跨兄弟传递,所以由 App 持有 Toolbar 的模板 ref 并 provide
// 一层转发 closure,这样 ConnectionTree 的 inject('openEditConnection') 才能拿到值。
const toolbarRef = ref<InstanceType<typeof Toolbar> | null>(null)
provide('openEditConnection', (id: string) => {
  toolbarRef.value?.openEditConnection(id)
})

// Listen for backend connection state events
let unlistenConnState: (() => void) | null = null
// load_config imported a config violating t2<t1<t3 / w≤⌊2k/3⌋; backend corrected it.
let unlistenTimingCorrected: (() => void) | null = null
// CA set updated by GI debouncer; refresh tree to reflect new common addresses in memory.
let unlistenCasUpdated: (() => void) | null = null

onMounted(async () => {
  // Tauri runtime 不可用(纯浏览器 vite dev / headless UI 验证)时跳过 IPC listener,
  // 仅渲染静态 UI。生产 Tauri 环境下 __TAURI_INTERNALS__ 存在,正常注册 listener。
  const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  if (!inTauri) {
    console.warn('Tauri runtime not detected; skipping IPC listeners for UI-only render')
    return
  }
  unlistenConnState = await listen<{ id: string; state: string }>('connection-state', (event) => {
    const { id, state } = event.payload
    if (selectedConnectionId.value === id) {
      selectedConnectionState.value = state
    }
    refreshTree()
  })
  unlistenTimingCorrected = await listen<Array<{ target_address: string; corrections: TimingCorrection[] }>>(
    'config-timing-corrected',
    (event) => {
      const detail = event.payload
        .map((e) => `${e.target_address}: ${formatCorrections(e.corrections)}`)
        .join('; ')
      void showAlert(t('newConn.timingCorrected', { detail }))
    },
  )
  unlistenCasUpdated = await listen<{ id: string; common_addresses: number[]; added: number[] }>(
    'connection-cas-updated',
    () => {
      // 后端 GI debouncer 自动扩充了内存中的 CA 集合;刷新连接树即可。
      // 后端会在 CA 列表变化后自动持久化工作区,前端只负责刷新显示。
      refreshTree()
    },
  )
  setTimeout(() => {
    checkUpdate(false).catch((e) => console.warn('auto update check failed', e))
  }, 2000)
})

onUnmounted(() => {
  unlistenConnState?.()
  unlistenTimingCorrected?.()
  unlistenCasUpdated?.()
  if (refreshTreePending !== null) {
    clearTimeout(refreshTreePending)
    refreshTreePending = null
  }
})

function handleConnectionSelect(id: string, state: string) {
  const changed = selectedConnectionId.value !== id
  selectedConnectionId.value = id
  selectedConnectionState.value = state
  // Only clear category when switching to a different connection
  if (changed) {
    selectedCA.value = null
    selectedCategory.value = null
    selectedPoints.value = []
  }
}

function handleCategorySelect(connectionId: string, category: string, ca: number | null) {
  selectedConnectionId.value = connectionId
  selectedCA.value = ca
  selectedCategory.value = category
}

function handlePointSelect(points: ReceivedDataPointInfo[]) {
  selectedPoints.value = points
}

function toggleLog() {
  logExpanded.value = !logExpanded.value
}

const updateMeta = ref<{ version: string; notes: string; pub_date?: string | null } | null>(null)
const updateVisible = ref(false)

async function checkUpdate(force = false): Promise<{ version: string; notes: string; pub_date?: string | null } | null> {
  const meta = await invoke<{ version: string; notes: string; pub_date?: string | null } | null>('check_for_update', { force })
  if (meta) {
    updateMeta.value = meta
    updateVisible.value = true
  }
  return meta
}
provide('checkUpdate', checkUpdate)

function resetWorkspaceView() {
  selectedConnectionId.value = null
  selectedConnectionState.value = 'Disconnected'
  selectedCA.value = null
  selectedCategory.value = null
  selectedPoints.value = []
  changedCategories.value = new Map()
  categoryCounts.value = new Map()
  workspaceEpoch.value++
}
provide('resetWorkspaceView', resetWorkspaceView)
</script>

<template>
  <div
    :class="['app-layout', { 'log-expanded': logExpanded }]"
    :style="{
      gridTemplateRows: gridRows,
      '--tree-w': treeWidth + 'px',
      '--panel-w': panelWidth + 'px',
    }"
  >
    <header class="toolbar-area">
      <Toolbar ref="toolbarRef" />
    </header>

    <aside class="tree-area">
      <ConnectionTree
        :key="workspaceEpoch"
        @connection-select="handleConnectionSelect"
        @category-select="handleCategorySelect"
      />
    </aside>
    <Splitter
      class="splitter-tree"
      axis="x"
      :min="TREE_MIN"
      :max="TREE_MAX"
      v-model="treeWidth"
    />
    <main class="content-area">
      <DataTable
        :key="workspaceEpoch"
        @point-select="handlePointSelect"
      />
    </main>
    <Splitter
      class="splitter-panel"
      axis="x"
      :min="PANEL_MIN"
      :max="PANEL_MAX"
      v-model="panelWidth"
      reverse
    />
    <aside class="panel-area">
      <ValuePanel :key="workspaceEpoch" />
    </aside>

    <Splitter
      v-show="logExpanded"
      class="splitter-log"
      axis="y"
      :min="LOG_MIN"
      :max="logMax"
      v-model="logHeight"
      reverse
    />
    <footer class="log-area">
      <LogPanel :key="workspaceEpoch" :expanded="logExpanded" @toggle="toggleLog" />
    </footer>
    <AppDialog />
    <ParseFrameDialog
      :visible="parseFrameVisible"
      :prefill="parseFramePrefill"
      @close="parseFrameVisible = false"
    />
    <UpdateDialog
      :visible="updateVisible"
      :version="updateMeta?.version ?? ''"
      :notes="updateMeta?.notes ?? ''"
      @close="updateVisible = false"
    />
  </div>
</template>

<style>
/* Reset, body, scrollbars and focus ring live in shared-frontend/styles/base.css. */
.app-layout {
  display: grid;
  grid-template-columns: var(--tree-w, 240px) 4px 1fr 4px var(--panel-w, 280px);
  grid-template-rows: 42px 1fr 0 32px;
  grid-template-areas:
    "toolbar toolbar toolbar toolbar toolbar"
    "tree    sp-l    content sp-r    panel"
    "splog   splog   splog   splog   splog"
    "log     log     log     log     log";
  height: 100vh;
  width: 100vw;
}

.toolbar-area {
  grid-area: toolbar;
  background: var(--c-base);
  border-bottom: 1px solid var(--c-surface0);
}

.tree-area {
  grid-area: tree;
  background: var(--c-mantle);
  overflow-y: auto;
}

.splitter-tree {
  grid-area: sp-l;
}

.content-area {
  grid-area: content;
  background: var(--c-crust);
  overflow: hidden;
}

.panel-area {
  grid-area: panel;
  background: var(--c-mantle);
  overflow-y: auto;
}

.splitter-panel {
  grid-area: sp-r;
}

.splitter-log {
  grid-area: splog;
}

.log-area {
  grid-area: log;
  background: var(--c-base);
  border-top: 1px solid var(--c-surface0);
  overflow: hidden;
}
</style>
