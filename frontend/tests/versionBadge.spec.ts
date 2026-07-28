import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { useI18n } from '@shared/i18n'
import VersionBadge from '@shared/components/VersionBadge.vue'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: () => Promise.resolve('1.15.1'),
}))

const REPO = 'https://github.com/Karl-Dai/IEC60870-5-104-Simulator'

describe('VersionBadge GitHub documentation link', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockResolvedValue(undefined)
    useI18n().setLocale('en-US')
  })

  it('opens an explicit README matching the active locale', async () => {
    const wrapper = mount(VersionBadge)
    await flushPromises()
    const github = wrapper.find('.github-link')

    expect(github.attributes('title')).toBe(`${REPO}/blob/main/README.md`)
    await github.trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: `${REPO}/blob/main/README.md`,
    })

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(github.attributes('title')).toBe(`${REPO}/blob/main/README_CN.md`)
    await github.trigger('click')
    expect(invokeMock).toHaveBeenLastCalledWith('plugin:opener|open_url', {
      url: `${REPO}/blob/main/README_CN.md`,
    })
    wrapper.unmount()
  })
})
