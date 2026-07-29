import { ref, readonly } from 'vue'
import { useI18n } from '../i18n'
import { localizeLegacyBackendText } from '../i18n/backendText'

const { t, locale } = useI18n()

export type DialogMode = 'alert' | 'confirm' | 'prompt'

export interface DialogState {
  visible: boolean
  mode: DialogMode
  title: string
  message: string
  defaultValue: string
  inputValue: string
}

const state = ref<DialogState>({
  visible: false,
  mode: 'alert',
  title: '',
  message: '',
  defaultValue: '',
  inputValue: '',
})

let resolvePromise: ((value: string | boolean | null) => void) | null = null

function open(mode: DialogMode, message: string, defaultValue = ''): Promise<any> {
  const localizedMessage = localizeLegacyBackendText(
    message,
    locale.value,
    t,
    'appDialog.backendMessageFallback',
  )
  return new Promise((resolve) => {
    resolvePromise = resolve
    state.value = {
      visible: true,
      mode,
      title: mode === 'alert' ? t('appDialog.titleAlert') : mode === 'confirm' ? t('appDialog.titleConfirm') : t('appDialog.titlePrompt'),
      message: localizedMessage,
      defaultValue,
      inputValue: defaultValue,
    }
  })
}

export function showAlert(message: string): Promise<void> {
  return open('alert', message) as Promise<void>
}

export function showConfirm(message: string): Promise<boolean> {
  return open('confirm', message) as Promise<boolean>
}

export function showPrompt(message: string, defaultValue = ''): Promise<string | null> {
  return open('prompt', message, defaultValue) as Promise<string | null>
}

export function dialogConfirm(value?: string) {
  if (!resolvePromise) return
  const mode = state.value.mode
  state.value.visible = false
  if (mode === 'alert') resolvePromise(undefined as any)
  else if (mode === 'confirm') resolvePromise(true)
  else resolvePromise(value ?? state.value.inputValue)
  resolvePromise = null
}

export function dialogCancel() {
  if (!resolvePromise) return
  const mode = state.value.mode
  state.value.visible = false
  if (mode === 'alert') resolvePromise(undefined as any)
  else if (mode === 'confirm') resolvePromise(false)
  else resolvePromise(null)
  resolvePromise = null
}

export function useDialogState() {
  return { state: readonly(state), dialogConfirm, dialogCancel }
}

export const dialogKey = Symbol('dialog') as symbol
