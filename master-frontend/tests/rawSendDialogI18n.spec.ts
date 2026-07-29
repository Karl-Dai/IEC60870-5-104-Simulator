import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { useI18n } from '@shared/i18n'
import RawSendDialog from '../src/components/RawSendDialog.vue'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

describe('RawSendDialog localization', () => {
  beforeEach(() => {
    useI18n().setLocale('en-US')
  })

  it('does not leak Chinese text in English and updates live with the locale', async () => {
    const wrapper = mount(RawSendDialog, {
      props: { visible: true, connectionId: 'conn-1' },
      global: { stubs: { Teleport: true, Transition: false } },
    })

    expect(wrapper.text()).toContain('Send Raw Frame')
    expect(wrapper.text()).toContain('General Interrogation act')
    expect(wrapper.text()).not.toMatch(/[\u3400-\u9fff]/u)

    await wrapper.find('textarea').setValue('GG')
    expect(wrapper.find('.preview-msg').text()).toBe('Contains invalid characters')

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.text()).toContain('原始报文发送')
    expect(wrapper.text()).toContain('总召唤 act')
    expect(wrapper.find('.preview-msg').text()).toBe('包含非法字符')
  })
})
