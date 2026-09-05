import { onBeforeUnmount, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { runToolbarAction, type ToolbarActionContext } from './context'

type ImportMode = 'merge' | 'replace'
type ImportResult = { imported: number; total_points: number; mutations_started: number }
const filters = [{ name: 'Point Configuration CSV', extensions: ['csv'] }]

export function usePointCsvActions(context: ToolbarActionContext) {
  const { t, selectedServerId, selectedServerState, selectedCA, showAlert, showConfirm, refreshTree, resetData } = context
  const showImportMode = ref(false)
  let resolveMode: ((mode: ImportMode | null) => void) | null = null
  function chooseMode(mode: ImportMode | null) {
    showImportMode.value = false
    resolveMode?.(mode)
    resolveMode = null
  }
  onBeforeUnmount(() => chooseMode(null))

  async function importCsv() {
    const serverId = selectedServerId.value
    const commonAddress = selectedCA.value
    if (!serverId || commonAddress === null || selectedServerState.value !== 'Stopped') return
    await runToolbarAction(context, async () => {
      try {
        const path = await open({ multiple: false, filters })
        if (typeof path !== 'string' || !path) return
        const mode = await new Promise<ImportMode | null>(resolve => {
          resolveMode = resolve
          showImportMode.value = true
        })
        if (!mode) return
        if (mode === 'replace' && !await showConfirm(t('toolbar.csvReplaceConfirm'))) return
        // The original station stays the target even if selection changes while a dialog is open.
        // The backend rechecks that its server is stopped before importing.
        const result = await invoke<ImportResult>('import_point_config_csv', { serverId, commonAddress, path, mode })
        refreshTree()
        await resetData()
        await showAlert(t('toolbar.csvImported', {
          count: result.imported, total: result.total_points, mutations: result.mutations_started,
        }))
      } catch (error) {
        await showAlert([t('toolbar.csvImportFailed'), t('toolbar.csvImportErrorHint'), String(error)].join('\n'))
      }
    })
  }

  async function exportCsv(template = false) {
    const serverId = selectedServerId.value
    const commonAddress = selectedCA.value
    if (!serverId || commonAddress === null) return
    await runToolbarAction(context, async () => {
      try {
        const path = await save({ filters, defaultPath: template
          ? `iec104-point-template-ca-${commonAddress}.csv` : `iec104-points-ca-${commonAddress}.csv` })
        if (!path) return
        if (template) {
          await invoke('save_point_config_csv_template', { commonAddress, path })
          await showAlert(t('toolbar.csvTemplateSaved'))
        } else {
          const count = await invoke<number>('save_point_config_csv', { serverId, commonAddress, path })
          await showAlert(t('toolbar.csvExported', { count }))
        }
      } catch (error) {
        await showAlert(`${t(template ? 'toolbar.csvTemplateFailed' : 'toolbar.csvExportFailed')}: ${error}`)
      }
    })
  }
  return { showImportMode, chooseMode, importCsv, exportCsv }
}
