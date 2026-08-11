<script setup lang="ts">
import { ref, shallowRef, computed, provide, onMounted, onUnmounted } from 'vue'
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

const LOG_H_KEY = 'iec104.logPanel.height'
function readSavedHeight(): number {
  try {
    const v = parseInt(localStorage.getItem(LOG_H_KEY) || '', 10)
    if (!isNaN(v) && v > 0) return v
  } catch { /* ignore */ }
  return 220
}
const logHeight = ref<number>(readSavedHeight())

function clampLogHeight(h: number): number {
  const max = Math.max(120, Math.floor(window.innerHeight * 0.7))
  return Math.min(max, Math.max(80, h))
}

const gridRows = computed(() => {
  if (!logExpanded.value) return '42px 1fr 0 32px'
  return `42px 1fr 4px ${logHeight.value}px`
})

function startResize(e: MouseEvent) {
  e.preventDefault()
  const startY = e.clientY
  const startH = logHeight.value
  document.body.style.cursor = 'ns-resize'
  document.body.style.userSelect = 'none'
  function onMove(ev: MouseEvent) {
    logHeight.value = clampLogHeight(startH + (startY - ev.clientY))
  }
  function onUp() {
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
    try { localStorage.setItem(LOG_H_KEY, String(logHeight.value)) } catch { /* ignore */ }
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}

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
      // 持久化由用户主动保存配置时触发,此处无需 save_config。
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

function snoozeUpdate() {
  if (updateMeta.value) {
    invoke('snooze_update', { version: updateMeta.value.version }).catch(() => {})
  }
}

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
  <div :class="['app-layout', { 'log-expanded': logExpanded }]" :style="{ gridTemplateRows: gridRows }">
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
    <main class="content-area">
      <DataTable
        :key="workspaceEpoch"
        @point-select="handlePointSelect"
      />
    </main>
    <aside class="panel-area">
      <ValuePanel :key="workspaceEpoch" />
    </aside>

    <div
      v-show="logExpanded"
      class="log-resizer"
      role="separator"
      aria-orientation="horizontal"
      @mousedown="startResize"
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
      @snooze="snoozeUpdate"
    />
  </div>
</template>

<style>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
  background: var(--c-crust);
  color: var(--c-text);
}

/* Dark scrollbars across the app — overrides macOS "Always show" white tracks */
*::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
*::-webkit-scrollbar-track {
  background: var(--c-mantle);
}
*::-webkit-scrollbar-thumb {
  background: var(--c-surface0);
  border-radius: 5px;
  border: 2px solid var(--c-mantle);
}
*::-webkit-scrollbar-thumb:hover {
  background: var(--c-surface1);
}
*::-webkit-scrollbar-corner {
  background: var(--c-mantle);
}
* {
  scrollbar-color: var(--c-surface0) var(--c-mantle);
  scrollbar-width: thin;
}

/* Keyboard focus ring — never hide it. Mouse focus stays clean via :focus-visible. */
:focus { outline: none; }
:focus-visible {
  outline: 2px solid var(--c-blue);
  outline-offset: 1px;
  border-radius: 2px;
}

.app-layout {
  display: grid;
  grid-template-columns: 260px 1fr 280px;
  grid-template-rows: 42px 1fr 0 32px;
  grid-template-areas:
    "toolbar toolbar toolbar"
    "tree content panel"
    "resizer resizer resizer"
    "log log log";
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
  border-right: 1px solid var(--c-surface0);
  overflow-y: auto;
}

.content-area {
  grid-area: content;
  background: var(--c-crust);
  overflow: hidden;
}

.panel-area {
  grid-area: panel;
  background: var(--c-mantle);
  border-left: 1px solid var(--c-surface0);
  overflow-y: auto;
}

.log-resizer {
  grid-area: resizer;
  height: 4px;
  background: var(--c-surface0);
  cursor: ns-resize;
  transition: background 0.15s;
  user-select: none;
}

.log-resizer:hover {
  background: var(--c-blue);
}

.log-area {
  grid-area: log;
  background: var(--c-base);
  border-top: 1px solid var(--c-surface0);
  overflow: hidden;
}
</style>
