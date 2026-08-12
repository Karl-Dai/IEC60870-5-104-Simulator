import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { useI18n } from '@shared/i18n'
import AboutDialog from '@shared/components/AboutDialog.vue'
import { ABOUT_RELEASE_NOTES } from '../src/releaseNotes'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: () => Promise.resolve('1.15.7'),
}))

const REPO = 'https://github.com/Karl-Dai/IEC60870-5-104-Simulator'
const SIMLAB = 'https://simlab.carldai.cloud'

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

    expect(wrapper.find('.about-notes').text()).toContain(ABOUT_RELEASE_NOTES['en-US'][0])
    expect(wrapper.find('.about-notes').text()).not.toMatch(/[\u3400-\u9fff]/u)
    expect(wrapper.get('[data-testid="simlab-link"]').text()).toBe('SimLab Online')
    await wrapper.get('[data-testid="simlab-link"]').trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: SIMLAB,
    })
    await wrapper.get('[data-testid="documentation-link"]').trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: `${REPO}/blob/main/README.md`,
    })

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.find('.about-notes').text()).toContain(ABOUT_RELEASE_NOTES['zh-CN'][0])
    expect(wrapper.get('[data-testid="simlab-link"]').text()).toBe('在线体验 SimLab')
    await wrapper.get('[data-testid="documentation-link"]').trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: `${REPO}/blob/main/README_CN.md`,
    })
  })
})
