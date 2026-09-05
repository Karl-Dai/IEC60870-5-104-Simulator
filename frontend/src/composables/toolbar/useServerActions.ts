import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ServerInfo } from '../../types'
import { formatStartServerError } from '../../errors'
import { runToolbarAction, type ToolbarActionContext } from './context'

export function useServerActions(context: ToolbarActionContext) {
  const { t, selectedServerId, selectedServerState, refreshTree, showAlert, showPrompt } = context
  const bulkAction = ref<'start' | 'stop' | null>(null)
  const completed = ref(0)
  const total = ref(0)

  async function changeServer(action: 'start' | 'stop') {
    const id = selectedServerId.value
    if (!id) return
    await runToolbarAction(context, async () => {
      try {
        await invoke(action === 'start' ? 'start_server' : 'stop_server', { id })
        if (selectedServerId.value === id) selectedServerState.value = action === 'start' ? 'Running' : 'Stopped'
        refreshTree()
      } catch (error) {
        await showAlert(action === 'start' ? formatStartServerError(error, t) : String(error))
      }
    })
  }

  async function changeAllServers(action: 'start' | 'stop') {
    await runToolbarAction(context, async () => {
      const starting = action === 'start'
      const targetState = starting ? 'Running' : 'Stopped'
      bulkAction.value = action
      completed.value = 0
      total.value = 0
      try {
        const servers = await invoke<ServerInfo[]>('list_servers')
        if (!servers.length) {
          await showAlert(t(starting ? 'toolbar.startAllEmpty' : 'toolbar.stopAllEmpty'))
          return
        }
        const pending = servers.filter(server => server.state !== targetState)
        total.value = pending.length
        let succeeded = 0
        const failures: string[] = []
        // Listener changes are serialized by the backend. Continue after each failure.
        for (const server of pending) {
          try {
            await invoke(starting ? 'start_server' : 'stop_server', { id: server.id })
            succeeded++
            if (selectedServerId.value === server.id) selectedServerState.value = targetState
          } catch (error) {
            failures.push(`${server.bind_address}:${server.port} (${server.id}): ${starting ? formatStartServerError(error, t) : String(error)}`)
          } finally { completed.value++ }
        }
        refreshTree()
        const summary = t(starting ? 'toolbar.startAllResult' : 'toolbar.stopAllResult', {
          started: succeeded, stopped: succeeded, skipped: servers.length - pending.length, failed: failures.length,
        })
        await showAlert([summary, ...failures].join('\n'))
      } catch (error) {
        await showAlert(`${t(starting ? 'toolbar.startAllFailed' : 'toolbar.stopAllFailed')}: ${String(error)}`)
      } finally {
        bulkAction.value = null
      }
    })
  }

  async function addStation() {
    const serverId = selectedServerId.value
    if (!serverId) return
    await runToolbarAction(context, async () => {
      try {
        const input = await showPrompt(t('prompt.inputCommonAddress'), '1')
        if (input === null) return
        const ca = Number(input)
        if (!Number.isInteger(ca) || ca < 1 || ca > 65534) {
          await showAlert(t('errors.invalidCa'))
          return
        }
        const name = await showPrompt(t('prompt.inputStationName'), t('station.defaultName', { ca }))
        if (name === null) return
        await invoke('add_station', { request: { server_id: serverId, common_address: ca, name } })
        refreshTree()
      } catch (error) { await showAlert(String(error)) }
    })
  }

  return { bulkAction, completed, total, changeServer, changeAllServers, addStation }
}
