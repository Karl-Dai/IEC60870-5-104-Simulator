import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { useI18n } from '@shared/i18n'
import ServerSettingsModal from '../src/components/ServerSettingsModal.vue'
import type { ServerTransportInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
let wrapper: VueWrapper | null
let current: ServerTransportInfo
const originalTls = { enabled: true, cert_file: '/server.crt', key_file: '/server.key', ca_file: '/ca.crt', require_client_cert: true }

beforeEach(() => {
  current = { id: 's1', bind_address: '127.0.0.1', port: 2404, state: 'Stopped', tls: { ...originalTls } }
  useI18n().setLocale('en-US')
  invokeMock.mockReset()
  invokeMock.mockImplementation((command: string) => {
    if (command === 'get_server_transport') return Promise.resolve(structuredClone(current))
    if (command === 'stop_server') { current.state = 'Stopped'; return Promise.resolve() }
    return Promise.resolve({ id: current.id, state: 'Stopped' })
  })
})
afterEach(() => { wrapper?.unmount(); wrapper = null })
async function open() {
  wrapper = mount(ServerSettingsModal, { props: { visible: true, serverId: 's1' }, global: { stubs: { Teleport: true } } })
  await flushPromises()
  return wrapper
}

describe('server transport and TLS settings', () => {
  it('turns TLS off while retaining certificate paths and never restarts or recreates the server', async () => {
    const modal = await open()
    expect(modal.findAll('.file-path-input').map(i => (i.element as HTMLInputElement).value)).toEqual(['/server.crt', '/server.key', '/ca.crt'])
    await modal.get('.tls-toggle input').setValue(false)
    expect(modal.findAll('.file-path-input')).toHaveLength(0)
    await modal.get('.primary').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('update_server_transport', { request: {
      server_id: 's1', bind_address: '127.0.0.1', port: 2404, tls: { ...originalTls, enabled: false },
    } })
    expect(invokeMock.mock.calls.some(([command]) => ['start_server', 'create_server', 'delete_server'].includes(command))).toBe(false)
    expect(modal.emitted('close')).toHaveLength(1)
    expect(modal.emitted('saved')).toHaveLength(1)
  })

  it('requires an explicit stop before editing a running listener', async () => {
    current.state = 'Running'
    const modal = await open()
    expect(modal.get('fieldset').attributes('disabled')).toBeDefined()
    expect(modal.get('.running-notice').text()).toContain('disconnects current clients')
    await modal.get('.running-notice button').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('stop_server', { id: 's1' })
    expect(modal.get('fieldset').attributes('disabled')).toBeUndefined()
    expect(modal.emitted('stopped')).toEqual([['s1']])
    expect(modal.emitted('close')).toBeUndefined()
  })

  it('keeps edited values after save failure and permits a corrected retry', async () => {
    const modal = await open()
    invokeMock.mockRejectedValueOnce('certificate unavailable')
    await modal.findAll('.file-path-input')[0].setValue('/wrong.crt')
    await modal.get('.primary').trigger('click')
    await flushPromises()
    expect(modal.get('[role="alert"]').text()).toContain('certificate unavailable')
    await modal.get('.settings-backdrop').trigger('mousedown')
    expect((modal.findAll('.file-path-input')[0].element as HTMLInputElement).value).toBe('/wrong.crt')
    expect(modal.emitted('close')).toBeUndefined()
    await modal.get('.tls-toggle input').setValue(false)
    await modal.get('.primary').trigger('click')
    await flushPromises()
    expect(modal.emitted('close')).toHaveLength(1)
  })

  it('blocks saves after a read error and can reload without writing blank defaults', async () => {
    invokeMock.mockRejectedValueOnce('read failed')
    const modal = await open()
    expect(modal.get('.primary').attributes('disabled')).toBeDefined()
    expect(modal.find('fieldset').exists()).toBe(false)
    await modal.get('[role="alert"] button').trigger('click')
    await flushPromises()
    expect(modal.get('fieldset').attributes('disabled')).toBeUndefined()
    expect(invokeMock.mock.calls.some(([command]) => command === 'update_server_transport')).toBe(false)
  })

  it('ignores a late response for the previous server', async () => {
    let resolveOld!: (value: ServerTransportInfo) => void
    invokeMock.mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve }))
    const modal = await open()
    current = { ...current, id: 's2', port: 2500, tls: { ...originalTls, enabled: false } }
    await modal.setProps({ serverId: 's2' })
    await flushPromises()
    resolveOld({ ...current, id: 's1', port: 2404, tls: { ...originalTls } })
    await flushPromises()
    expect((modal.get('input[type="number"]').element as HTMLInputElement).value).toBe('2500')
    expect(modal.findAll('.file-path-input')).toHaveLength(0)
  })

  it('keeps the draft and offers stopping if another action started the server', async () => {
    const modal = await open()
    await modal.get('.tls-toggle input').setValue(false)
    current.state = 'Running'
    invokeMock.mockRejectedValueOnce('stop the server first')
    await modal.get('.primary').trigger('click')
    await flushPromises()
    expect(modal.get('.running-notice button').text()).toBe('Stop and Edit')
    expect(modal.findAll('.file-path-input')).toHaveLength(0)
    expect(modal.get('fieldset').attributes('disabled')).toBeDefined()
    expect(modal.emitted('close')).toBeUndefined()
  })

  it('locks dismissal and repeated saves and does not close a new modal session', async () => {
    const modal = await open()
    await modal.get('.tls-toggle input').setValue(false)
    let finish!: (value: unknown) => void
    invokeMock.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    await modal.get('.primary').trigger('click')
    await modal.get('.primary').trigger('click')
    await modal.get('.settings-backdrop').trigger('mousedown')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(modal.emitted('close')).toBeUndefined()
    expect(invokeMock.mock.calls.filter(([command]) => command === 'update_server_transport')).toHaveLength(1)
    current = { ...current, id: 's2', port: 2500 }
    await modal.setProps({ serverId: 's2' })
    await flushPromises()
    finish({ id: 's1', state: 'Stopped' })
    await flushPromises()
    expect(modal.emitted('close')).toBeUndefined()
    expect(modal.emitted('saved')).toBeUndefined()
    expect((modal.get('input[type="number"]').element as HTMLInputElement).value).toBe('2500')
  })
})
