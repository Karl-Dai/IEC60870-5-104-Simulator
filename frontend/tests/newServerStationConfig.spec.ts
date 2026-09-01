import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import NewServerModal from '../src/components/NewServerModal.vue'

const invokeMock = vi.fn()
const openMock = vi.fn()
const alertMock = vi.fn(() => Promise.resolve())
const refreshTreeMock = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => openMock(...args),
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
      if (command === 'create_and_start_server') return Promise.resolve({ id: 'server_1' })
      return Promise.resolve(undefined)
    })
    alertMock.mockClear()
    openMock.mockReset()
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

    expect(invokeMock).toHaveBeenCalledWith('create_and_start_server', {
      request: expect.objectContaining({
        common_address: 456,
        station_name: '220TVAA',
      }),
    })
    expect(invokeMock.mock.calls.some(([command]) => command === 'start_server')).toBe(false)
    expect(refreshTreeMock).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('rejects a fractional common address before invoking the backend', async () => {
    const wrapper = mountModal()
    await wrapper.findAll('input[type="number"]')[1].setValue('1.5')
    await wrapper.find('.modal-btn.confirm').trigger('click')
    await flushPromises()

    expect(alertMock).toHaveBeenCalledWith('Please enter a valid common address (1-65534)')
    expect(invokeMock.mock.calls.some(([command]) => command === 'create_and_start_server')).toBe(false)
    wrapper.unmount()
  })

  it('selects TLS certificate and key files and submits their paths', async () => {
    openMock
      .mockResolvedValueOnce('/tmp/server.crt')
      .mockResolvedValueOnce('/tmp/server.key')
      .mockResolvedValueOnce('/tmp/ca.pem')

    const wrapper = mountModal()
    const tlsToggle = wrapper.findAll('label').find((label) => label.text().includes('Enable TLS'))!
    await tlsToggle.find('input').setValue(true)

    const browseButtons = wrapper.findAll('.file-path-button')
    expect(browseButtons).toHaveLength(3)
    for (const button of browseButtons) {
      await button.trigger('click')
      await flushPromises()
    }

    const pathInputs = wrapper.findAll('.file-path-input')
    expect(pathInputs.map((input) => (input.element as HTMLInputElement).value)).toEqual([
      '/tmp/server.crt',
      '/tmp/server.key',
      '/tmp/ca.pem',
    ])
    expect(openMock).toHaveBeenNthCalledWith(1, expect.objectContaining({
      filters: [{ name: 'Certificate files', extensions: ['crt', 'cer', 'pem'] }],
    }))
    expect(openMock).toHaveBeenNthCalledWith(2, expect.objectContaining({
      filters: [{ name: 'Private key files', extensions: ['key', 'pem'] }],
    }))

    await wrapper.find('.modal-btn.confirm').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('create_and_start_server', {
      request: expect.objectContaining({
        cert_file: '/tmp/server.crt',
        key_file: '/tmp/server.key',
        ca_file: '/tmp/ca.pem',
      }),
    })
    wrapper.unmount()
  })

  it('keeps every field after startup failure and closes only after a successful retry', async () => {
    let attempts = 0
    invokeMock.mockImplementation((command: string) => {
      if (command === 'create_and_start_server') {
        if (++attempts === 1) return Promise.reject({ code: 'start_failed', message: 'bad certificate' })
        return Promise.resolve({ id: 'server_1' })
      }
      return Promise.resolve(['127.0.0.1'])
    })
    const wrapper = mountModal()
    await wrapper.findAll('input[type="text"]')[0].setValue('127.0.0.1')
    await wrapper.findAll('input[type="text"]')[1].setValue('Saved station')
    const numbers = wrapper.findAll('input[type="number"]')
    await numbers[0].setValue(12404)
    await numbers[1].setValue(456)
    await numbers[2].setValue(12)
    await wrapper.find('input[type="radio"][value="random"]').setValue()
    await wrapper.find('input[type="checkbox"]').setValue(true)
    for (const [index, input] of wrapper.findAll('.file-path-input').entries()) {
      await input.setValue(['/certs/server.crt', '/certs/server.key', '/certs/ca.crt'][index])
    }
    await wrapper.findAll('input[type="checkbox"]')[1].setValue(true)
    await wrapper.find('.confirm').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('update:visible')).toBeUndefined()
    expect(wrapper.get('[role="alert"]').text()).toContain('bad certificate')
    await wrapper.get('.modal-overlay').trigger('mousedown')
    expect(wrapper.emitted('update:visible')).toBeUndefined()
    expect(refreshTreeMock).not.toHaveBeenCalled()
    expect(wrapper.findAll('input[type="number"]').map(i => (i.element as HTMLInputElement).value)).toEqual(['12404', '456', '12'])
    expect(wrapper.findAll('.file-path-input').map(i => (i.element as HTMLInputElement).value)).toEqual(['/certs/server.crt', '/certs/server.key', '/certs/ca.crt'])
    await wrapper.find('.confirm').trigger('click')
    await flushPromises()
    const requests = invokeMock.mock.calls.filter(([command]) => command === 'create_and_start_server')
    expect(requests).toHaveLength(2)
    expect(requests[0][1]).toEqual(requests[1][1])
    expect(wrapper.emitted('update:visible')).toEqual([[false]])
    expect(refreshTreeMock).toHaveBeenCalledOnce()
    wrapper.unmount()
  })

  it('prevents duplicate creation and accidental dismissal while pending', async () => {
    let finish!: (value: unknown) => void
    invokeMock.mockImplementation(() => new Promise(resolve => { finish = resolve }))
    const wrapper = mountModal()
    await wrapper.find('.confirm').trigger('click')
    await wrapper.find('.confirm').trigger('click')
    await wrapper.find('input[type="text"]').trigger('keyup.enter')
    await wrapper.find('.modal-overlay').trigger('mousedown')
    expect(wrapper.get('fieldset').attributes('disabled')).toBeDefined()
    expect(wrapper.emitted('update:visible')).toBeUndefined()
    expect(invokeMock.mock.calls.filter(([command]) => command === 'create_and_start_server')).toHaveLength(1)
    finish({ id: 'server_1' })
    await flushPromises()
    expect(wrapper.emitted('update:visible')).toEqual([[false]])
    wrapper.unmount()
  })
})
