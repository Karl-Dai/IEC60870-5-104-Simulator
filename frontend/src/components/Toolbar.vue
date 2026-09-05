<script setup lang="ts">
import { computed, inject, ref, type Ref } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert, showPrompt as ShowPrompt, showConfirm as ShowConfirm } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import AboutDialog from '@shared/components/AboutDialog.vue'
import LangSwitch from '@shared/components/LangSwitch.vue'
import VersionBadge from '@shared/components/VersionBadge.vue'
import NewServerModal from './NewServerModal.vue'
import CsvImportModeModal from './CsvImportModeModal.vue'
import ToolbarMenu from './ToolbarMenu.vue'
import { useServerActions } from '../composables/toolbar/useServerActions'
import { useConfigActions } from '../composables/toolbar/useConfigActions'
import { usePointCsvActions } from '../composables/toolbar/usePointCsvActions'

const { t } = useI18n()
const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedServerState = inject<Ref<string>>('selectedServerState')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const refreshTree = inject<() => void>('refreshTree')!
const refreshData = inject<() => void>('refreshData')!
const resetData = inject<() => Promise<void>>('resetData')!
const resetWorkspaceView = inject<() => void>('resetWorkspaceView', () => {})
const { showAlert, showPrompt, showConfirm } = inject<{
  showAlert: typeof ShowAlert; showPrompt: typeof ShowPrompt; showConfirm: typeof ShowConfirm
}>(dialogKey)!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!
const openRuntimeParamsDrawer = inject<() => void>('openRuntimeParamsDrawer')!
const openServerSettings = inject<() => void>('openServerSettings', () => {})
const busy = ref(false)
const showNewServerModal = ref(false)
const showAbout = ref(false)
const locked = computed(() => busy.value || showNewServerModal.value)
const context = { busy, t, selectedServerId, selectedServerState, selectedCA,
  refreshTree, refreshData, resetData, resetWorkspaceView, showAlert, showPrompt, showConfirm }
const { bulkAction, completed, total, changeServer, changeAllServers, addStation } = useServerActions(context)
const { loading: configLoading, openConfig, saveConfig } = useConfigActions(context)
const { showImportMode, chooseMode, importCsv, exportCsv } = usePointCsvActions(context)
const openMenu = ref<string | null>(null)
const toggleMenu = (id: string) => { openMenu.value = openMenu.value === id ? null : id }
const closeMenu = () => { openMenu.value = null }
type UpdateMeta = { version: string; notes: string; pub_date?: string | null }
const checkUpdate = inject<(force?: boolean) => Promise<UpdateMeta | null>>('checkUpdate')!
const updateChecking = ref(false)
const MIRROR_RELEASE_URL = 'https://ghfast.top/https://github.com/Karl-Dai/IEC60870-5-104-Simulator/releases/latest'

async function manualCheckUpdate() {
  if (busy.value || updateChecking.value) return
  busy.value = true
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
    busy.value = false
  }
}


const menus = computed(() => [
  { id: 'config', label: t('toolbar.menuConfig'), items: [
    { id: 'open-config', label: t('toolbar.openConfig'), action: () => openConfig() },
    { id: 'open-config-by-path', label: t('toolbar.openConfigByPath'), title: t('toolbar.openConfigByPathTitle'), busy: configLoading.value, action: () => openConfig(true) },
    { id: 'save-config', label: t('toolbar.saveConfig'), action: saveConfig },
  ] },
  { id: 'new', label: t('toolbar.menuNew'), items: [
    { id: 'new-server', label: t('toolbar.newServer'), action: () => { showNewServerModal.value = true } },
    { id: 'add-station', label: t('toolbar.addStation'), disabled: !selectedServerId.value, action: addStation },
  ] },
])
const secondaryMenus = computed(() => [
  { id: 'points', label: t('toolbar.menuPoints'), items: [
    { id: 'import-point-csv', label: t('toolbar.importCsv'), disabled: !selectedServerId.value || selectedCA.value === null || selectedServerState.value !== 'Stopped',
      title: selectedServerState.value === 'Stopped' ? t('toolbar.importCsv') : t('toolbar.csvImportStoppedOnly'), action: importCsv },
    { id: 'export-point-csv', label: t('toolbar.exportCsv'), disabled: !selectedServerId.value || selectedCA.value === null, action: () => exportCsv() },
    { id: 'download-point-csv-template', label: t('toolbar.downloadCsvTemplate'), disabled: !selectedServerId.value || selectedCA.value === null, action: () => exportCsv(true) },
  ] },
  { id: 'settings', label: t('toolbar.menuSettings'), items: [
    { id: 'server-settings', label: t('serverSettings.title'), disabled: !selectedServerId.value, action: openServerSettings },
    { id: 'runtime-params', label: t('runtimeParams.title'), disabled: !selectedServerId.value, action: openRuntimeParamsDrawer },
  ] },
  { id: 'tools', label: t('toolbar.menuTools'), items: [
    { id: 'parse-frame', label: t('toolbar.parseFrame'), action: () => openParseFrame() },
  ] },
])
const helpItems = computed(() => [
  { id: 'check-update', label: updateChecking.value ? t('toolbar.checkingUpdate') : t('toolbar.checkUpdate'), action: manualCheckUpdate },
  { id: 'about', label: t('toolbar.about'), action: () => { showAbout.value = true } },
])
</script>

<template>
  <div class="toolbar slave-toolbar">
    <div class="toolbar-main">
      <ToolbarMenu v-for="menu in menus" :key="menu.id" v-bind="menu" :open="openMenu === menu.id"
        :disabled="locked" @toggle="toggleMenu(menu.id)" @close="closeMenu" />
      <div class="toolbar-divider" aria-hidden="true"></div>
      <div class="toolbar-group" role="group" :aria-label="t('toolbar.currentServer')">
        <span class="scope-label">{{ t('toolbar.currentScope') }}</span>
        <button class="toolbar-btn btn-start" data-testid="start-server" @click="changeServer('start')"
          :disabled="locked || !selectedServerId || selectedServerState === 'Running'" :title="t('toolbar.titleStartServer')">
          {{ t('toolbar.start') }}
        </button>
        <button class="toolbar-btn btn-stop" data-testid="stop-server" @click="changeServer('stop')"
          :disabled="locked || !selectedServerId || selectedServerState === 'Stopped'" :title="t('toolbar.titleStopServer')">
          {{ t('toolbar.stop') }}
        </button>
      </div>
      <div class="toolbar-divider" aria-hidden="true"></div>
      <div class="toolbar-group" role="group" :aria-label="t('toolbar.allServers')">
        <button class="toolbar-btn btn-start" data-testid="start-all-servers" @click="changeAllServers('start')"
          :disabled="locked" :aria-busy="bulkAction === 'start'" :title="t('toolbar.titleStartAll')">
          <span role="status">{{ bulkAction === 'start'
            ? (total ? t('toolbar.startAllProgress', { completed, total }) : t('common.loading')) : t('toolbar.startAll') }}</span>
        </button>
        <button class="toolbar-btn btn-stop" data-testid="stop-all-servers" @click="changeAllServers('stop')"
          :disabled="locked" :aria-busy="bulkAction === 'stop'" :title="t('toolbar.titleStopAll')">
          <span role="status">{{ bulkAction === 'stop'
            ? (total ? t('toolbar.stopAllProgress', { completed, total }) : t('common.loading')) : t('toolbar.stopAll') }}</span>
        </button>
      </div>
      <div class="toolbar-divider" aria-hidden="true"></div>
      <ToolbarMenu v-for="menu in secondaryMenus" :key="menu.id" v-bind="menu" :open="openMenu === menu.id"
        :disabled="locked" @toggle="toggleMenu(menu.id)" @close="closeMenu" />
    </div>
    <div class="toolbar-aside">
      <ToolbarMenu id="help" :label="t('toolbar.menuHelp')" :items="helpItems" :open="openMenu === 'help'"
        :disabled="locked" @toggle="toggleMenu('help')" @close="closeMenu" />
      <LangSwitch />
      <VersionBadge />
    </div>
  </div>
  <AboutDialog :visible="showAbout" @close="showAbout = false" />
  <NewServerModal v-model:visible="showNewServerModal" />
  <CsvImportModeModal :visible="showImportMode" @choose="chooseMode" @cancel="chooseMode(null)" />
</template>

<style scoped>
.slave-toolbar :deep(.toolbar-btn) { padding: 5px 7px; }
.slave-toolbar .toolbar-btn:focus-visible { outline: 2px solid var(--c-blue); outline-offset: -2px; }
.scope-label { align-self: center; margin-right: 2px; color: var(--c-subtext0); font-size: 11px; white-space: nowrap; }
@media (max-width: 1050px) {
  .scope-label { display: none; }
  .slave-toolbar :deep(.toolbar-btn) { padding-inline: 5px; }
  .slave-toolbar .toolbar-divider { margin-inline: 3px; }
}
</style>
