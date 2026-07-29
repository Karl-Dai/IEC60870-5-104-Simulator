import { afterEach, describe, expect, it } from 'vitest'
import { useI18n } from '@shared/i18n'
import { containsCjk, localizeLegacyBackendText } from '@shared/i18n/backendText'
import {
  dialogConfirm,
  showAlert,
  useDialogState,
} from '@shared/composables/useDialog'

afterEach(() => {
  dialogConfirm()
  useI18n().setLocale('en-US')
})

describe('master backend message localization', () => {
  it('localizes a structured connection event in both languages', () => {
    const { t, setLocale } = useI18n()
    setLocale('en-US')
    const english = t('log.masterConnected', {
      address: '127.0.0.1:2404',
      transport: 'TLS',
    })
    expect(english).toBe('Connected to 127.0.0.1:2404 (TLS)')
    expect(containsCjk(english)).toBe(false)
    expect(t('parseFrame.cotNames.10')).toBe('Activation termination')

    setLocale('zh-CN')
    expect(t('log.masterConnected', {
      address: '127.0.0.1:2404',
      transport: 'TLS',
    })).toContain('已连接到')
  })

  it('keeps technical fields while removing legacy CJK detail in English', () => {
    const { t } = useI18n()
    const raw = 't1 超时: TESTFR ACT 后对端在 t1 内仍无任何响应,连接关闭 CA=2'
    const english = localizeLegacyBackendText(raw, 'en-US', t, 'log.backendDetailFallback')
    expect(containsCjk(english)).toBe(false)
    expect(english).toContain('t1')
    expect(english).toContain('TESTFR ACT')
    expect(english).toContain('CA=2')
  })

  it('guards a representative command error in the shared dialog', () => {
    useI18n().setLocale('en-US')
    void showAlert('选择确认超时 (5s) IOA=10 CA=1')
    const message = useDialogState().state.value.message
    expect(containsCjk(message)).toBe(false)
    expect(message).toContain('5s')
    expect(message).toContain('IOA=10 CA=1')
  })
})
