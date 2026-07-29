import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { useI18n } from '@shared/i18n'
import AboutDialog from '@shared/components/AboutDialog.vue'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: () => Promise.resolve('1.15.4'),
}))

const REPO = 'https://github.com/Karl-Dai/IEC60870-5-104-Simulator'

describe('AboutDialog localization', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockResolvedValue(undefined)
    useI18n().setLocale('en-US')
  })

  it('localizes current release notes and the documentation URL', async () => {
    const wrapper = mount(AboutDialog, {
      props: { visible: true },
      global: { stubs: { Teleport: true, Transition: false } },
    })
    await flushPromises()

    expect(wrapper.find('.about-notes').text()).toContain('v1.15.4')
    expect(wrapper.find('.about-notes').text()).not.toMatch(/[\u3400-\u9fff]/u)
    await wrapper.findAll('.about-links a')[0].trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: `${REPO}/blob/main/README.md`,
    })

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.find('.about-notes').text()).toContain('点位工作流升级')
    await wrapper.findAll('.about-links a')[0].trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: `${REPO}/blob/main/README_CN.md`,
    })
  })
})
