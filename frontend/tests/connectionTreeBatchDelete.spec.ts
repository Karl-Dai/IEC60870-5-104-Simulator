import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import ConnectionTree from '../src/components/ConnectionTree.vue'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
const showConfirm = vi.fn()
const showAlert = vi.fn()
let wrapper: ReturnType<typeof mount>
let servers: Array<{ id: string; port: number; state: string; bind_address: string }>
let stations: Record<string, Array<{ common_address: number; name: string; point_count: number }>>
let failId: string | null
const selectedServerId = ref<string | null>('a')
const selectedCA = ref<number | null>(1)
const selectedCategory = ref<string | null>('single_point')

beforeEach(async () => {
  useI18n().setLocale('en-US')
  showConfirm.mockReset().mockResolvedValue(true)
  showAlert.mockReset().mockResolvedValue(undefined)
  selectedServerId.value = 'a'; selectedCA.value = 1; selectedCategory.value = 'single_point'
  servers = ['a', 'b', 'c'].map((id, i) => ({ id, port: 2404 + i, state: i === 0 ? 'Running' : 'Stopped', bind_address: '0.0.0.0' }))
  stations = Object.fromEntries(servers.map(server => [server.id, [1, 2].map(common_address => ({ common_address, name: `Station ${common_address}`, point_count: 0 }))]))
  failId = null
  invoke.mockReset().mockImplementation(async (command: string, args: any) => {
    if (command === 'list_servers') return servers.map(server => ({ ...server }))
    if (command === 'list_stations') return stations[args.serverId].map(station => ({ ...station }))
    if (command === 'delete_server') {
      if (args.id === failId) throw new Error('stop failed')
      servers = servers.filter(server => server.id !== args.id)
    }
    if (command === 'remove_station') stations[args.serverId] = stations[args.serverId].filter(station => station.common_address !== args.commonAddress)
  })
  wrapper = mount(ConnectionTree, { global: { provide: {
    [dialogKey]: { showConfirm, showAlert, showPrompt: vi.fn() },
    treeRefreshKey: ref(0), dataRefreshKey: ref(0), selectedServerId, selectedCA, selectedCategory,
    categoryCounts: ref(new Map()),
  } } })
  await flushPromises()
  await wrapper.get('.tree-header button').trigger('click')
})
afterEach(() => wrapper.unmount())
const mutations = () => invoke.mock.calls.filter(([command]) => command === 'delete_server' || command === 'remove_station')
async function deleteSelected() { await wrapper.get('.batch-delete').trigger('click'); await flushPromises() }

describe('batch deletion', () => {
  it('requires a selection and cancellation leaves all data untouched', async () => {
    expect(wrapper.get('.batch-delete').attributes('disabled')).toBeDefined()
    await wrapper.findAll('.server-node input')[0].setValue(true)
    showConfirm.mockResolvedValueOnce(false)
    await deleteSelected()
    expect(mutations()).toEqual([])
    expect(servers).toHaveLength(3)
    expect(wrapper.get('.batch-delete').text()).toContain('(1)')
  })
  it('deletes all selected servers once, including collapsed children, and clears the current view', async () => {
    await wrapper.findAll('.server-node .node-arrow')[0].trigger('click')
    await wrapper.get('.batch-actions input').setValue(true)
    await deleteSelected()
    expect(showConfirm).toHaveBeenCalledOnce()
    expect(showConfirm.mock.calls[0][0]).toContain('Running servers will stop')
    expect(mutations()).toEqual(['a', 'b', 'c'].map(id => ['delete_server', { id }]))
    expect(servers).toHaveLength(0)
    expect(selectedServerId.value).toBeNull()
    expect(selectedCA.value).toBeNull()
    expect(selectedCategory.value).toBeNull()
    expect(wrapper.emitted('selection-cleared')).toHaveLength(1)
  })
  it('deduplicates children of selected parents while allowing stations from other servers', async () => {
    await wrapper.findAll('.station-node input')[0].setValue(true)
    await wrapper.findAll('.server-node input')[0].setValue(true)
    await wrapper.findAll('.station-node input')[2].setValue(true)
    expect(wrapper.findAll('.station-node input')[0].attributes('disabled')).toBeDefined()
    await deleteSelected()
    expect(mutations()).toEqual([['delete_server', { id: 'a' }], ['remove_station', { serverId: 'b', commonAddress: 1 }]])
    expect(stations.b.map(s => s.common_address)).toEqual([2])
    expect(servers.map(s => s.id)).toEqual(['b', 'c'])
  })
  it('deletes multiple stations while retaining the server and unselected station', async () => {
    await wrapper.findAll('.station-node input')[0].setValue(true)
    await wrapper.findAll('.station-node input')[4].setValue(true)
    await deleteSelected()
    expect(mutations()).toEqual([['remove_station', { serverId: 'a', commonAddress: 1 }], ['remove_station', { serverId: 'c', commonAddress: 1 }]])
    expect(servers).toHaveLength(3)
    expect(stations.a.map(s => s.common_address)).toEqual([2])
  })
  it('continues after errors and retains only failed selections for retry', async () => {
    failId = 'b'
    await wrapper.get('.batch-actions input').setValue(true)
    await deleteSelected()
    expect(mutations()).toHaveLength(3)
    expect(servers.map(s => s.id)).toEqual(['b'])
    expect(showAlert.mock.calls[0][0]).toContain('Deleted 2 items; 1 failed.')
    expect(showAlert.mock.calls[0][0]).toContain('0.0.0.0:2405: Error: stop failed')
    expect(wrapper.get('.batch-delete').text()).toContain('(1)')
    failId = null
    await deleteSelected()
    expect(servers).toHaveLength(0)
    expect(wrapper.find('.batch-actions').exists()).toBe(false)
  })
  it('locks selection and suppresses duplicate submission while confirmation is pending', async () => {
    let resolve!: (answer: boolean) => void
    showConfirm.mockImplementationOnce(() => new Promise<boolean>(r => { resolve = r }))
    await wrapper.findAll('.server-node input')[0].setValue(true)
    await wrapper.get('.batch-delete').trigger('click')
    expect(wrapper.get('.batch-delete').attributes('disabled')).toBeDefined()
    await wrapper.findAll('.server-node')[1].trigger('click')
    await wrapper.findAll('.server-node')[1].trigger('contextmenu')
    expect(wrapper.find('.context-menu').exists()).toBe(false)
    resolve(true)
    await flushPromises()
    expect(mutations()).toEqual([['delete_server', { id: 'a' }]])
  })
  it('shows a mixed select-all state for a partial selection', async () => {
    await wrapper.findAll('.station-node input')[0].setValue(true)
    expect((wrapper.get('.batch-actions input').element as HTMLInputElement).indeterminate).toBe(true)
    await wrapper.get('.batch-actions input').setValue(true)
    expect((wrapper.get('.batch-actions input').element as HTMLInputElement).indeterminate).toBe(false)
    expect(wrapper.get('.batch-delete').text()).toContain('(3)')
  })
  it('does not delete the previous workspace after its confirmation resolves', async () => {
    let resolve!: (answer: boolean) => void
    showConfirm.mockImplementationOnce(() => new Promise<boolean>(r => { resolve = r }))
    await wrapper.get('.batch-actions input').setValue(true)
    await wrapper.get('.batch-delete').trigger('click')
    wrapper.unmount()
    resolve(true)
    await flushPromises()
    expect(mutations()).toEqual([])
    expect(showAlert).not.toHaveBeenCalled()
  })
  it('shows progress and stops the remaining deletions when the workspace is replaced', async () => {
    let resolve!: () => void
    const original = invoke.getMockImplementation()!
    invoke.mockImplementation((command: string, args: any) => command === 'delete_server'
      ? new Promise<void>(r => { resolve = r }) : original(command, args))
    await wrapper.get('.batch-actions input').setValue(true)
    await deleteSelected()
    expect(wrapper.get('.batch-delete').text()).toBe('Deleting (0/3)')
    expect(wrapper.get('.batch-delete').attributes('aria-busy')).toBe('true')
    wrapper.unmount()
    resolve()
    await flushPromises()
    expect(mutations()).toEqual([['delete_server', { id: 'a' }]])
    expect(selectedServerId.value).toBe('a')
    expect(showAlert).not.toHaveBeenCalled()
  })
})
