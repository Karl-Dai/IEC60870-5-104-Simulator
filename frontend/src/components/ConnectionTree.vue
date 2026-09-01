<script setup lang="ts">
import { ref, inject, watch, onMounted, onBeforeUnmount, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert, showConfirm as ShowConfirm, showPrompt as ShowPrompt } from '@shared/composables/useDialog'
import type { ServerInfo, StationInfo } from '../types'
import { useI18n, localizeCategoryLabel } from '@shared/i18n'
import EmptyState from '@shared/components/EmptyState.vue'
import { formatStartServerError } from '../errors'
import ClientConnectionsModal from './ClientConnectionsModal.vue'

const { t } = useI18n()
const { showAlert, showConfirm, showPrompt } = inject<{
  showAlert: typeof ShowAlert
  showConfirm: typeof ShowConfirm
  showPrompt: typeof ShowPrompt
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

// 每个监视方向 category 对应的 ASDU TypeId: 无时标 · CP24 时标 · CP56 时标
// 与 crates/iec104sim-core/src/types.rs::AsduTypeId::category 一致
const CATEGORY_TYPEIDS: Record<string, string> = {
  single_point: '1 · 2 · 30',
  double_point: '3 · 4 · 31',
  step_position: '5 · 6 · 32',
  bitstring: '7 · 33',
  normalized_measured: '9 · 10 · 21 · 34',
  scaled_measured: '11 · 12 · 35',
  float_measured: '13 · 14 · 36',
  integrated_totals: '15 · 37',
  single_command: '45 · 58',
  double_command: '46 · 59',
  step_command: '47 · 60',
  bitstring_command: '51 · 64',
  normalized_setpoint: '48 · 61',
  scaled_setpoint: '49 · 62',
  float_setpoint: '50 · 63',
}

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
  (e: 'station-select', serverId: string, ca: number, state: string, stationName: string): void
  (e: 'category-select', serverId: string, ca: number, category: string, state: string, stationName: string): void
  (e: 'edit-runtime-params', serverId: string, label: string): void
  (e: 'edit-server', serverId: string): void
}>()

const treeRefreshKey = inject<Ref<number>>('treeRefreshKey')!
const dataRefreshKey = inject<Ref<number>>('dataRefreshKey')!
const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const selectedCategory = inject<Ref<string | null>>('selectedCategory')!

const treeData = ref<TreeServer[]>([])
const connectionsVisible = ref(false)
const connectionsServerId = ref('')
const connectionsServerLabel = ref('')
const contextMenu = ref({
  show: false,
  x: 0,
  y: 0,
  type: '' as 'server' | 'station',
  serverId: '',
  ca: 0,
  stationName: '',
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

// Connection count is live runtime state, unlike station definitions. Refresh
// only the lightweight server snapshots so the tree does not re-fetch every
// station once per second or lose the user's expanded/collapsed state.
async function refreshServerRuntime() {
  try {
    const servers = await invoke<ServerInfo[]>('list_servers')
    const latestById = new Map(servers.map(server => [server.id, server]))
    for (const treeServer of treeData.value) {
      const latest = latestById.get(treeServer.server.id)
      if (latest) treeServer.server = latest
    }
  } catch {
    // The next polling cycle retries; transient IPC failures should not disturb
    // the existing tree.
  }
}

let runtimeRefreshTimer: number | undefined
let runtimeRefreshStopped = false
function scheduleRuntimeRefresh() {
  if (runtimeRefreshStopped) return
  runtimeRefreshTimer = window.setTimeout(async () => {
    await refreshServerRuntime()
    scheduleRuntimeRefresh()
  }, 1000)
}

// Point-definition mutations bump dataRefreshKey. Refresh the lightweight
// station snapshots as well so their category totals stay current without
// coupling the tree to whichever station the large point table has loaded.
watch([treeRefreshKey, dataRefreshKey], () => loadTree())
onMounted(() => {
  runtimeRefreshStopped = false
  void loadTree()
  scheduleRuntimeRefresh()
})
onBeforeUnmount(() => {
  runtimeRefreshStopped = true
  if (runtimeRefreshTimer !== undefined) window.clearTimeout(runtimeRefreshTimer)
})

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
  emit('station-select', ts.server.id, tst.station.common_address, ts.server.state, tst.station.name)
}

function selectCategory(ts: TreeServer, tst: TreeStation, category: string) {
  emit('category-select', ts.server.id, tst.station.common_address, category, ts.server.state, tst.station.name)
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
    stationName: '',
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
    stationName: tst.station.name,
    serverState: ts.server.state,
  }
}

function closeContextMenu() {
  contextMenu.value.show = false
}

function clientCount(server: ServerInfo) {
  return Number.isFinite(server.client_count) ? server.client_count : 0
}

function openClientConnections(treeServer: TreeServer) {
  connectionsServerId.value = treeServer.server.id
  connectionsServerLabel.value = `${treeServer.server.bind_address}:${treeServer.server.port}`
  connectionsVisible.value = true
  closeContextMenu()
}

function ctxViewClientConnections() {
  const treeServer = treeData.value.find(item => item.server.id === contextMenu.value.serverId)
  if (treeServer) openClientConnections(treeServer)
  else closeContextMenu()
}

function contextServerClientCount() {
  const server = treeData.value.find(item => item.server.id === contextMenu.value.serverId)?.server
  return server ? clientCount(server) : 0
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

// 删除服务器前必须确认(issue #28);运行中的服务器用更重的措辞,
// 提示会先停止、且未保存的点表数据会丢失(可先「保存配置」)。
async function ctxDeleteServer() {
  const { serverId, serverState } = contextMenu.value
  closeContextMenu()
  const ts = treeData.value.find(item => item.server.id === serverId)
  const label = ts ? `${ts.server.bind_address}:${ts.server.port}` : serverId
  const message = serverState === 'Running'
    ? t('tree.confirmDeleteRunningServer', { server: label })
    : t('tree.confirmDeleteServer', { server: label })
  if (!(await showConfirm(message))) return
  try {
    await invoke('delete_server', { id: serverId })
    if (selectedServerId.value === serverId) {
      selectedServerId.value = null
    }
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxDeleteStation() {
  const { serverId, ca } = contextMenu.value
  closeContextMenu()
  if (!(await showConfirm(t('tree.confirmDeleteStation', { ca })))) return
  try {
    await invoke('remove_station', {
      serverId,
      commonAddress: ca,
    })
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function ctxEditStation() {
  const {
    serverId,
    ca: currentCommonAddress,
    stationName,
    serverState,
  } = contextMenu.value
  closeContextMenu()

  const caText = await showPrompt(t('prompt.inputCommonAddress'), String(currentCommonAddress))
  if (caText === null) return
  const commonAddress = Number(caText)
  if (!Number.isInteger(commonAddress) || commonAddress < 1 || commonAddress > 65534) {
    await showAlert(t('errors.invalidCa'))
    return
  }
  if (commonAddress !== currentCommonAddress && serverState === 'Running') {
    await showAlert(t('errors.stationCaRunning'))
    return
  }

  const name = await showPrompt(t('prompt.inputStationName'), stationName)
  if (name === null) return
  try {
    await invoke('update_station', {
      request: {
        server_id: serverId,
        current_common_address: currentCommonAddress,
        common_address: commonAddress,
        name: name.trim(),
      },
    })
    emit('station-select', serverId, commonAddress, serverState, name.trim())
    await loadTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

function stationLabel(station: StationInfo) {
  return t('station.displayName', {
    name: station.name.trim() || t('station.unnamedName'),
    ca: station.common_address,
  })
}

function categoryCount(station: StationInfo, category: string): number {
  return station.category_counts?.[category] ?? 0
}

function ctxEditRuntimeParams() {
  const { serverId, ca, type, serverState } = contextMenu.value
  const ts = treeData.value.find(t => t.server.id === serverId)
  const serverLabel = ts ? `${ts.server.bind_address}:${ts.server.port}` : serverId
  closeContextMenu()
  // issue #28:工具栏抽屉与数据表的 +TB 徽标读的是【树选中的】selectedServerId,
  // 而右键弹窗走独立的 serverId。若右键的不是当前选中节点(含树上什么都没选),
  // 先把树选中对齐到右键的服务器 —— 复用 server-select / station-select 的完整语义
  // (顺带清掉陈旧的 CA / 分类 / 点位选择),否则改完 B 的参数抽屉和徽标仍显示 A。
  if (type === 'station') {
    // 仅在服务器不一致时才对齐(用 station-select 而非 server-select,顺带对上 CA,
    // 保存后当前点表直接能看到 +TB 徽标)。服务器本已一致时不动用户当前查看的站 ——
    // 右键看一眼服务器级参数不该把点表切到别的站。
    if (selectedServerId.value !== serverId) {
      const station = ts?.stations.find(item => item.station.common_address === ca)?.station
      emit('station-select', serverId, ca, ts ? ts.server.state : serverState, station?.name ?? '')
    }
  } else if (selectedServerId.value !== serverId) {
    emit('server-select', serverId, ts ? ts.server.state : serverState)
  }
  emit('edit-runtime-params', serverId, serverLabel)
}

function ctxEditServer() {
  const id = contextMenu.value.serverId
  closeContextMenu()
  emit('edit-server', id)
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
        <span v-if="ts.server.use_tls" class="tls-badge">TLS</span>
        <button
          type="button"
          class="client-count-badge"
          :title="t('tree.connTooltip', { n: clientCount(ts.server) })"
          :aria-label="t('tree.connTooltip', { n: clientCount(ts.server) })"
          @click.stop="openClientConnections(ts)"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M6.3 5.2H4.8a2.8 2.8 0 1 0 0 5.6h1.5M9.7 5.2h1.5a2.8 2.8 0 1 1 0 5.6H9.7M5.8 8h4.4" />
          </svg>
          <span>{{ clientCount(ts.server) }}</span>
        </button>
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
            <span class="node-label">{{ stationLabel(tst.station) }}</span>
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
              <span class="node-badge" v-if="categoryCount(tst.station, cat)">
                {{ categoryCount(tst.station, cat) }}
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
        <div class="context-menu-item" @click="ctxViewClientConnections">
          {{ t('tree.ctxViewConnections', { n: contextServerClientCount() }) }}
        </div>
        <div class="context-menu-item" @click="ctxEditServer">{{ t('serverSettings.entry') }}</div>
        <div class="context-menu-item" @click="ctxEditRuntimeParams">{{ t('tree.ctxEditRuntimeParams') }}</div>
        <div class="context-menu-item danger" @click="ctxDeleteServer">{{ t('tree.ctxDeleteServer') }}</div>
      </template>
      <template v-if="contextMenu.type === 'station'">
        <div class="context-menu-item" @click="ctxEditStation">{{ t('tree.ctxEditStation') }}</div>
        <div class="context-menu-item" @click="ctxEditRuntimeParams">{{ t('tree.ctxEditRuntimeParams') }}</div>
        <div class="context-menu-item danger" @click="ctxDeleteStation">{{ t('tree.ctxDeleteStation') }}</div>
      </template>
    </div>

    <ClientConnectionsModal
      :visible="connectionsVisible"
      :server-id="connectionsServerId"
      :server-label="connectionsServerLabel"
      @close="connectionsVisible = false"
    />
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
.tls-badge { flex-shrink: 0; padding: 1px 4px; border-radius: 3px; background: color-mix(in srgb, var(--c-blue) 15%, transparent); color: var(--c-blue); font-size: 10px; line-height: 1.4; }

.client-count-badge {
  margin-left: auto;
  min-width: 34px;
  height: 20px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  flex-shrink: 0;
  color: var(--c-overlay1);
  background: var(--c-surface0);
  border: 1px solid transparent;
  border-radius: 10px;
  font: 10px/1 var(--font-mono);
  cursor: pointer;
}

.client-count-badge:hover {
  color: var(--c-green);
  border-color: color-mix(in srgb, var(--c-green) 45%, transparent);
}

.client-count-badge svg {
  width: 11px;
  height: 11px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
}

.tree-node.selected .client-count-badge {
  color: var(--c-base);
  background: rgba(0, 0, 0, 0.18);
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
