<script setup lang="ts">
import { ref, inject, watch, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ConnectionInfo, ChangedCategoriesMap, CategoryCountsMap } from '../types'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()

const emit = defineEmits<{
  (e: 'connection-select', id: string, state: string): void
  // ca === null means "all CAs combined" (matches the connection-level
  // category click); otherwise it's the specific CA the user picked.
  (e: 'category-select', connectionId: string, category: string, ca: number | null): void
}>()

const treeRefreshKey = inject<Ref<number>>('treeRefreshKey')!
const refreshTree = inject<() => void>('refreshTree')!
// Provided by Toolbar: opens the new-connection dialog in edit mode for the
// given connection id. Optional — if Toolbar isn't mounted (shouldn't happen
// in this app), the menu item just no-ops.
const openEditConnection = inject<((connId: string) => void) | null>('openEditConnection', null)
const changedCategories = inject<Ref<ChangedCategoriesMap>>('changedCategories')!
const sharedCategoryCounts = inject<Ref<CategoryCountsMap>>('categoryCounts')!
let disposed = false
let loadGeneration = 0

// Flash key 用真实 (connId, ca, category) 三元组。single-CA 视图也能拿到唯一
// CA (`conn.info.common_addresses[0]`),所以不需要 wildcard sentinel。
const flashingCategories = ref<Set<string>>(new Set())
const flashTimers = new Map<string, number>()
const flashKey = (connId: string, ca: number, cat: string) => `${connId}|${ca}|${cat}`

watch(changedCategories, (map) => {
  if (disposed || map.size === 0) return
  for (const [connId, byCa] of map) {
    for (const [ca, cats] of byCa) {
      for (const cat of cats) {
        const key = flashKey(connId, ca, cat)
        flashingCategories.value.add(key)
        const prev = flashTimers.get(key)
        if (prev) clearTimeout(prev)
        flashTimers.set(key, window.setTimeout(() => {
          if (disposed) return
          flashingCategories.value.delete(key)
          flashTimers.delete(key)
        }, 3000))
      }
    }
  }
  changedCategories.value = new Map()
})

onUnmounted(() => {
  disposed = true
  loadGeneration++
  for (const t of flashTimers.values()) clearTimeout(t)
  flashTimers.clear()
})

const DATA_CATEGORIES = [
  'single_point',
  'double_point',
  'step_position',
  'bitstring',
  'normalized_measured',
  'scaled_measured',
  'float_measured',
  'integrated_totals',
].map(key => ({ key }))

// 每个监视方向 category 对应的 ASDU TypeId: 无时标 · CP24 时标 · CP56 时标
// 与 crates/iec104sim-core/src/types.rs::AsduTypeId::category 一致, 与子站树展示对齐
const CATEGORY_TYPEIDS: Record<string, string> = {
  single_point: '1 · 2 · 30',
  double_point: '3 · 4 · 31',
  step_position: '5 · 6 · 32',
  bitstring: '7 · 33',
  normalized_measured: '9 · 10 · 21 · 34',
  scaled_measured: '11 · 12 · 35',
  float_measured: '13 · 14 · 36',
  integrated_totals: '15 · 37',
}

interface TreeConnection {
  info: ConnectionInfo
  expanded: boolean
  // Per-CA expanded state. Keyed by CA value.
  caExpanded: Record<number, boolean>
}

const connections = ref<TreeConnection[]>([])
// Selected node id is one of:
//   "<connId>"                     — the connection node itself
//   "<connId>:ca:<ca>"             — a specific CA group node
//   "<connId>:ca:<ca>:<catKey>"    — a category under a specific CA
//   "<connId>:<catKey>"            — a category under "all CAs" (single-CA case)
const selectedNodeId = ref<string | null>(null)

const contextMenu = ref<{ visible: boolean; x: number; y: number; connId: string }>({
  visible: false, x: 0, y: 0, connId: ''
})

// Look up a count for a specific (conn, ca, category) bucket. ca=null sums
// across every CA (used when the connection has only one CA configured and
// the tree is rendered flat).
function countFor(connId: string, label: string, ca: number | null): number {
  const byCa = sharedCategoryCounts.value.get(connId)
  if (!byCa) return 0
  if (ca !== null) {
    return byCa.get(ca)?.get(label) ?? 0
  }
  let total = 0
  for (const m of byCa.values()) total += m.get(label) ?? 0
  return total
}

// Should we render per-CA sub-nodes for this connection? Yes if the user
// configured multiple CAs (`common_addresses.length > 1`); otherwise the
// classic flat tree is friendlier.
function isMultiCA(conn: TreeConnection): boolean {
  return conn.info.common_addresses.length > 1
}

async function loadTree() {
  const generation = ++loadGeneration
  try {
    const conns = await invoke<ConnectionInfo[]>('list_connections')
    if (disposed || generation !== loadGeneration) return
    const activeIds = new Set(conns.map(c => c.id))
    const newTree: TreeConnection[] = []
    for (const conn of conns) {
      const existing = connections.value.find(c => c.info.id === conn.id)
      // 合并 caExpanded:保留用户对旧 CA 的展开/折叠状态,
      // 把广播 GI debouncer 新学到的 CA 默认 expanded=true。
      // 直接用 `existing?.caExpanded ?? ...` 会让新 CA 永远拿不到 key,
      // 模板 `v-if="caExpanded[ca]"` 拒绝渲染子分类,导致用户感觉"CA 节点不出现"。
      const mergedCaExpanded: Record<number, boolean> = { ...(existing?.caExpanded ?? {}) }
      for (const ca of conn.common_addresses) {
        if (!(ca in mergedCaExpanded)) mergedCaExpanded[ca] = true
      }
      newTree.push({
        info: conn,
        expanded: existing?.expanded ?? true,
        caExpanded: mergedCaExpanded,
      })
    }
    connections.value = newTree

    const staleCounts = [...sharedCategoryCounts.value.keys()].filter(k => !activeIds.has(k))
    if (staleCounts.length > 0) {
      const next = new Map(sharedCategoryCounts.value)
      for (const id of staleCounts) next.delete(id)
      sharedCategoryCounts.value = next
    }
    const staleFlash = [...changedCategories.value.keys()].filter(k => !activeIds.has(k))
    if (staleFlash.length > 0) {
      const next = new Map(changedCategories.value)
      for (const id of staleFlash) next.delete(id)
      changedCategories.value = next
    }
  } catch (_e) {
    // Ignore errors on load
  }
}

watch(treeRefreshKey, loadTree)
onMounted(loadTree)

function selectConnection(conn: TreeConnection) {
  selectedNodeId.value = conn.info.id
  emit('connection-select', conn.info.id, conn.info.state)
}

function selectCategory(conn: TreeConnection, cat: { key: string }, ca: number | null) {
  const id = ca === null
    ? `${conn.info.id}:${cat.key}`
    : `${conn.info.id}:ca:${ca}:${cat.key}`
  selectedNodeId.value = id
  emit('category-select', conn.info.id, cat.key, ca)
}

function toggleExpand(conn: TreeConnection) {
  conn.expanded = !conn.expanded
}

function toggleCAExpand(conn: TreeConnection, ca: number) {
  conn.caExpanded[ca] = !conn.caExpanded[ca]
}

function showContextMenu(e: MouseEvent, connId: string) {
  e.preventDefault()
  contextMenu.value = { visible: true, x: e.clientX, y: e.clientY, connId }
}

function hideContextMenu() {
  contextMenu.value.visible = false
}

async function ctxDeleteConnection() {
  try {
    await invoke('delete_connection', { id: contextMenu.value.connId })
    refreshTree()
  } catch (_e) { /* ignore */ }
  hideContextMenu()
}

function ctxEditConnection() {
  const id = contextMenu.value.connId
  hideContextMenu()
  openEditConnection?.(id)
}

function stateClass(state: string): string {
  const s = state.toLowerCase()
  if (s === 'connected') return 'connected'
  if (s.includes('error')) return 'error'
  return 'disconnected'
}
</script>

<template>
  <div class="tree-container" @click="hideContextMenu">
    <div class="tree-header">{{ t('tree.title') }}</div>

    <div v-if="connections.length === 0" class="tree-empty">
      {{ t('tree.noConnections') }}
    </div>

    <div v-for="conn in connections" :key="conn.info.id" class="tree-node-group">
      <!-- Connection node -->
      <div
        :class="['tree-node', { selected: selectedNodeId === conn.info.id }]"
        @click="selectConnection(conn)"
        @contextmenu="showContextMenu($event, conn.info.id)"
      >
        <span class="node-expand" @click.stop="toggleExpand(conn)">
          {{ conn.expanded ? '▼' : '▶' }}
        </span>
        <span :class="['node-status', stateClass(conn.info.state)]"></span>
        <span class="node-label">{{ conn.info.target_address }}:{{ conn.info.port }}</span>
        <span v-if="conn.info.use_tls || conn.info.use_socks5" class="node-features">
          <button
            v-if="conn.info.use_tls"
            type="button"
            class="feature-badge tls"
            :title="t('tree.editConnection')"
            @click.stop="openEditConnection?.(conn.info.id)"
          >TLS</button>
          <button
            v-if="conn.info.use_socks5"
            type="button"
            class="feature-badge socks"
            :title="t('tree.editConnection')"
            @click.stop="openEditConnection?.(conn.info.id)"
          >S5</button>
        </span>
        <span class="node-ca">CA:{{ conn.info.common_addresses.join(',') }}</span>
      </div>

      <!-- Children -->
      <div v-if="conn.expanded" class="tree-children">

        <!-- Multi-CA: connection -> CA -> category -->
        <template v-if="isMultiCA(conn)">
          <div v-for="ca in conn.info.common_addresses" :key="ca" class="ca-group">
            <div
              :class="['tree-node', 'ca-node', { selected: selectedNodeId === `${conn.info.id}:ca:${ca}` }]"
              @click="toggleCAExpand(conn, ca)"
            >
              <span class="node-expand">{{ conn.caExpanded[ca] ? '▼' : '▶' }}</span>
              <span class="ca-badge">CA {{ ca }}</span>
            </div>
            <div v-if="conn.caExpanded[ca]" class="ca-children">
              <div
                v-for="cat in DATA_CATEGORIES"
                :key="`${ca}-${cat.key}`"
                :class="['tree-node', 'tree-child', 'tree-grand', {
                  selected: selectedNodeId === `${conn.info.id}:ca:${ca}:${cat.key}`,
                  'cat-flash': flashingCategories.has(flashKey(conn.info.id, ca, cat.key)),
                }]"
                @click="selectCategory(conn, cat, ca)"
              >
                <span class="node-label">{{ t(`category.${cat.key}`) }}</span>
                <span class="node-typeid">{{ CATEGORY_TYPEIDS[cat.key] }}</span>
                <span class="node-count" v-if="countFor(conn.info.id, cat.key, ca) > 0">
                  {{ countFor(conn.info.id, cat.key, ca) }}
                </span>
              </div>
            </div>
          </div>
        </template>

        <!-- Single-CA: classic flat tree (counts summed across all CAs, which
             in this case is just the one configured CA). -->
        <template v-else>
          <div
            v-for="cat in DATA_CATEGORIES"
            :key="cat.key"
            :class="['tree-node', 'tree-child', {
              selected: selectedNodeId === `${conn.info.id}:${cat.key}`,
              'cat-flash': flashingCategories.has(flashKey(conn.info.id, conn.info.common_addresses[0], cat.key)),
            }]"
            @click="selectCategory(conn, cat, null)"
          >
            <span class="node-label">{{ t(`category.${cat.key}`) }}</span>
            <span class="node-typeid">{{ CATEGORY_TYPEIDS[cat.key] }}</span>
            <span class="node-count" v-if="countFor(conn.info.id, cat.key, null) > 0">
              {{ countFor(conn.info.id, cat.key, null) }}
            </span>
          </div>
        </template>
      </div>
    </div>

    <div v-if="contextMenu.visible" class="context-menu" :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }">
      <div class="ctx-item" @click="ctxEditConnection">{{ t('tree.editConnection') }}</div>
      <div class="ctx-item danger" @click="ctxDeleteConnection">{{ t('tree.deleteConnection') }}</div>
    </div>
  </div>
</template>

<style scoped>
.tree-container {
  padding: 4px 0;
  font-size: 12px;
  user-select: none;
}

.tree-header {
  padding: 8px 12px;
  font-size: 11px;
  text-transform: uppercase;
  color: var(--c-overlay0);
  letter-spacing: 0.5px;
}

.tree-empty {
  padding: 24px 12px;
  color: var(--c-overlay0);
  text-align: center;
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
  background: var(--c-base);
}

.tree-node.selected {
  background: var(--c-blue);
  color: var(--c-base);
}

.tree-node.selected .node-ca,
.tree-node.selected .node-count,
.tree-node.selected .ca-badge {
  color: var(--c-base);
  opacity: 0.85;
}

.tree-child {
  padding-left: 28px;
}

.tree-grand {
  padding-left: 48px;
}

.ca-node {
  padding-left: 18px;
  font-weight: 500;
}

.ca-badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 10px;
  background: var(--c-surface0);
  color: var(--c-mauve);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.3px;
}

.node-expand {
  font-size: 8px;
  width: 12px;
  text-align: center;
  color: var(--c-overlay0);
}

.node-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.node-status.connected { background: var(--c-green); }
.node-status.disconnected { background: var(--c-overlay0); }
.node-status.error { background: var(--c-red); }

.node-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.node-ca {
  font-size: 10px;
  color: var(--c-overlay0);
}

.node-features {
  display: inline-flex;
  gap: 3px;
  flex-shrink: 0;
}

.feature-badge {
  min-width: 23px;
  padding: 1px 4px;
  border: 1px solid var(--c-surface1);
  border-radius: 8px;
  background: var(--c-surface0);
  font: 600 9px/1.25 var(--font-mono);
  cursor: pointer;
}

.feature-badge.tls { color: var(--c-sapphire); }
.feature-badge.socks { color: var(--c-mauve); }
.feature-badge:hover,
.feature-badge:focus-visible {
  border-color: currentColor;
  outline: none;
}

.tree-node.selected .feature-badge {
  border-color: rgba(30, 30, 46, 0.28);
  background: rgba(30, 30, 46, 0.14);
  color: var(--c-base);
}

.node-typeid {
  margin-left: auto;
  margin-right: 6px;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--c-sapphire);
  letter-spacing: 0.3px;
  opacity: 0.85;
  white-space: nowrap;
  flex-shrink: 0;
}

.tree-node.selected .node-typeid {
  color: rgba(30, 30, 46, 0.7);
}

.node-count {
  font-size: 10px;
  color: var(--c-overlay0);
  background: var(--c-surface0);
  padding: 0 5px;
  border-radius: 8px;
  min-width: 18px;
  text-align: center;
}

.context-menu {
  position: fixed;
  background: var(--c-base);
  border: 1px solid var(--c-surface1);
  border-radius: 6px;
  padding: 4px 0;
  z-index: 999;
  min-width: 120px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

.ctx-item {
  padding: 6px 14px;
  cursor: pointer;
  font-size: 12px;
}

.ctx-item:hover {
  background: var(--c-surface0);
}

.ctx-item.danger {
  color: var(--c-red);
}

.cat-flash {
  background: rgba(250, 179, 135, 0.2) !important;
}

.cat-flash .node-label {
  color: var(--c-peach);
  font-weight: 600;
}
</style>
