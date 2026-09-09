import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { runToolbarAction, type ToolbarActionContext } from './context'

type SchedulePreview = {
  event_count: number
  point_count: number
  duration_ms: number
}

const filters = [{ name: 'Point Event JSON', extensions: ['json'] }]

export function usePointEventActions(context: ToolbarActionContext) {
  const { t, selectedServerId, selectedServerState, selectedCA, showAlert, showConfirm } = context

  async function importEventJson() {
    const serverId = selectedServerId.value
    const commonAddress = selectedCA.value
    if (!serverId || commonAddress === null || selectedServerState.value !== 'Running') return

    await runToolbarAction(context, async () => {
      try {
        const path = await open({ multiple: false, filters })
        if (typeof path !== 'string' || !path) return
        const preview = await invoke<SchedulePreview>('inspect_point_event_schedule', {
          serverId,
          commonAddress,
          path,
        })
        const confirmed = await showConfirm(t('toolbar.eventJsonConfirm', {
          events: preview.event_count,
          points: preview.point_count,
          duration: preview.duration_ms,
        }))
        if (!confirmed) return
        await invoke('start_point_event_schedule', { serverId, commonAddress, path })
      } catch (error) {
        await showAlert(`${t('toolbar.eventJsonImportFailed')}: ${String(error)}`)
      }
    })
  }

  async function saveEventExample() {
    const serverId = selectedServerId.value
    const commonAddress = selectedCA.value
    if (!serverId || commonAddress === null) return

    await runToolbarAction(context, async () => {
      try {
        const path = await save({
          filters,
          defaultPath: `iec104-point-events-ca-${commonAddress}.json`,
        })
        if (!path) return
        const count = await invoke<number>('save_point_event_schedule_example', {
          serverId,
          commonAddress,
          path,
        })
        await showAlert(t('toolbar.eventJsonExampleSaved', { count }))
      } catch (error) {
        await showAlert(`${t('toolbar.eventJsonExampleFailed')}: ${String(error)}`)
      }
    })
  }

  return { importEventJson, saveEventExample }
}
