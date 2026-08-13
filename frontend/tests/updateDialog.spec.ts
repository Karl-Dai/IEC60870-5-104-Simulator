import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { useI18n } from '@shared/i18n'
import UpdateDialog from '@shared/components/UpdateDialog.vue'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function mountDialog() {
  return mount(UpdateDialog, {
    props: {
      visible: true,
      version: '2.0.0',
      notes: '### Changed\n- Background download',
    },
    global: { stubs: { Teleport: true, Transition: false } },
  })
}

describe('UpdateDialog ready actions', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockResolvedValue(undefined)
    useI18n().setLocale('zh-CN')
  })

  it('only appears after download and offers all three choices', () => {
    const wrapper = mountDialog()

    expect(wrapper.get('.upd-ready').text()).toContain('后台下载完成')
    expect(wrapper.findAll('button').map((button) => button.text())).toEqual([
      '跳过此版本',
      '下次启动自动更新',
      '立即更新',
    ])
  })

  it('maps each choice to its dedicated backend command', async () => {
    let wrapper = mountDialog()
    await wrapper.findAll('button')[0].trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('skip_update', { version: '2.0.0' })
    expect(wrapper.emitted('close')).toHaveLength(1)

    wrapper.unmount()
    wrapper = mountDialog()
    await wrapper.findAll('button')[1].trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('schedule_update_on_next_launch', {
      version: '2.0.0',
    })
    expect(wrapper.emitted('close')).toHaveLength(1)

    wrapper.unmount()
    wrapper = mountDialog()
    await wrapper.findAll('button')[2].trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('install_update', undefined)
    expect(wrapper.emitted('close')).toBeUndefined()
  })
})
