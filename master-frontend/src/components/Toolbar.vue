<script setup lang="ts">
import { inject, ref, watch, onMounted, onBeforeUnmount, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save, open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert, showConfirm as ShowConfirm } from '@shared/composables/useDialog'
import AboutDialog from '@shared/components/AboutDialog.vue'
import ControlDialog from './ControlDialog.vue'
import NewConnectionModal from './NewConnectionModal.vue'
import LangSwitch from '@shared/components/LangSwitch.vue'
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

const broadcastMenuOpen = ref(false)
const broadcastAddrLabel = ref('FFFF')

// 总召唤改为"选 CA"交互:多 CA 连接点按钮弹出菜单(全部 CA / 各 CA),
// 单 CA 连接直接发。connCAs 缓存当前连接的 CA 列表,用于按钮 ▾ 提示。
const giMenuOpen = ref(false)
const giCAs = ref<number[]>([])
const giMenuConnectionId = ref<string | null>(null)
// 计量召唤(C_CI)沿用与总召相同的"选 CA"交互:多 CA 弹菜单,单 CA 直发。
const ccMenuOpen = ref(false)
const ccCAs = ref<number[]>([])
const ccMenuConnectionId = ref<string | null>(null)
// 停止激活(COT=8)去激活也支持"选 CA":多 CA 弹菜单,单 CA 直发。
const giDeactMenuOpen = ref(false)
const giDeactCAs = ref<number[]>([])
const giDeactMenuConnectionId = ref<string | null>(null)
const ccDeactMenuOpen = ref(false)
const ccDeactCAs = ref<number[]>([])
const ccDeactMenuConnectionId = ref<string | null>(null)
const connCAs = ref<number[]>([])

// Dropdown menus are teleported to <body> so the toolbar's horizontal-scroll
// container can't clip them. That means fixed-positioning them from the
// trigger's viewport rect, captured the moment the menu opens.
const giMenuPos = ref({ top: 0, left: 0 })
const ccMenuPos = ref({ top: 0, left: 0 })
const giDeactMenuPos = ref({ top: 0, left: 0 })
const ccDeactMenuPos = ref({ top: 0, left: 0 })
const broadcastMenuPos = ref({ top: 0, left: 0 })
function anchorPos(el: HTMLElement) {
  const r = el.getBoundingClientRect()
  return { top: r.bottom + 2, left: r.left }
}
function toggleBroadcastMenu(e: MouseEvent) {
  const wrap = (e.currentTarget as HTMLElement).closest('.split-btn') as HTMLElement | null
  broadcastMenuPos.value = anchorPos(wrap ?? (e.currentTarget as HTMLElement))
  broadcastMenuOpen.value = !broadcastMenuOpen.value
}

async function loadConnCAs() {
  const connectionId = selectedConnectionId.value
  if (!connectionId) { connCAs.value = []; return }
  try {
    const cas = await getConnCAs(connectionId)
    if (cas !== null && selectedConnectionId.value === connectionId) connCAs.value = cas
  } catch {
    if (selectedConnectionId.value === connectionId) connCAs.value = []
  }
}

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

watch(selectedConnectionId, () => {
  broadcastMenuOpen.value = false
  giMenuOpen.value = false
  giMenuConnectionId.value = null
  giDeactMenuOpen.value = false
  giDeactMenuConnectionId.value = null
  ccMenuOpen.value = false
  ccMenuConnectionId.value = null
  ccDeactMenuOpen.value = false
  ccDeactMenuConnectionId.value = null
  void loadBroadcastAddr()
  void loadConnCAs()
}, { immediate: true })

function closeBroadcastMenu(e: MouseEvent) {
  const el = e.target as HTMLElement
  if (!el.closest('.split-btn')) broadcastMenuOpen.value = false
  if (!el.closest('.gi-btn-wrap')) giMenuOpen.value = false
  if (!el.closest('.cc-btn-wrap')) ccMenuOpen.value = false
  if (!el.closest('.gi-deact-wrap')) giDeactMenuOpen.value = false
  if (!el.closest('.cc-deact-wrap')) ccDeactMenuOpen.value = false
}
onMounted(() => document.addEventListener('click', closeBroadcastMenu))
onBeforeUnmount(() => document.removeEventListener('click', closeBroadcastMenu))

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

// 点"总召唤":单 CA 连接直接发;多 CA 连接弹出菜单让用户选具体 CA 或全部。
async function sendGI(e: MouseEvent) {
  // Capture the anchor synchronously — `currentTarget` is nulled after the await.
  const anchor = anchorPos(e.currentTarget as HTMLElement)
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    const cas = await getConnCAs(connectionId)
    if (cas === null || selectedConnectionId.value !== connectionId) return
    giCAs.value = cas
    if (cas.length <= 1) {
      await doGI(cas[0] ?? null, connectionId)
    } else {
      giMenuConnectionId.value = connectionId
      giMenuPos.value = anchor
      giMenuOpen.value = !giMenuOpen.value
    }
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

// 发送总召唤。ca 为具体公共地址;ca === null 表示对所有 CA 并发(菜单"全部 CA")。
async function doGI(ca: number | null, connectionId: string | null) {
  giMenuOpen.value = false
  giMenuConnectionId.value = null
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

// 停止激活(COT=8)总召唤:单 CA 直发,多 CA 弹菜单选具体 CA 或全部。
async function sendGIDeactivation(e: MouseEvent) {
  const anchor = anchorPos(e.currentTarget as HTMLElement)
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    const cas = await getConnCAs(connectionId)
    if (cas === null || selectedConnectionId.value !== connectionId) return
    giDeactCAs.value = cas
    if (cas.length <= 1) {
      await doGIDeactivation(cas[0] ?? null, connectionId)
    } else {
      giDeactMenuConnectionId.value = connectionId
      giDeactMenuPos.value = anchor
      giDeactMenuOpen.value = !giDeactMenuOpen.value
    }
  } catch (err) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(err))
  }
}

// 发送停止激活(COT=8)总召唤。ca === null 表示对所有 CA 并发取消进行中的 GI。
async function doGIDeactivation(ca: number | null, connectionId: string | null) {
  giDeactMenuOpen.value = false
  giDeactMenuConnectionId.value = null
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

// 点"计量召唤":单 CA 连接直接发;多 CA 连接弹出菜单让用户选具体 CA 或全部。
async function sendCounterRead(e: MouseEvent) {
  // Capture the anchor synchronously — `currentTarget` is nulled after the await.
  const anchor = anchorPos(e.currentTarget as HTMLElement)
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    const cas = await getConnCAs(connectionId)
    if (cas === null || selectedConnectionId.value !== connectionId) return
    ccCAs.value = cas
    if (cas.length <= 1) {
      await doCounterRead(cas[0] ?? null, connectionId)
    } else {
      ccMenuConnectionId.value = connectionId
      ccMenuPos.value = anchor
      ccMenuOpen.value = !ccMenuOpen.value
    }
  } catch (e) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(e))
  }
}

// 发送计量召唤。ca 为具体公共地址;ca === null 表示对所有 CA 并发(菜单"全部 CA")。
async function doCounterRead(ca: number | null, connectionId: string | null) {
  ccMenuOpen.value = false
  ccMenuConnectionId.value = null
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

// 停止激活(COT=8)计数量召唤:单 CA 直发,多 CA 弹菜单选具体 CA 或全部。
async function sendCounterReadDeactivation(e: MouseEvent) {
  const anchor = anchorPos(e.currentTarget as HTMLElement)
  const connectionId = selectedConnectionId.value
  if (!connectionId) return
  try {
    const cas = await getConnCAs(connectionId)
    if (cas === null || selectedConnectionId.value !== connectionId) return
    ccDeactCAs.value = cas
    if (cas.length <= 1) {
      await doCounterReadDeactivation(cas[0] ?? null, connectionId)
    } else {
      ccDeactMenuConnectionId.value = connectionId
      ccDeactMenuPos.value = anchor
      ccDeactMenuOpen.value = !ccDeactMenuOpen.value
    }
  } catch (err) {
    if (selectedConnectionId.value === connectionId) await showAlert(String(err))
  }
}

// 发送停止激活(COT=8)计数量召唤。ca === null 表示对所有 CA 并发取消进行中的累计量扫描。
async function doCounterReadDeactivation(ca: number | null, connectionId: string | null) {
  ccDeactMenuOpen.value = false
  ccDeactMenuConnectionId.value = null
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
  } finally {
    broadcastMenuOpen.value = false
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
  } finally {
    broadcastMenuOpen.value = false
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
  } finally {
    broadcastMenuOpen.value = false
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
  } finally {
    broadcastMenuOpen.value = false
  }
}
</script>

<template>
  <div class="toolbar">
    <div class="toolbar-main">
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="openNewConnection">
        <span class="btn-icon">+</span> {{ t('toolbar.newConnection') }}
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn btn-start" :disabled="!hasConnection() || isConnected()" @click="connectMaster">
        {{ t('toolbar.connect') }}
      </button>
      <button class="toolbar-btn btn-stop" :disabled="!hasConnection() || !isConnected()" @click="disconnectMaster">
        {{ t('toolbar.disconnect') }}
      </button>
      <button
        class="toolbar-btn btn-edit"
        :disabled="!hasConnection()"
        :title="t('toolbar.editConnection')"
        @click="editSelectedConnection"
      >
        {{ t('toolbar.editConnection') }}
      </button>
      <button class="toolbar-btn btn-close" :disabled="!hasConnection()" @click="deleteMaster">
        {{ t('toolbar.delete') }}
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <div class="gi-btn-wrap">
        <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendGI">
          {{ t('toolbar.sendGI') }}<span v-if="connCAs.length > 1" class="gi-caret">&#9662;</span>
        </button>
        <Teleport to="body">
          <ul
            v-if="giMenuOpen"
            class="split-menu floating"
            :style="{ top: giMenuPos.top + 'px', left: giMenuPos.left + 'px' }"
            @click.stop
          >
            <li @click="doGI(null, giMenuConnectionId)">{{ t('toolbar.giAllCAs') }}</li>
            <li v-for="ca in giCAs" :key="ca" @click="doGI(ca, giMenuConnectionId)">CA {{ ca }}</li>
          </ul>
        </Teleport>
      </div>
      <div class="gi-btn-wrap gi-deact-wrap">
        <button
          class="toolbar-btn"
          :disabled="!hasConnection() || !isConnected()"
          :title="t('toolbar.deactivateGI')"
          @click="sendGIDeactivation"
        >
          {{ t('toolbar.deactivateGI') }}<span v-if="connCAs.length > 1" class="gi-caret">&#9662;</span>
        </button>
        <Teleport to="body">
          <ul
            v-if="giDeactMenuOpen"
            class="split-menu floating"
            :style="{ top: giDeactMenuPos.top + 'px', left: giDeactMenuPos.left + 'px' }"
            @click.stop
          >
            <li @click="doGIDeactivation(null, giDeactMenuConnectionId)">{{ t('toolbar.giAllCAs') }}</li>
            <li v-for="ca in giDeactCAs" :key="ca" @click="doGIDeactivation(ca, giDeactMenuConnectionId)">CA {{ ca }}</li>
          </ul>
        </Teleport>
      </div>
      <div class="split-btn" :class="{ disabled: !hasConnection() || !isConnected() }">
        <button
          class="toolbar-btn"
          :disabled="!hasConnection() || !isConnected()"
          :title="`${t('toolbar.broadcastAddressLabel')}: 0x${broadcastAddrLabel}`"
          @click="sendBroadcastGI"
        >
          {{ t('toolbar.broadcast') }}
        </button>
        <button
          class="toolbar-btn split-toggle"
          :disabled="!hasConnection() || !isConnected()"
          @click="toggleBroadcastMenu"
        >&#9662;</button>
        <Teleport to="body">
          <ul
            v-if="broadcastMenuOpen"
            class="split-menu floating"
            :style="{ top: broadcastMenuPos.top + 'px', left: broadcastMenuPos.left + 'px' }"
            @click.stop
          >
            <li @click="sendBroadcastGI">{{ t('toolbar.broadcastGi') }}</li>
            <li @click="sendBroadcastCounterRead">{{ t('toolbar.broadcastCounterRead') }}</li>
            <li @click="sendBroadcastGIDeactivation">{{ t('toolbar.broadcastGiDeactivation') }}</li>
            <li @click="sendBroadcastCounterReadDeactivation">{{ t('toolbar.broadcastCounterReadDeactivation') }}</li>
          </ul>
        </Teleport>
      </div>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendClockSync">
        {{ t('toolbar.clockSync') }}
      </button>
      <div class="gi-btn-wrap cc-btn-wrap">
        <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="sendCounterRead">
          {{ t('toolbar.counterRead') }}<span v-if="connCAs.length > 1" class="gi-caret">&#9662;</span>
        </button>
        <Teleport to="body">
          <ul
            v-if="ccMenuOpen"
            class="split-menu floating"
            :style="{ top: ccMenuPos.top + 'px', left: ccMenuPos.left + 'px' }"
            @click.stop
          >
            <li @click="doCounterRead(null, ccMenuConnectionId)">{{ t('toolbar.giAllCAs') }}</li>
            <li v-for="ca in ccCAs" :key="ca" @click="doCounterRead(ca, ccMenuConnectionId)">CA {{ ca }}</li>
          </ul>
        </Teleport>
      </div>
      <div class="gi-btn-wrap cc-btn-wrap cc-deact-wrap">
        <button
          class="toolbar-btn"
          :disabled="!hasConnection() || !isConnected()"
          :title="t('toolbar.deactivateCounterRead')"
          @click="sendCounterReadDeactivation"
        >
          {{ t('toolbar.deactivateCounterRead') }}<span v-if="connCAs.length > 1" class="gi-caret">&#9662;</span>
        </button>
        <Teleport to="body">
          <ul
            v-if="ccDeactMenuOpen"
            class="split-menu floating"
            :style="{ top: ccDeactMenuPos.top + 'px', left: ccDeactMenuPos.left + 'px' }"
            @click.stop
          >
            <li @click="doCounterReadDeactivation(null, ccDeactMenuConnectionId)">{{ t('toolbar.giAllCAs') }}</li>
            <li v-for="ca in ccDeactCAs" :key="ca" @click="doCounterReadDeactivation(ca, ccDeactMenuConnectionId)">CA {{ ca }}</li>
          </ul>
        </Teleport>
      </div>
      <button class="toolbar-btn" :disabled="!hasConnection() || !isConnected()" @click="openCustomControl">
        {{ t('toolbar.customControl') }}
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn" @click="openParseFrame()">
        {{ t('toolbar.parseFrame') }}
      </button>
    </div>

    <div class="toolbar-divider"></div>

    <div class="toolbar-group">
      <button class="toolbar-btn" @click="saveConfig">
        {{ t('toolbar.saveConfig') }}
      </button>
      <button class="toolbar-btn" data-testid="open-config" @click="openConfig">
        {{ t('toolbar.openConfig') }}
      </button>
    </div>

    </div>
    <div class="toolbar-aside">
      <button class="toolbar-btn" :disabled="updateChecking" @click="manualCheckUpdate">
        {{ updateChecking ? t('toolbar.checkingUpdate') : t('toolbar.checkUpdate') }}
      </button>
      <LangSwitch />
      <VersionBadge />
      <button class="toolbar-title as-button" @click="showAbout = true" :title="t('toolbar.about')">
        {{ t('toolbar.appTitle') }}
      </button>
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
/* Common toolbar chrome (.toolbar, .toolbar-btn, .toolbar-divider, .toolbar-title …)
   lives in @shared/styles/toolbar.css so master and slave stay identical.
   Only master-specific split-button / dropdown styles remain here. */

.gi-btn-wrap { position: relative; display: inline-flex; }
.gi-caret { margin-left: 2px; font-size: 10px; opacity: 0.7; }

.split-btn { position: relative; display: inline-flex; }
.split-btn .split-toggle { padding: 0 6px; min-width: 0; }
.split-menu {
  position: absolute; top: 100%; left: 0; z-index: 50;
  list-style: none; margin: 0; padding: 4px 0;
  background: var(--bg-elevated, var(--c-base, #fff));
  border: 1px solid var(--c-surface0, #ccc);
  border-radius: 4px; box-shadow: 0 4px 12px rgba(0,0,0,0.12);
  min-width: 160px;
}
/* Teleported to <body>; positioned from the trigger's viewport rect. */
.split-menu.floating {
  position: fixed;
}
.split-menu li { padding: 6px 12px; cursor: pointer; white-space: nowrap; font-size: 12px; }
.split-menu li:hover { background: var(--c-surface0, #f0f0f0); }

</style>
