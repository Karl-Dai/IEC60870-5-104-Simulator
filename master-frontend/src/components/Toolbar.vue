<script setup lang="ts">
import { computed, inject, ref, watch, onMounted, onBeforeUnmount, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save, open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert, showConfirm as ShowConfirm } from '@shared/composables/useDialog'
import AboutDialog from '@shared/components/AboutDialog.vue'
import ToolbarMenu from '@shared/components/ToolbarMenu.vue'
import type { ToolbarMenuItem } from '@shared/components/toolbarMenu'
import ControlDialog from './ControlDialog.vue'
import NewConnectionModal from './NewConnectionModal.vue'
import LangSwitch from '@shared/components/LangSwitch.vue'
import ThemeSwitch from '@shared/components/ThemeSwitch.vue'
import VersionBadge from '@shared/components/VersionBadge.vue'
import { useI18n } from '@shared/i18n'

const { t } = useI18n()

const { showAlert, showConfirm } = inject<{
  showAlert: typeof ShowAlert
  showConfirm: typeof ShowConfirm
}>(dialogKey)!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!
const selectedConnectionId = inject<Ref<string | null>>('selectedConnectionId')!
const selectedConnectionState = inject<Ref<string>>('selectedConnectionState')!
const refreshTree = inject<() => void>('refreshTree')!
const refreshData = inject<() => void>('refreshData')!
const resetWorkspaceView = inject<() => void>('resetWorkspaceView')!
type UpdateMeta = { version: string; notes: string; pub_date?: string | null }
const checkUpdate = inject<(force?: boolean) => Promise<UpdateMeta | null>>('checkUpdate')!
const updateChecking = ref(false)
const MIRROR_RELEASE_URL = 'https://ghfast.top/https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/latest'

async function manualCheckUpdate() {
  if (updateChecking.value) return
  updateChecking.value = true
  try {
    const meta = await checkUpdate(true)
    if (!meta) await showAlert(t('toolbar.alreadyLatest'))
  } catch (e) {
    console.warn('update check failed', e)
    const wantMirror = await showConfirm(t('toolbar.updateCheckFailedMirrorPrompt'))
    if (wantMirror) {
      try {
        await openUrl(MIRROR_RELEASE_URL)
      } catch (err) {
        await showAlert(`${t('toolbar.updateCheckFailed')}: ${err}`)
      }
    }
  } finally {
    updateChecking.value = false
  }
}

const broadcastAddrLabel = ref('FFFF')

async function loadBroadcastAddr() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) {
    broadcastAddrLabel.value = 'FFFF'
    return
  }
  try {
    const conns = await invoke<any[]>('list_connections')
    if (selectedConnectionId.value !== connectionId) return
    const c = conns.find((x: any) => x.id === connectionId)
    const v = c?.broadcast_address ?? 0xFFFF
    broadcastAddrLabel.value = v.toString(16).toUpperCase().padStart(4, '0')
  } catch {
    if (selectedConnectionId.value === connectionId) broadcastAddrLabel.value = 'FFFF'
  }
}

const showAbout = ref(false)

// Free-form control dialog (entry from the toolbar; no preselected point)
const showCustomControl = ref(false)
const customControlCA = ref<number>(1)
async function openCustomControl() {
  customControlCA.value = 1
  const connectionId = selectedConnectionId.value
  // If a connection is selected, default the dialog's CA to its first
  // configured Common Address — saves the user a step in single-CA setups
  // and gives a sensible starting point in multi-CA ones.
  if (connectionId) {
    try {
      const conns = await invoke<{ id: string; common_addresses: number[] }[]>('list_connections')
      if (selectedConnectionId.value !== connectionId) return
      const conn = conns.find((c) => c.id === connectionId)
      if (conn?.common_addresses?.length) customControlCA.value = conn.common_addresses[0]
    } catch {
      if (selectedConnectionId.value !== connectionId) return
      // Ignore current-workspace lookup failures and fall back to CA 1.
    }
  }
  showCustomControl.value = true
}

// New Connection modal — owned by NewConnectionModal.vue. We expose
// openEditConnection here so App.vue's provide('openEditConnection') can
// forward right-click "Edit" actions from ConnectionTree (Toolbar and
// ConnectionTree are sibling components — provide can't bridge siblings).
const showNewConn = ref(false)
const newConnModalRef = ref<InstanceType<typeof NewConnectionModal> | null>(null)
function openEditConnection(connId: string) {
  return newConnModalRef.value?.openEditConnection(connId)
}
function editSelectedConnection() {
  const connId = selectedConnectionId.value
  if (connId) return openEditConnection(connId)
}
function openNewConnection() {
  newConnModalRef.value?.openNew()
}
defineExpose({ openEditConnection })

async function getConnCAs(connectionId: string): Promise<number[] | null> {
  const conns = await invoke<any[]>('list_connections')
  if (selectedConnectionId.value !== connectionId) return null
  const conn = conns.find((c: any) => c.id === connectionId)
  const list: unknown = conn?.common_addresses
  if (Array.isArray(list) && list.length > 0) return list as number[]
  return [conn?.common_address ?? 1]
}

// Fan out a per-CA invocation across all CAs of the current connection
// concurrently. Backend serializes I-frame writes via send_lock, but
// running the IPC round-trips in parallel still saves a 3×CA latency multiplier.
async function fanOutCAs(cmd: string, connectionId: string): Promise<boolean> {
  const cas = await getConnCAs(connectionId)
  if (cas === null || selectedConnectionId.value !== connectionId) return false
  await Promise.all(
    cas.map((ca) => invoke(cmd, { id: connectionId, commonAddress: ca })),
  )
  return selectedConnectionId.value === connectionId
}

async function connectMaster() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    await invoke('connect_master', { id: connectionId })
    if (selectedConnectionId.value !== connectionId) return
    selectedConnectionState.value = 'Connected'
    refreshTree()
    // 连接后不再自动总召唤:旧逻辑对所有 CA 并发 GI,会触发远端对未配置的
    // CA 报错甚至主动断链。改由用户手动点"总召唤"按钮按需选择 CA。
  } catch (e) {
    if (selectedConnectionId.value === connectionId) {
      await showAlert(String(e))
    }
  }
}

async function disconnectMaster() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  let alertErr: unknown = null
  try {
    await invoke('disconnect_master', { id: connectionId })
  } catch (e) {
    // "NotConnected" is benign: backend already saw the socket close before
    // the user clicked. For any other error we still surface it but also
    // force the UI to Disconnected so the user isn't stuck with a dead
    // button while the backend reconciles.
    const msg = String(e)
    if (!msg.includes('NotConnected') && !msg.includes('not connected')) {
      alertErr = e
    }
  } finally {
    if (selectedConnectionId.value === connectionId) {
      selectedConnectionState.value = 'Disconnected'
      refreshTree()
    }
  }
  if (alertErr !== null && selectedConnectionId.value === connectionId) {
    await showAlert(String(alertErr))
  }
}

async function deleteMaster() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    await invoke('delete_connection', { id: connectionId })
    if (selectedConnectionId.value !== connectionId) return
    selectedConnectionId.value = null
    selectedConnectionState.value = 'Disconnected'
    refreshTree()
  } catch (e) {
    if (selectedConnectionId.value === connectionId) {
      await showAlert(String(e))
    }
  }
}

// 发送总召唤。ca 为具体公共地址;ca === null 表示对所有 CA 并发(菜单"全部 CA")。
async function doGI(ca: number | null, connectionId: string | null) {
  if (!connectionId || selectedConnectionId.value !== connectionId) return
  try {
    if (ca === null) {
      if (!await fanOutCAs('send_interrogation', connectionId)) return
    } else {
      await invoke('send_interrogation', { id: connectionId, commonAddress: ca })
    }
    if (selectedConnectionId.value !== connectionId) return
    refreshData()
    setTimeout(() => {
      if (selectedConnectionId.value === connectionId) refreshTree()
    }, 3000)
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

// 发送停止激活(COT=8)总召唤。ca === null 表示对所有 CA 并发取消进行中的 GI。
async function doGIDeactivation(ca: number | null, connectionId: string | null) {
  if (!connectionId || selectedConnectionId.value !== connectionId) return
  try {
    if (ca === null) {
      if (!await fanOutCAs('send_interrogation_deactivation', connectionId)) return
    } else {
      await invoke('send_interrogation_deactivation', { id: connectionId, commonAddress: ca })
    }
    if (selectedConnectionId.value !== connectionId) return
  } catch (err) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(err))
  }
}

async function sendClockSync() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    if (!await fanOutCAs('send_clock_sync', connectionId)) return
    if (selectedConnectionId.value !== connectionId) return
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

// 发送计量召唤。ca 为具体公共地址;ca === null 表示对所有 CA 并发(菜单"全部 CA")。
async function doCounterRead(ca: number | null, connectionId: string | null) {
  if (!connectionId || selectedConnectionId.value !== connectionId) return
  try {
    if (ca === null) {
      if (!await fanOutCAs('send_counter_read', connectionId)) return
    } else {
      await invoke('send_counter_read', { id: connectionId, commonAddress: ca })
    }
    if (selectedConnectionId.value !== connectionId) return
    refreshData()
    setTimeout(() => {
      if (selectedConnectionId.value === connectionId) refreshTree()
    }, 3000)
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

// 发送停止激活(COT=8)计数量召唤。ca === null 表示对所有 CA 并发取消进行中的累计量扫描。
async function doCounterReadDeactivation(ca: number | null, connectionId: string | null) {
  if (!connectionId || selectedConnectionId.value !== connectionId) return
  try {
    if (ca === null) {
      if (!await fanOutCAs('send_counter_read_deactivation', connectionId)) return
    } else {
      await invoke('send_counter_read_deactivation', { id: connectionId, commonAddress: ca })
    }
    if (selectedConnectionId.value !== connectionId) return
  } catch (err) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(err))
  }
}

async function saveConfig() {
  const path = await save({
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
    defaultPath: 'iec104-master-config.json',
  })
  if (!path) return
  try {
    await invoke('save_config', { path })
    await showAlert(t('toolbar.configSaved'))
  } catch (e) {
    await showAlert(`${t('toolbar.configSaveFailed')}: ${e}`)
  }
}

async function openConfig() {
  const path = await open({
    multiple: false,
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
  })
  if (!path || typeof path !== 'string') return
  try {
    const count = await invoke<number>('load_config', { path })
    resetWorkspaceView()
    refreshTree()
    refreshData()
    await showAlert(t('toolbar.configLoaded', { count }))
  } catch (e) {
    await showAlert(`${t('toolbar.configLoadFailed')}: ${e}`)
  }
}

const isConnected = () => selectedConnectionState.value === 'Connected'
const hasConnection = () => selectedConnectionId.value !== null

async function sendBroadcastGI() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    await invoke('send_broadcast_gi', { id: connectionId })
    if (selectedConnectionId.value !== connectionId) return
    // 树刷新由后端 `connection-cas-updated` 事件触发(debouncer 1s 安静期后 flush),
    // 不再走固定 3500ms setTimeout fallback,避免延迟感。
    refreshData()
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

async function sendBroadcastCounterRead() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    await invoke('send_broadcast_counter_read', { id: connectionId })
    if (selectedConnectionId.value !== connectionId) return
    refreshData()
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

async function sendBroadcastGIDeactivation() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    await invoke('send_broadcast_gi_deactivation', { id: connectionId })
    if (selectedConnectionId.value !== connectionId) return
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

async function sendBroadcastCounterReadDeactivation() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    await invoke('send_broadcast_counter_read_deactivation', { id: connectionId })
    if (selectedConnectionId.value !== connectionId) return
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

type CACommand = 'gi' | 'stop-gi' | 'counter' | 'stop-counter'
const caActions = { gi: doGI, 'stop-gi': doGIDeactivation, counter: doCounterRead, 'stop-counter': doCounterReadDeactivation }
const caLabels = { gi: 'toolbar.sendGI', 'stop-gi': 'toolbar.deactivateGI', counter: 'toolbar.counterRead', 'stop-counter': 'toolbar.deactivateCounterRead' } as const
const openMenu = ref<string | null>(null)
const loadingCAs = ref(false)
const caSelection = ref<{ menu: string; command: CACommand; connectionId: string; cas: number[] } | null>(null)
let lookupVersion = 0
function closeMenu() {
  lookupVersion++
  openMenu.value = null
  caSelection.value = null
  loadingCAs.value = false
}
function toggleMenu(id: string) {
  const wasOpen = openMenu.value === id
  closeMenu()
  if (!wasOpen) openMenu.value = id
}
function backToCommands() {
  lookupVersion++
  caSelection.value = null
  loadingCAs.value = false
}
async function requestCAs(command: CACommand, menu = 'commands') {
  const connectionId = selectedConnectionId.value
  if (menu === 'quick-gi' && openMenu.value === menu) { closeMenu(); return }
  if (!connectionId || !isConnected() || loadingCAs.value) return
  openMenu.value = menu
  caSelection.value = null
  loadingCAs.value = true
  const version = ++lookupVersion
  try {
    const cas = await getConnCAs(connectionId)
    if (version !== lookupVersion || cas === null || selectedConnectionId.value !== connectionId || !isConnected()) return
    if (cas.length <= 1) {
      closeMenu()
      await caActions[command](cas[0] ?? null, connectionId)
    } else {
      caSelection.value = { menu, command, connectionId, cas }
    }
  } catch (error) {
    if (version === lookupVersion) { closeMenu(); await showAlert(String(error)) }
  } finally {
    if (version === lookupVersion) loadingCAs.value = false
  }
}
const caItems = computed<ToolbarMenuItem[]>(() => {
  const selection = caSelection.value
  if (!selection) return []
  return [null, ...selection.cas].map(ca => ({
    id: ca === null ? 'ca-all' : `ca-${ca}`,
    label: ca === null ? t('toolbar.giAllCAs') : `CA ${ca}`,
    action: () => caActions[selection.command](ca, selection.connectionId),
  }))
})
const unavailable = computed(() => !hasConnection() || !isConnected())
const menus = computed(() => [
  { id: 'config', label: t('toolbar.menuConfig'), items: [
    { id: 'open-config', label: t('toolbar.openConfig'), action: openConfig },
    { id: 'save-config', label: t('toolbar.saveConfig'), action: saveConfig },
  ] },
  { id: 'connection', label: t('toolbar.menuConnection'), items: [
    { id: 'new-connection', label: t('toolbar.newConnection'), action: openNewConnection },
    { id: 'edit-connection', label: t('toolbar.editConnection'), disabled: !hasConnection(), action: editSelectedConnection },
    { id: 'delete-connection', label: t('toolbar.delete'), disabled: !hasConnection(), danger: true, separator: true, action: deleteMaster },
  ] },
  { id: 'commands', label: t('toolbar.menuCommands'), items: [
    ...(['gi', 'stop-gi', 'counter', 'stop-counter'] as CACommand[]).map(command => ({
      id: command, label: t(caLabels[command]), disabled: unavailable.value, keepOpen: true,
      separator: command === 'counter', action: () => requestCAs(command),
    })),
    { id: 'clock-sync', label: t('toolbar.clockSync'), disabled: unavailable.value, separator: true, action: sendClockSync },
    { id: 'custom-control', label: t('toolbar.customControl'), disabled: unavailable.value, action: openCustomControl },
  ] },
  { id: 'broadcast', label: t('toolbar.broadcast'), items: [
    { id: 'broadcast-gi', label: t('toolbar.broadcastGi'), disabled: unavailable.value, action: sendBroadcastGI },
    { id: 'broadcast-counter', label: t('toolbar.broadcastCounterRead'), disabled: unavailable.value, action: sendBroadcastCounterRead },
    { id: 'broadcast-stop-gi', label: t('toolbar.broadcastGiDeactivation'), disabled: unavailable.value, separator: true, action: sendBroadcastGIDeactivation },
    { id: 'broadcast-stop-counter', label: t('toolbar.broadcastCounterReadDeactivation'), disabled: unavailable.value, action: sendBroadcastCounterReadDeactivation },
  ] },
  { id: 'tools', label: t('toolbar.menuTools'), items: [
    { id: 'parse-frame', label: t('toolbar.parseFrame'), action: () => openParseFrame() },
  ] },
])
const helpItems = computed(() => [
  { id: 'check-update', label: updateChecking.value ? t('toolbar.checkingUpdate') : t('toolbar.checkUpdate'), disabled: updateChecking.value, busy: updateChecking.value, action: manualCheckUpdate },
  { id: 'about', label: t('toolbar.about'), action: () => { showAbout.value = true } },
])
function menuHeading(id: string) {
  if (openMenu.value === id && loadingCAs.value) return t('common.loading')
  if (caSelection.value?.menu === id) return t(caLabels[caSelection.value.command])
  if (id === 'broadcast') return `${t('toolbar.broadcastAddressLabel')}: 0x${broadcastAddrLabel.value}`
  return undefined
}
// Resizing can hide the GI shortcut; never leave its popup without a visible anchor.
onMounted(() => window.addEventListener('resize', closeMenu))
onBeforeUnmount(() => window.removeEventListener('resize', closeMenu))
watch([selectedConnectionId, selectedConnectionState], () => {
  closeMenu()
  void loadBroadcastAddr()
}, { immediate: true })

</script>

<template>
  <div class="toolbar master-toolbar">
    <div class="toolbar-main">
      <ToolbarMenu v-for="menu in menus" :key="menu.id" :id="menu.id" :label="menu.label"
        :items="caSelection?.menu === menu.id ? caItems : menu.items" :open="openMenu === menu.id"
        :heading="menuHeading(menu.id)" :loading="openMenu === menu.id && loadingCAs"
        :back-label="caSelection?.menu === menu.id ? t('toolbar.menuCommands') : undefined"
        @toggle="toggleMenu(menu.id)" @close="closeMenu" @back="backToCommands" />
      <div class="toolbar-divider" aria-hidden="true"></div>
      <div class="toolbar-group" role="group" :aria-label="t('toolbar.menuConnection')">
        <button type="button" class="toolbar-btn btn-start" data-testid="connect"
          :disabled="!hasConnection() || isConnected() || selectedConnectionState === 'Connecting'" @click="connectMaster">
          {{ t('toolbar.connect') }}
        </button>
        <button type="button" class="toolbar-btn btn-stop" data-testid="disconnect"
          :disabled="unavailable" @click="disconnectMaster">{{ t('toolbar.disconnect') }}</button>
      </div>
      <div class="gi-shortcut">
        <ToolbarMenu id="quick-gi" :label="t('toolbar.sendGI')" :items="caItems" :disabled="unavailable"
          :open="openMenu === 'quick-gi'" :loading="openMenu === 'quick-gi' && loadingCAs"
          :heading="menuHeading('quick-gi')" @toggle="requestCAs('gi', 'quick-gi')" @close="closeMenu" />
      </div>
    </div>
    <div class="toolbar-aside">
      <ToolbarMenu id="help" :label="t('toolbar.menuHelp')" :items="helpItems" :open="openMenu === 'help'"
        @toggle="toggleMenu('help')" @close="closeMenu" />
      <LangSwitch />
      <ThemeSwitch />
      <VersionBadge />
    </div>
  </div>

  <AboutDialog :visible="showAbout" @close="showAbout = false" />

  <!-- Free-form control dialog. The user can pick a CA, type any IOA,
       choose a command type, and send — independent of any selected
       data point. Useful for sending control commands to IOAs that
       haven't been received yet (e.g. write-only points). -->
  <ControlDialog
    :visible="showCustomControl"
    :connection-id="selectedConnectionId"
    :common-address="customControlCA"
    :prefill-ioa="null"
    :prefill-command-type="null"
    @close="showCustomControl = false"
  />

  <NewConnectionModal ref="newConnModalRef" v-model:visible="showNewConn" />
</template>

<style scoped>
.master-toolbar { min-height: 42px; height: auto; padding-block: 5px; gap: 8px; flex-wrap: wrap; }
.master-toolbar .toolbar-main { overflow: visible; flex-wrap: wrap; row-gap: 4px; }
.master-toolbar :deep(.toolbar-btn) { padding: 5px 7px; }
.master-toolbar :deep(.toolbar-btn:focus-visible) { outline: 2px solid var(--accent); outline-offset: -2px; }
.master-toolbar .btn-start:not(:disabled) { background: color-mix(in srgb, var(--success) 14%, transparent); }
.master-toolbar .toolbar-aside { margin-left: auto; }
.gi-shortcut { flex: none; }
@media (max-width: 1050px) { .gi-shortcut { display: none; } }
</style>
