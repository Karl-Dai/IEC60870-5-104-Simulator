import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import NewServerModal from '../src/components/NewServerModal.vue'

const invokeMock = vi.fn()
const alertMock = vi.fn(() => Promise.resolve())
const refreshTreeMock = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function mountModal() {
  return mount(NewServerModal, {
    props: { visible: true },
    global: {
      provide: {
        [dialogKey as symbol]: { showAlert: alertMock },
        refreshTree: refreshTreeMock,
      },
      stubs: { Teleport: true },
    },
  })
}

describe('NewServerModal station configuration', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockImplementation((command: string) => {
      if (command === 'list_bind_address_suggestions') return Promise.resolve(['0.0.0.0'])
      if (command === 'create_server') return Promise.resolve({ id: 'server_1' })
      return Promise.resolve(undefined)
    })
    alertMock.mockClear()
    refreshTreeMock.mockClear()
    useI18n().setLocale('en-US')
  })

  it('creates the initial station with the requested CA and name', async () => {
    const wrapper = mountModal()
    const numberInputs = wrapper.findAll('input[type="number"]')
    const textInputs = wrapper.findAll('input[type="text"]')
    await numberInputs[1].setValue('456')
    await textInputs[1].setValue('220TVAA')
    await wrapper.find('.modal-btn.confirm').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('create_server', {
      request: expect.objectContaining({
        common_address: 456,
        station_name: '220TVAA',
      }),
    })
    expect(invokeMock).toHaveBeenCalledWith('start_server', { id: 'server_1' })
    expect(refreshTreeMock).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('rejects a fractional common address before invoking the backend', async () => {
    const wrapper = mountModal()
    await wrapper.findAll('input[type="number"]')[1].setValue('1.5')
    await wrapper.find('.modal-btn.confirm').trigger('click')
    await flushPromises()

    expect(alertMock).toHaveBeenCalledWith('Please enter a valid common address (1-65534)')
    expect(invokeMock.mock.calls.some(([command]) => command === 'create_server')).toBe(false)
    wrapper.unmount()
  })
})
