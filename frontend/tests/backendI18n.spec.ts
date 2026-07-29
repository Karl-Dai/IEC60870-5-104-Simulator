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

describe('slave backend message localization', () => {
  it('localizes structured command/server log events in English and Chinese', () => {
    const { t, setLocale } = useI18n()
    setLocale('en-US')
    const english = t('log.cmdRejected', {
      type: 'C_SC_NA_1',
      reason: 'unexpected_cot',
      cot: 8,
      ioa: 100,
      ca: 1,
    })
    expect(english).toContain('C_SC_NA_1 rejected')
    expect(containsCjk(english)).toBe(false)
    expect(t('log.serverStarted', { address: '0.0.0.0:2404', transport: 'TCP' }))
      .toBe('Server started: 0.0.0.0:2404 (TCP)')
    expect(t('parseFrame.cotNames.7')).toBe('Activation confirmation')

    setLocale('zh-CN')
    expect(t('log.cmdRejected', {
      type: 'C_SC_NA_1',
      reason: 'unexpected_cot',
      cot: 8,
      ioa: 100,
      ca: 1,
    })).toContain('已拒绝')
  })

  it('guards legacy log details in English but preserves the Chinese original', () => {
    const { t } = useI18n()
    const raw = '读取错误,连接断开: os error 10054 CA=1'
    const english = localizeLegacyBackendText(raw, 'en-US', t, 'log.backendDetailFallback')
    expect(containsCjk(english)).toBe(false)
    expect(english).toContain('10054 CA=1')
    expect(localizeLegacyBackendText(raw, 'zh-CN', t, 'log.backendDetailFallback'))
      .toBe(raw)
  })

  it('guards a representative backend error before the shared dialog renders it', () => {
    useI18n().setLocale('en-US')
    void showAlert('读取文件失败: C:\\Temp\\station.json (os error 2)')
    const message = useDialogState().state.value.message
    expect(containsCjk(message)).toBe(false)
    expect(message).toContain('C:\\Temp\\station.json')
  })
})
