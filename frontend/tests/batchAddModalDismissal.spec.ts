import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import BatchAddModal from '../src/components/BatchAddModal.vue'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

let wrapper: VueWrapper | undefined

function mountModal() {
  wrapper = mount(BatchAddModal, {
    props: {
      visible: true,
      serverId: 'server_1',
      commonAddress: 1,
      existingPoints: [],
    },
    global: {
      stubs: { teleport: true },
      provide: {
        [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
      },
    },
  })
  return wrapper
}

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('BatchAddModal 关闭边界', () => {
  it('点击外部遮罩不关闭弹窗', async () => {
    const modal = mountModal()

    await modal.get('.modal-backdrop').trigger('click')

    expect(modal.emitted('close')).toBeUndefined()
    expect(modal.find('.modal').exists()).toBe(true)
  })

  it('右上角关闭按钮仍发送 close', async () => {
    const modal = mountModal()

    await modal.get('.btn-close').trigger('click')

    expect(modal.emitted('close')).toHaveLength(1)
  })

  it('底部取消按钮仍发送 close', async () => {
    const modal = mountModal()

    await modal.get('.btn-secondary').trigger('click')

    expect(modal.emitted('close')).toHaveLength(1)
  })
})
