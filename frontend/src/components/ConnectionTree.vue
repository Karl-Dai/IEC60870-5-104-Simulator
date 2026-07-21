<script setup lang="ts">
import { ref, inject, watch, onMounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert, showConfirm as ShowConfirm } from '@shared/composables/useDialog'
import type { ServerInfo, StationInfo } from '../types'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import EmptyState from '@shared/components/EmptyState.vue'
import { formatStartServerError } from '../errors'

const { t } = useI18n()
const { showAlert, showConfirm } = inject<{
  showAlert: typeof ShowAlert
  showConfirm: typeof ShowConfirm
}>(dialogKey)!

const CATEGORIES = [
  'single_point',
  'double_point',
  'step_position',
  'bitstring',
  'normalized_measured',
  'scaled_measured',
  'float_measured',
  'integrated_totals',
  'single_command',
  'double_command',
  'step_command',
  'bitstring_command',
  'normalized_setpoint',
  'scaled_setpoint',
  'float_setpoint',
]

// 每个监视方向 category 对应的 ASDU TypeId: 无时标 · CP56 时标
// 与 crates/iec104sim-core/src/types.rs::AsduTypeId::category 一致
const CATEGORY_TYPEIDS: Record<string, string> = {
  single_point: '1 · 30',
  double_point: '3 · 31',
  step_position: '5 · 32',
  bitstring: '7 · 33',
  normalized_measured: '9 · 21 · 34',
  scaled_measured: '11 · 35',
  float_measured: '13 · 36',
  integrated_totals: '15 · 37',
  single_command: '45 · 58',
  double_command: '46 · 59',
  step_command: '47 · 60',
  bitstring_command: '51 · 64',
  normalized_setpoint: '48 · 61',
  scaled_setpoint: '49 · 62',
  float_setpoint: '50 · 63',
}

const sharedCategoryCounts = inject<Ref<Map<string, number>>>('categoryCounts')!

interface TreeServer {
  server: ServerInfo
  expanded: boolean
  stations: TreeStation[]
}

interface TreeStation {
  station: StationInfo
  expanded: boolean
  serverId: string
}

const emit = defineEmits<{
  (e: 'server-select', id: string, state: string): void
  (e: 'station-select', serverId: string, ca: number): void
  (e: 'category-select', serverId: string, ca: number, category: string): void
  (e: 'edit-runtime-params', serverId: string, label: string): void
}>()

const treeRefreshKey = inject<Ref<number>>('treeRefreshKey')!
const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedCategory = inject<Ref<string | null>>('selectedCategory')!

const treeData = ref<TreeServer[]>([])
const contextMenu = ref({
  show: false,
  x: 0,
  y: 0,
  type: '' as 'server' | 'station',
  serverId: '',
  ca: 0,
  serverState: '',
})

async function loadTree() {
  try {
    const servers = await invoke<ServerInfo[]>('list_servers')
    const newTree: TreeServer[] = []

    for (const server of servers) {
      const existing = treeData.value.find(t => t.server.id === server.id)
      const stations = await invoke<StationInfo[]>('list_stations', { serverId: server.id })
      newTree.push({
        server,
        expanded: existing ? existing.expanded : true,
        stations: stations.map(s => ({
          station: s,
          expanded: existing?.stations.find(es => es.station.common_address === s.common_address)?.expanded ?? true,
          serverId: server.id,
        })),
      })
    }
    treeData.value = newTree
  } catch (e) {
    console.error('Failed to load tree:', e)
  }
}

watch(treeRefreshKey, () => loadTree())
onMounted(loadTree)

function toggleServer(ts: TreeServer) {
  ts.expanded = !ts.expanded
}

function toggleStation(tst: TreeStation) {
  tst.expanded = !tst.expanded
}

function selectServer(ts: TreeServer) {
  emit('server-select', ts.server.id, ts.server.state)
}

function selectStation(ts: TreeServer, tst: TreeStation) {
  emit('station-select', ts.server.id, tst.station.common_address)
}

function selectCategory(ts: TreeServer, tst: TreeStation, category: string) {
  emit('category-select', ts.server.id, tst.station.common_address, category)
}

function showContextMenuForServer(e: MouseEvent, ts: TreeServer) {
  e.preventDefault()
  contextMenu.value = {
    show: true,
    x: e.clientX,
    y: e.clientY,
    type: 'server',
    serverId: ts.server.id,
    ca: 0,
    serverState: ts.server.state,
  }
}

function showContextMenuForStation(e: MouseEvent, ts: TreeServer, tst: TreeStation) {
  e.preventDefault()
  contextMenu.value = {
    show: true,
    x: e.clientX,
    y: e.clientY,
    type: 'station',
    serverId: ts.server.id,
    ca: tst.station.common_address,
    serverState: '',
  }
}

function closeContextMenu() {
  contextMenu.value.show = false
}

async function ctxStartServer() {
  closeContextMenu()
  try {
    await invoke('start_server', { id: contextMenu.value.serverId })
    await loadTree()
  } catch (e) {
    await showAlert(formatStartServerError(e, t))
  }
}

async function ctxStopServer() {
  closeContextMenu()
  try {
    await invoke('stop_server', { id: contextMenu.value.serverId })
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxDeleteServer() {
  closeContextMenu()
  // 运行中的服务器会先停机再删除,提示语区分两种情形,避免误点直接丢掉在线服务。
  const running = contextMenu.value.serverState === 'Running'
  const ok = await showConfirm(
    running ? t('tree.confirmDeleteRunningServer') : t('tree.confirmDeleteServer'),
  )
  if (!ok) return
  try {
    await invoke('delete_server', { id: contextMenu.value.serverId })
    if (selectedServerId.value === contextMenu.value.serverId) {
      selectedServerId.value = null
    }
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxDeleteStation() {
  closeContextMenu()
  try {
    await invoke('remove_station', {
      serverId: contextMenu.value.serverId,
      commonAddress: contextMenu.value.ca,
    })
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

function ctxEditRuntimeParams() {
  const serverId = contextMenu.value.serverId
  const ts = treeData.value.find(t => t.server.id === serverId)
  const serverLabel = ts ? `${ts.server.bind_address}:${ts.server.port}` : serverId
  closeContextMenu()
  emit('edit-runtime-params', serverId, serverLabel)
}

function isServerSelected(ts: TreeServer): boolean {
  return ts.server.id === selectedServerId.value && selectedCA.value === null
}

function isStationSelected(ts: TreeServer, tst: TreeStation): boolean {
  return ts.server.id === selectedServerId.value
    && tst.station.common_address === selectedCA.value
    && selectedCategory.value === null
}

function isCategorySelected(ts: TreeServer, tst: TreeStation, category: string): boolean {
  return ts.server.id === selectedServerId.value
    && tst.station.common_address === selectedCA.value
    && selectedCategory.value === category
}
</script>

<template>
  <div class="connection-tree" @click="closeContextMenu">
    <div class="tree-header">{{ t('tree.title') }}</div>
    <EmptyState
      v-if="treeData.length === 0"
      compact
      :title="t('tree.noServers')"
      :hint="t('tree.noServersHint')"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
        <rect x="3" y="4" width="18" height="6" rx="1.5" />
        <rect x="3" y="14" width="18" height="6" rx="1.5" />
        <path d="M6.5 7h.01M6.5 17h.01" />
      </svg>
    </EmptyState>

    <div v-for="ts in treeData" :key="ts.server.id" class="tree-node-group">
      <!-- Server Node -->
      <div
        :class="['tree-node server-node', { selected: isServerSelected(ts) }]"
        @click.stop="selectServer(ts)"
        @contextmenu.prevent="showContextMenuForServer($event, ts)"
      >
        <span class="node-arrow" @click.stop="toggleServer(ts)">{{ ts.expanded ? '\u25BC' : '\u25B6' }}</span>
        <span :class="['node-status', ts.server.state === 'Running' ? 'running' : 'stopped']"></span>
        <span class="node-label">{{ ts.server.bind_address }}:{{ ts.server.port }}</span>
      </div>

      <!-- Station Nodes -->
      <template v-if="ts.expanded">
        <div v-for="tst in ts.stations" :key="tst.station.common_address" class="tree-child">
          <div
            :class="['tree-node station-node', { selected: isStationSelected(ts, tst) }]"
            @click.stop="selectStation(ts, tst)"
            @contextmenu.prevent="showContextMenuForStation($event, ts, tst)"
          >
            <span class="node-arrow" @click.stop="toggleStation(tst)">{{ tst.expanded ? '\u25BC' : '\u25B6' }}</span>
            <span class="node-label">{{ tst.station.name || t('station.defaultName', { ca: tst.station.common_address }) }}</span>
            <span class="node-badge">{{ tst.station.point_count }}</span>
          </div>

          <!-- Category Nodes -->
          <template v-if="tst.expanded">
            <div
              v-for="cat in CATEGORIES"
              :key="cat"
              :class="['tree-node category-node', { selected: isCategorySelected(ts, tst, cat) }]"
              @click.stop="selectCategory(ts, tst, cat)"
            >
              <span class="node-label">{{ localizeCategoryLabel(cat) }}</span>
              <span class="node-typeid">{{ CATEGORY_TYPEIDS[cat] }}</span>
              <span class="node-badge" v-if="sharedCategoryCounts.get(cat)">
                {{ sharedCategoryCounts.get(cat) }}
              </span>
            </div>
          </template>
        </div>
      </template>
    </div>

    <!-- Context Menu -->
    <div
      v-if="contextMenu.show"
      class="context-menu"
      :style="{ top: contextMenu.y + 'px', left: contextMenu.x + 'px' }"
      @click.stop
    >
      <template v-if="contextMenu.type === 'server'">
        <div
          v-if="contextMenu.serverState !== 'Running'"
          class="context-menu-item"
          @click="ctxStartServer"
        >{{ t('tree.ctxStartServer') }}</div>
        <div
          v-else
          class="context-menu-item"
          @click="ctxStopServer"
        >{{ t('tree.ctxStopServer') }}</div>
        <div class="context-menu-item" @click="ctxEditRuntimeParams">{{ t('tree.ctxEditRuntimeParams') }}</div>
        <div class="context-menu-item danger" @click="ctxDeleteServer">{{ t('tree.ctxDeleteServer') }}</div>
      </template>
      <template v-if="contextMenu.type === 'station'">
        <div class="context-menu-item" @click="ctxEditRuntimeParams">{{ t('tree.ctxEditRuntimeParams') }}</div>
        <div class="context-menu-item danger" @click="ctxDeleteStation">{{ t('tree.ctxDeleteStation') }}</div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.connection-tree {
  padding: 0;
  font-size: 13px;
  user-select: none;
  height: 100%;
  position: relative;
}

.tree-header {
  padding: 8px 12px;
  font-size: 11px;
  text-transform: uppercase;
  color: var(--c-overlay0);
  letter-spacing: 0.5px;
}

.tree-node {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  cursor: pointer;
  border-radius: 3px;
  margin: 1px 4px;
}

.tree-node:hover {
  background: var(--c-surface0);
}

.tree-node.selected {
  background: var(--c-blue);
  color: var(--c-base);
}

.tree-child {
  padding-left: 16px;
}

.category-node {
  padding-left: 32px;
}

.node-arrow {
  font-size: 8px;
  width: 12px;
  text-align: center;
  flex-shrink: 0;
  color: var(--c-overlay0);
}

.tree-node.selected .node-arrow {
  color: var(--c-base);
}

.node-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.node-status.running {
  background: var(--c-green);
}

.node-status.stopped {
  background: var(--c-surface2);
}

.node-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.node-badge {
  margin-left: auto;
  font-size: 10px;
  color: var(--c-overlay0);
  background: var(--c-surface0);
  padding: 1px 6px;
  border-radius: 8px;
}

.tree-node.selected .node-badge {
  background: rgba(0, 0, 0, 0.2);
  color: var(--c-base);
}

.node-typeid {
  margin-left: auto;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--c-sapphire);
  letter-spacing: 0.3px;
  opacity: 0.85;
  white-space: nowrap;
  flex-shrink: 0;
}

.category-node .node-badge {
  margin-left: 8px;
}

.tree-node.selected .node-typeid {
  color: rgba(30, 30, 46, 0.7);
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
}

.context-menu-item:first-child {
  border-radius: 6px 6px 0 0;
}

.context-menu-item:last-child {
  border-radius: 0 0 6px 6px;
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
</style>
