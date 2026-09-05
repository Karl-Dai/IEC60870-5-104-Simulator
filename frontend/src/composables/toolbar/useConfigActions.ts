import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { runToolbarAction, type ToolbarActionContext } from './context'

const filters = [{ name: 'IEC104 Config', extensions: ['json'] }]
export function useConfigActions(context: ToolbarActionContext) {
  const { t, showAlert, showPrompt, resetWorkspaceView, refreshTree, refreshData } = context
  const loading = ref(false)
  const lastPath = ref('')

  async function loadPath(path: string) {
    loading.value = true
    try {
      const count = await invoke<number>('load_config', { path })
      resetWorkspaceView()
      refreshTree()
      refreshData()
      await showAlert(t('toolbar.configLoaded', { count }))
    } finally { loading.value = false }
  }

  async function openConfig(byPath = false) {
    await runToolbarAction(context, async () => {
      try {
        let path: string
        if (byPath) {
          const input = await showPrompt(t('toolbar.configPathPrompt'), lastPath.value)
          if (input === null) return
          path = input.trim()
          if (path.length >= 2 && ((path.startsWith('"') && path.endsWith('"'))
            || (path.startsWith("'") && path.endsWith("'")))) path = path.slice(1, -1)
          if (!path.trim()) {
            await showAlert(t('toolbar.configPathRequired'))
            return
          }
          lastPath.value = path
        } else {
          const selected = await open({ multiple: false, filters })
          if (typeof selected !== 'string' || !selected) return
          path = selected
        }
        await loadPath(path)
      } catch (error) { await showAlert(`${t('toolbar.configLoadFailed')}: ${error}`) }
    })
  }

  async function saveConfig() {
    await runToolbarAction(context, async () => {
      try {
        const path = await save({ filters, defaultPath: 'iec104-slave-config.json' })
        if (!path) return
        await invoke('save_config', { path })
        await showAlert(t('toolbar.configSaved'))
      } catch (error) { await showAlert(`${t('toolbar.configSaveFailed')}: ${error}`) }
    })
  }
  return { loading, openConfig, saveConfig }
}
