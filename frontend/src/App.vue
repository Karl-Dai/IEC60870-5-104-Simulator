<script setup lang="ts">
import { ref, computed, provide, onMounted, onUnmounted, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import Toolbar from './components/Toolbar.vue'
import ConnectionTree from './components/ConnectionTree.vue'
import DataPointTable from './components/DataPointTable.vue'
import ValuePanel from './components/ValuePanel.vue'
import LogPanel from './components/LogPanel.vue'
import RemoteParamsDrawer from './components/RemoteParamsDrawer.vue'
import RemoteParamsModal from './components/RemoteParamsModal.vue'
import AppDialog from '@shared/components/AppDialog.vue'
import UpdateDialog from '@shared/components/UpdateDialog.vue'
import ParseFrameDialog from '@shared/components/ParseFrameDialog.vue'
import Splitter from './components/Splitter.vue'
import { invoke } from '@tauri-apps/api/core'
import { showAlert, showConfirm, showPrompt, dialogKey } from '@shared/composables/useDialog'
import { formatCorrections, type TimingCorrection } from '@shared/timing'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()

const dataPointTableRef = ref<InstanceType<typeof DataPointTable> | null>(null)

// Shared state
const selectedServerId = ref<string | null>(null)
const selectedServerState = ref<string>('Stopped')
const selectedCA = ref<number | null>(null)
const selectedCategory = ref<string | null>(null)
const selectedPoints = ref<{ ioa: number; asdu_type: string; category: string; value: string }[]>([])
const logExpanded = ref(false)

// Resizable layout — widths persisted to localStorage
const LS_TREE_W = 'iec104.layout.treeWidth'
const LS_PANEL_W = 'iec104.layout.panelWidth'
const TREE_MIN = 180, TREE_MAX = 480, TREE_DEFAULT = 240
const PANEL_MIN = 220, PANEL_MAX = 600, PANEL_DEFAULT = 280

function loadWidth(key: string, def: number, min: number, max: number): number {
  try {
    const v = parseInt(localStorage.getItem(key) ?? '', 10)
    if (Number.isFinite(v) && v >= min && v <= max) return v
  } catch { /* ignore */ }
  return def
}

const treeWidth = ref(loadWidth(LS_TREE_W, TREE_DEFAULT, TREE_MIN, TREE_MAX))
const panelWidth = ref(loadWidth(LS_PANEL_W, PANEL_DEFAULT, PANEL_MIN, PANEL_MAX))

watch(treeWidth, (v) => {
  try { localStorage.setItem(LS_TREE_W, String(Math.round(v))) } catch { /* ignore */ }
})
watch(panelWidth, (v) => {
  try { localStorage.setItem(LS_PANEL_W, String(Math.round(v))) } catch { /* ignore */ }
})

// 通信日志面板展开后的高度,可拖拽调节并持久化。上限随窗口高度动态变化。
const LS_LOG_H = 'iec104.layout.logHeight'
const LOG_MIN = 80, LOG_DEFAULT = 200
const logHeight = ref(loadWidth(LS_LOG_H, LOG_DEFAULT, LOG_MIN, 100000))
const logMax = computed(() => Math.max(LOG_MIN, Math.floor(window.innerHeight * 0.7)))
watch(logHeight, (v) => {
  try { localStorage.setItem(LS_LOG_H, String(Math.round(v))) } catch { /* ignore */ }
})

// Provide shared state to children
provide('selectedServerId', selectedServerId)
provide('selectedServerState', selectedServerState)
provide('selectedCA', selectedCA)
provide('selectedCategory', selectedCategory)
provide('selectedPoints', selectedPoints)

// Tree refresh trigger
const treeRefreshKey = ref(0)
provide('treeRefreshKey', treeRefreshKey)

function refreshTree() {
  treeRefreshKey.value++
}
provide('refreshTree', refreshTree)

// Data refresh trigger
const dataRefreshKey = ref(0)
provide('dataRefreshKey', dataRefreshKey)

function refreshData() {
  dataRefreshKey.value++
}
provide('refreshData', refreshData)

// Realtime category counts derived from DataPointTable's dataMap
const categoryCounts = ref<Map<string, number>>(new Map())
provide('categoryCounts', categoryCounts)
provide(dialogKey, { showAlert, showConfirm, showPrompt })

// Frame parser dialog (opened from Toolbar button or LogPanel right-click)
const parseFrameVisible = ref(false)
const parseFramePrefill = ref<string>('')
function openParseFrame(prefill?: string) {
  parseFramePrefill.value = prefill ?? ''
  parseFrameVisible.value = true
}
provide('openParseFrame', openParseFrame)

function handleServerSelect(id: string, state: string) {
  selectedServerId.value = id
  selectedServerState.value = state
  selectedCA.value = null
  selectedCategory.value = null
  selectedPoints.value = []
}

function handleStationSelect(serverId: string, ca: number, state: string) {
  selectedServerId.value = serverId
  selectedServerState.value = state
  selectedCA.value = ca
  selectedCategory.value = null
  selectedPoints.value = []
  dataPointTableRef.value?.loadData()
}

function handleCategorySelect(serverId: string, ca: number, category: string, state: string) {
  selectedServerId.value = serverId
  selectedServerState.value = state
  selectedCA.value = ca
  selectedCategory.value = category
  selectedPoints.value = []
  dataPointTableRef.value?.loadData()
}

function handlePointSelect(points: { ioa: number; asdu_type: string; category: string; value: string }[]) {
  selectedPoints.value = points
}

function toggleLog() {
  logExpanded.value = !logExpanded.value
}

type UpdateMeta = { version: string; notes: string; pub_date?: string | null }
const updateMeta = ref<UpdateMeta | null>(null)
const updateVisible = ref(false)

async function checkUpdate(force = false): Promise<UpdateMeta | null> {
  const meta = await invoke<UpdateMeta | null>('check_for_update', { force })
  if (meta) {
    updateMeta.value = meta
    updateVisible.value = true
  }
  return meta
}
provide('checkUpdate', checkUpdate)

// Backend pushes server-state-changed on every start/stop; without this the
// toolbar buttons & tree dot drift out of sync when actions originate
// outside the toolbar (e.g. tree context menu, error-driven auto-stop).
let unlistenServerState: (() => void) | null = null
// load_config 导入了违反 t2<t1<t3 / w≤⌊2k/3⌋ 的旧配置时,后端纠正后推此事件。
let unlistenTimingCorrected: (() => void) | null = null

onMounted(async () => {
  unlistenServerState = await listen<{ id: string; state: string }>('server-state-changed', (event) => {
    const { id, state } = event.payload
    if (selectedServerId.value === id) {
      selectedServerState.value = state
    }
    refreshTree()
  })
  unlistenTimingCorrected = await listen<Array<{ endpoint: string; corrections: TimingCorrection[] }>>(
    'config-timing-corrected',
    (event) => {
      const lines = event.payload.map((e) => `${e.endpoint}: ${formatCorrections(e.corrections)}`)
      void showAlert(`${t('remoteParams.configTimingCorrected')}\n${lines.join('\n')}`)
    },
  )
  setTimeout(() => {
    checkUpdate(false).catch((e) => console.warn('auto update check failed', e))
  }, 2000)
})

onUnmounted(() => {
  unlistenServerState?.()
  unlistenTimingCorrected?.()
})

function snoozeUpdate() {
  if (updateMeta.value) {
    invoke('snooze_update', { version: updateMeta.value.version }).catch(() => {})
  }
}

// Runtime params modal — opened from ConnectionTree right-click menu
const runtimeParamsModalVisible = ref(false)
const runtimeParamsModalServerId = ref<string | null>(null)
const runtimeParamsModalLabel = ref<string>('')
function openRuntimeParamsModal(serverId: string, label: string) {
  runtimeParamsModalServerId.value = serverId
  runtimeParamsModalLabel.value = label
  runtimeParamsModalVisible.value = true
}
function closeRuntimeParamsModal() {
  runtimeParamsModalVisible.value = false
}

// Runtime params drawer — opened from toolbar, bound to currently selected server
const runtimeParamsDrawerVisible = ref(false)
function openRuntimeParamsDrawer() {
  runtimeParamsDrawerVisible.value = true
}
function closeRuntimeParamsDrawer() {
  runtimeParamsDrawerVisible.value = false
}

// 弹窗/抽屉保存参数后:刷新树(transport 可能改了标签)并 bump dataRefreshKey,
// 让 DataPointTable 重载点位并刷新 sync-TB 徽标(issue #28:保存后 +TB 徽标不出现)。
function onRuntimeParamsSaved() {
  refreshTree()
  refreshData()
}
provide('openRuntimeParamsDrawer', openRuntimeParamsDrawer)
</script>

<template>
  <div
    :class="['app-layout', { 'log-expanded': logExpanded }]"
    :style="{
      '--tree-w': treeWidth + 'px',
      '--panel-w': panelWidth + 'px',
      '--log-h': logHeight + 'px',
    }"
  >
    <header class="toolbar-area">
      <Toolbar />
    </header>

    <aside class="tree-area">
      <ConnectionTree
        @server-select="handleServerSelect"
        @station-select="handleStationSelect"
        @category-select="handleCategorySelect"
        @edit-runtime-params="openRuntimeParamsModal"
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
      <DataPointTable
        ref="dataPointTableRef"
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
      <ValuePanel />
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
      <LogPanel :expanded="logExpanded" @toggle="toggleLog" />
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
    <RemoteParamsModal
      :visible="runtimeParamsModalVisible"
      :server-id="runtimeParamsModalServerId"
      :server-label="runtimeParamsModalLabel"
      @saved="onRuntimeParamsSaved"
      @close="closeRuntimeParamsModal"
    />
    <RemoteParamsDrawer
      :visible="runtimeParamsDrawerVisible"
      @saved="onRuntimeParamsSaved"
      @close="closeRuntimeParamsDrawer"
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

.app-layout.log-expanded {
  grid-template-rows: 42px 1fr 4px var(--log-h, 200px);
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

.splitter-panel {
  grid-area: sp-r;
}

.splitter-log {
  grid-area: splog;
}

.panel-area {
  grid-area: panel;
  background: var(--c-mantle);
  overflow-y: auto;
}

.log-area {
  grid-area: log;
  background: var(--c-base);
  border-top: 1px solid var(--c-surface0);
  overflow: hidden;
}
</style>
