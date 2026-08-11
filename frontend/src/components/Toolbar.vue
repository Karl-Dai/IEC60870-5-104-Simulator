<script setup lang="ts">
import { ref, inject, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save, open } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { dialogKey } from '@shared/composables/useDialog'
import type { showAlert as ShowAlert, showPrompt as ShowPrompt, showConfirm as ShowConfirm } from '@shared/composables/useDialog'
import AboutDialog from '@shared/components/AboutDialog.vue'
import LangSwitch from '@shared/components/LangSwitch.vue'
import VersionBadge from '@shared/components/VersionBadge.vue'
import NewServerModal from './NewServerModal.vue'
import CsvImportModeModal from './CsvImportModeModal.vue'
import { useI18n } from '@shared/i18n'
import { formatStartServerError } from '../errors'

const { t } = useI18n()
const showAbout = ref(false)

const selectedServerId = inject<Ref<string | null>>('selectedServerId')!
const selectedServerState = inject<Ref<string>>('selectedServerState')!
const selectedCA = inject<Ref<number | null>>('selectedCA')!
const refreshTree = inject<() => void>('refreshTree')!
const refreshData = inject<() => void>('refreshData')!
const resetData = inject<() => Promise<void>>('resetData')!
const resetWorkspaceView = inject<() => void>('resetWorkspaceView')!
const { showAlert, showPrompt, showConfirm } = inject<{
  showAlert: typeof ShowAlert
  showPrompt: typeof ShowPrompt
  showConfirm: typeof ShowConfirm
}>(dialogKey)!
const openParseFrame = inject<(prefill?: string) => void>('openParseFrame')!
const openRuntimeParamsDrawer = inject<() => void>('openRuntimeParamsDrawer')!

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

const showNewServerModal = ref(false)
const serverActionPending = ref(false)
const csvActionPending = ref(false)
const csvImportPath = ref<string | null>(null)
const showCsvImportMode = ref(false)

type PointCsvImportResult = {
  imported: number
  total_points: number
  mutations_started: number
}

async function startServer() {
  if (!selectedServerId.value || serverActionPending.value) return
  serverActionPending.value = true
  try {
    await invoke('start_server', { id: selectedServerId.value })
    selectedServerState.value = 'Running'
    refreshTree()
  } catch (e) {
    await showAlert(formatStartServerError(e, t))
  } finally {
    serverActionPending.value = false
  }
}

async function stopServer() {
  if (!selectedServerId.value || serverActionPending.value) return
  serverActionPending.value = true
  try {
    await invoke('stop_server', { id: selectedServerId.value })
    selectedServerState.value = 'Stopped'
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  } finally {
    serverActionPending.value = false
  }
}

async function addStation() {
  if (!selectedServerId.value) return
  const caStr = await showPrompt(t('prompt.inputCommonAddress'), '1')
  if (caStr === null) return
  const ca = Number(caStr)
  if (!Number.isInteger(ca) || ca < 1 || ca > 65534) {
    await showAlert(t('errors.invalidCa'))
    return
  }
  const defaultName = t('station.defaultName', { ca })
  const name = await showPrompt(t('prompt.inputStationName'), defaultName)
  if (name === null) return
  try {
    await invoke('add_station', {
      request: {
        server_id: selectedServerId.value,
        common_address: ca,
        name: name || '',
      },
    })
    refreshTree()
  } catch (e) {
    await showAlert(String(e))
  }
}

async function saveConfig() {
  const path = await save({
    filters: [{ name: 'IEC104 Config', extensions: ['json'] }],
    defaultPath: 'iec104-slave-config.json',
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
    // 配置里的运行参数(如 sync_tb_by_category)也换了一批:必须 bump dataRefreshKey,
    // 否则数据表沿用旧快照,+TB 徽标按老配置显示(issue #28 同类残余)。
    refreshData()
    await showAlert(t('toolbar.configLoaded', { count }))
  } catch (e) {
    await showAlert(`${t('toolbar.configLoadFailed')}: ${e}`)
  }
}

async function chooseCsvImport() {
  if (
    !selectedServerId.value
    || selectedCA.value === null
    || selectedServerState.value !== 'Stopped'
    || csvActionPending.value
  ) return
  const path = await open({
    multiple: false,
    filters: [{ name: 'Point Configuration CSV', extensions: ['csv'] }],
  })
  if (!path || typeof path !== 'string') return
  csvImportPath.value = path
  showCsvImportMode.value = true
}

function cancelCsvImport() {
  showCsvImportMode.value = false
  csvImportPath.value = null
}

async function importCsv(mode: 'merge' | 'replace') {
  const path = csvImportPath.value
  const serverId = selectedServerId.value
  const commonAddress = selectedCA.value
  showCsvImportMode.value = false
  csvImportPath.value = null
  if (!path || !serverId || commonAddress === null || csvActionPending.value) return
  if (selectedServerState.value !== 'Stopped') {
    await showAlert(t('toolbar.csvImportStoppedOnly'))
    return
  }
  if (mode === 'replace' && !await showConfirm(t('toolbar.csvReplaceConfirm'))) return

  csvActionPending.value = true
  try {
    const result = await invoke<PointCsvImportResult>('import_point_config_csv', {
      serverId,
      commonAddress,
      path,
      mode,
    })
    refreshTree()
    await resetData()
    await showAlert(t('toolbar.csvImported', {
      count: result.imported,
      total: result.total_points,
      mutations: result.mutations_started,
    }))
  } catch (error) {
    await showAlert([
      t('toolbar.csvImportFailed'),
      t('toolbar.csvImportErrorHint'),
      String(error),
    ].join('\n'))
  } finally {
    csvActionPending.value = false
  }
}

async function exportCsv() {
  const serverId = selectedServerId.value
  const commonAddress = selectedCA.value
  if (!serverId || commonAddress === null || csvActionPending.value) return
  const path = await save({
    filters: [{ name: 'Point Configuration CSV', extensions: ['csv'] }],
    defaultPath: `iec104-points-ca-${commonAddress}.csv`,
  })
  if (!path) return

  csvActionPending.value = true
  try {
    const count = await invoke<number>('save_point_config_csv', {
      serverId,
      commonAddress,
      path,
    })
    await showAlert(t('toolbar.csvExported', { count }))
  } catch (error) {
    await showAlert(`${t('toolbar.csvExportFailed')}: ${error}`)
  } finally {
    csvActionPending.value = false
  }
}

async function downloadCsvTemplate() {
  const commonAddress = selectedCA.value
  if (!selectedServerId.value || commonAddress === null || csvActionPending.value) return
  const path = await save({
    filters: [{ name: 'Point Configuration CSV', extensions: ['csv'] }],
    defaultPath: `iec104-point-template-ca-${commonAddress}.csv`,
  })
  if (!path) return

  csvActionPending.value = true
  try {
    await invoke('save_point_config_csv_template', { commonAddress, path })
    await showAlert(t('toolbar.csvTemplateSaved'))
  } catch (error) {
    await showAlert(`${t('toolbar.csvTemplateFailed')}: ${error}`)
  } finally {
    csvActionPending.value = false
  }
}

</script>

<template>
  <div class="toolbar">
    <div class="toolbar-main">
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="showNewServerModal = true" :title="t('toolbar.titleNewServer')">
        <span class="btn-icon">+</span>
        <span class="toolbar-label">{{ t('toolbar.newServer') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button
        class="toolbar-btn btn-start"
        @click="startServer"
        :disabled="serverActionPending || !selectedServerId || selectedServerState === 'Running'"
        :title="t('toolbar.titleStartServer')"
      >
        <span class="toolbar-label">{{ t('toolbar.start') }}</span>
      </button>
      <button
        class="toolbar-btn btn-stop"
        @click="stopServer"
        :disabled="serverActionPending || !selectedServerId || selectedServerState === 'Stopped'"
        :title="t('toolbar.titleStopServer')"
      >
        <span class="toolbar-label">{{ t('toolbar.stop') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button
        class="toolbar-btn"
        @click="addStation"
        :disabled="!selectedServerId"
        :title="t('toolbar.titleAddStation')"
      >
        <span class="toolbar-label">{{ t('toolbar.addStation') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="openParseFrame()" :title="t('toolbar.parseFrame')">
        <span class="toolbar-label">{{ t('toolbar.parseFrame') }}</span>
      </button>
      <button
        class="toolbar-btn toolbar-btn-params"
        :disabled="!selectedServerId"
        @click="openRuntimeParamsDrawer()"
        :title="t('runtimeParams.title')"
      >
        <svg class="toolbar-icon-svg" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="8" cy="8" r="2.1"/>
          <path d="M8 1.5v1.6M8 12.9v1.6M3.4 3.4l1.13 1.13M11.47 11.47l1.13 1.13M1.5 8h1.6M12.9 8h1.6M3.4 12.6l1.13-1.13M11.47 4.53l1.13-1.13"/>
        </svg>
        <span class="toolbar-label">{{ t('runtimeParams.title') }}</span>
      </button>
    </div>
    <div class="toolbar-divider"></div>
    <div class="toolbar-group">
      <button class="toolbar-btn" @click="saveConfig" :title="t('toolbar.saveConfig')">
        <span class="toolbar-label">{{ t('toolbar.saveConfig') }}</span>
      </button>
      <button class="toolbar-btn" data-testid="open-config" @click="openConfig" :title="t('toolbar.openConfig')">
        <span class="toolbar-label">{{ t('toolbar.openConfig') }}</span>
      </button>
      <button
        class="toolbar-btn"
        data-testid="import-point-csv"
        :disabled="csvActionPending || !selectedServerId || selectedCA === null || selectedServerState !== 'Stopped'"
        @click="chooseCsvImport"
        :title="selectedServerState === 'Stopped' ? t('toolbar.importCsv') : t('toolbar.csvImportStoppedOnly')"
      >
        <span class="toolbar-label">{{ t('toolbar.importCsv') }}</span>
      </button>
      <button
        class="toolbar-btn"
        data-testid="export-point-csv"
        :disabled="csvActionPending || !selectedServerId || selectedCA === null"
        @click="exportCsv"
        :title="t('toolbar.exportCsv')"
      >
        <span class="toolbar-label">{{ t('toolbar.exportCsv') }}</span>
      </button>
      <button
        class="toolbar-btn"
        data-testid="download-point-csv-template"
        :disabled="csvActionPending || !selectedServerId || selectedCA === null"
        @click="downloadCsvTemplate"
        :title="t('toolbar.downloadCsvTemplate')"
      >
        <span class="toolbar-label">{{ t('toolbar.downloadCsvTemplate') }}</span>
      </button>
    </div>
    </div>
    <div class="toolbar-aside">
      <button class="toolbar-btn" :disabled="updateChecking" @click="manualCheckUpdate">
        {{ updateChecking ? t('toolbar.checkingUpdate') : t('toolbar.checkUpdate') }}
      </button>
      <LangSwitch />
      <VersionBadge />
      <button class="toolbar-title as-button" @click="showAbout = true" :title="t('toolbar.about')">{{ t('toolbar.appTitle') }}</button>
    </div>
  </div>

  <AboutDialog :visible="showAbout" @close="showAbout = false" />
  <NewServerModal v-model:visible="showNewServerModal" />
  <CsvImportModeModal
    :visible="showCsvImportMode"
    @choose="importCsv"
    @cancel="cancelCsvImport"
  />
</template>

<style scoped>
/* Common toolbar chrome (.toolbar, .toolbar-btn, .toolbar-divider, .toolbar-title …)
   lives in @shared/styles/toolbar.css so master and slave stay identical.
   Only the slave-specific runtime-params gear icon remains here. */

.toolbar-icon-svg {
  width: 13px;
  height: 13px;
  flex: none;
  color: var(--c-subtext0);
  transition: color 100ms linear, transform 220ms ease;
}

.toolbar-btn-params:hover:not(:disabled) .toolbar-icon-svg {
  color: var(--c-blue);
  transform: rotate(45deg);
}
</style>
