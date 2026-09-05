import type { Ref } from 'vue'
import type { useI18n } from '@shared/i18n'
import type { showAlert, showPrompt, showConfirm } from '@shared/composables/useDialog'

export interface ToolbarActionContext {
  busy: Ref<boolean>
  t: ReturnType<typeof useI18n>['t']
  selectedServerId: Ref<string | null>
  selectedServerState: Ref<string>
  selectedCA: Ref<number | null>
  refreshTree: () => void
  refreshData: () => void
  resetData: () => Promise<void>
  resetWorkspaceView: () => void
  showAlert: typeof showAlert
  showPrompt: typeof showPrompt
  showConfirm: typeof showConfirm
}

// Acquire before opening a picker or prompt, and release even after cancellation.
export async function runToolbarAction(context: ToolbarActionContext, action: () => Promise<void>) {
  if (context.busy.value) return
  context.busy.value = true
  try { await action() } finally { context.busy.value = false }
}
