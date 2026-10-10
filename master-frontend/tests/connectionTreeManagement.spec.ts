import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { ref } from 'vue'
import ConnectionTree from '../src/components/ConnectionTree.vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import type { ConnectionInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}

function connection(id: string, cas = [1]): ConnectionInfo {
  return { id, target_address: `192.0.2.${id}`, port: 2404, state: 'Connected', common_addresses: cas } as ConnectionInfo
}

describe('ConnectionTree management', () => {
  let wrapper: VueWrapper | null
  let rows: ConnectionInfo[]
  const treeRefreshKey = ref(0)
  const confirm = vi.fn()
  const alert = vi.fn()

  beforeEach(() => {
    useI18n().setLocale('zh-CN')
    rows = [connection('1', [1, 251]), connection('2'), connection('3')]
    confirm.mockReset().mockResolvedValue(true)
    alert.mockReset().mockResolvedValue(undefined)
    treeRefreshKey.value = 0
    invokeMock.mockReset().mockImplementation(async (command, args) => {
      if (command === 'list_connections') return structuredClone(rows)
      if (command === 'delete_connection') rows = rows.filter(row => row.id !== args.id)
    })
  })

  afterEach(() => {
    wrapper?.unmount()
    wrapper = null
  })

  async function render() {
    wrapper = mount(ConnectionTree, { global: { provide: {
      treeRefreshKey,
      refreshTree: () => treeRefreshKey.value++,
      categoryCounts: ref(new Map()),
      changedCategories: ref(new Map()),
      [dialogKey]: { showConfirm: confirm, showAlert: alert },
    } } })
    await flushPromises()
    return wrapper
  }

  function button(label: string) {
    const result = wrapper!.findAll('button').find(btn => btn.text() === label)
    expect(result, `button ${label}`).toBeDefined()
    return result!
  }

  function deletes() {
    return invokeMock.mock.calls.filter(([command]) => command === 'delete_connection')
  }

  it('collapses every level, keeps it collapsed on refresh, and expands every CA again', async () => {
    await render()
    expect(wrapper!.findAll('.tree-child')).toHaveLength(32)
    await button('全部收起').trigger('click')
    expect(wrapper!.findAll('.tree-children')).toHaveLength(0)
    expect(wrapper!.findAll('.node-expand').every(node => node.attributes('aria-expanded') === 'false')).toBe(true)
    treeRefreshKey.value++
    await flushPromises()
    expect(wrapper!.findAll('.tree-children')).toHaveLength(0)
    await wrapper!.find('.node-expand').trigger('click')
    expect(wrapper!.findAll('.ca-node')).toHaveLength(2)
    expect(wrapper!.findAll('.ca-children')).toHaveLength(0)
    await button('全部展开').trigger('click')
    expect(wrapper!.findAll('.tree-child')).toHaveLength(32)
    expect(deletes()).toEqual([])
    expect(wrapper!.emitted('connection-select')).toBeUndefined()
  })

  it('supports individual checks, indeterminate select-all, deselect-all, and exiting without changing the active view', async () => {
    await render()
    await button('批量管理').trigger('click')
    expect(wrapper!.findAll('.tree-child')).toHaveLength(0)
    await wrapper!.findAll('.tree-node-group input')[0].setValue(true)
    expect((wrapper!.find('.batch-actions input').element as HTMLInputElement).indeterminate).toBe(true)
    expect(button('删除已选 (1)').attributes('disabled')).toBeUndefined()
    await wrapper!.find('.batch-actions input').setValue(true)
    expect(wrapper!.findAll('.tree-node-group input').every(input => (input.element as HTMLInputElement).checked)).toBe(true)
    await wrapper!.find('.batch-actions input').setValue(false)
    expect(button('删除已选 (0)').attributes('disabled')).toBeDefined()
    await wrapper!.find('.tree-node-group > .tree-node').trigger('click')
    expect(button('删除已选 (1)')).toBeDefined()
    await button('完成').trigger('click')
    expect(wrapper!.findAll('input[type="checkbox"]')).toHaveLength(0)
    expect(wrapper!.findAll('.tree-child')).toHaveLength(32)
    expect(wrapper!.emitted('connection-select')).toBeUndefined()
  })

  it('cancelling confirmation never deletes and retains the checked connections', async () => {
    confirm.mockResolvedValue(false)
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    await button('删除已选 (3)').trigger('click')
    await flushPromises()
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining('3 个连接'))
    expect(deletes()).toEqual([])
    expect(alert).not.toHaveBeenCalled()
    expect(button('删除已选 (3)').attributes('disabled')).toBeUndefined()
  })

  it('deletes only checked connections, reports success, and clears selection events for deleted IDs', async () => {
    await render()
    await wrapper!.find('.tree-child').trigger('click')
    await button('批量管理').trigger('click')
    await wrapper!.findAll('.tree-node-group input')[0].setValue(true)
    await wrapper!.findAll('.tree-node-group input')[2].setValue(true)
    await button('删除已选 (2)').trigger('click')
    await flushPromises()
    expect(deletes()).toEqual([['delete_connection', { id: '1' }], ['delete_connection', { id: '3' }]])
    expect(confirm.mock.calls[0][0]).toContain('192.0.2.1:2404')
    expect(confirm.mock.calls[0][0]).not.toContain('192.0.2.2:2404')
    expect(rows.map(row => row.id)).toEqual(['2'])
    expect(wrapper!.emitted('connection-deleted')).toEqual([['1'], ['3']])
    expect(alert).toHaveBeenCalledWith('已删除 2 个连接，失败 0 个。')
    expect(button('批量管理')).toBeDefined()
    expect(wrapper!.findAll('.tree-node.selected')).toHaveLength(0)
  })

  it('continues after a failed deletion and retains the failed connection for retry', async () => {
    const invokeNormally = invokeMock.getMockImplementation()!
    invokeMock.mockImplementation((command, args) => {
      if (command === 'delete_connection' && args.id === '2') return Promise.reject(new Error('permission denied'))
      return invokeNormally(command, args)
    })
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    await button('删除已选 (3)').trigger('click')
    await flushPromises()
    expect(deletes().map(([, args]) => args.id)).toEqual(['1', '2', '3'])
    expect(alert).toHaveBeenCalledWith(expect.stringContaining('已删除 2 个连接，失败 1 个。'))
    expect(alert.mock.calls[0][0]).toContain('192.0.2.2:2404: Error: permission denied')
    expect(button('删除已选 (1)').attributes('disabled')).toBeUndefined()
    expect((wrapper!.find('.tree-node-group input').element as HTMLInputElement).checked).toBe(true)
  })

  it('locks the selection and prevents duplicate submissions while showing deletion progress', async () => {
    const pending = deferred<void>()
    const invokeNormally = invokeMock.getMockImplementation()!
    invokeMock.mockImplementation(async (command, args) => {
      if (command === 'delete_connection' && args.id === '1') await pending.promise
      return invokeNormally(command, args)
    })
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    const deleteButton = button('删除已选 (3)')
    await deleteButton.trigger('click')
    await flushPromises()
    expect(button('删除中 0/3').attributes('disabled')).toBeDefined()
    expect(button('完成').attributes('disabled')).toBeDefined()
    expect(wrapper!.findAll('input').every(input => input.attributes('disabled') !== undefined)).toBe(true)
    await deleteButton.trigger('click')
    await wrapper!.findAll('.tree-node-group')[1].find('.tree-node').trigger('click')
    expect(confirm).toHaveBeenCalledTimes(1)
    pending.resolve()
    await flushPromises()
    expect(deletes()).toHaveLength(3)
  })

  it('does not start deleting if the workspace changed during confirmation', async () => {
    const pending = deferred<boolean>()
    confirm.mockReturnValue(pending.promise)
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    await button('删除已选 (3)').trigger('click')
    wrapper!.unmount()
    wrapper = null
    pending.resolve(true)
    await flushPromises()
    expect(deletes()).toEqual([])
  })

  it('keeps the confirmed batch fixed when a new connection appears while confirmation is open', async () => {
    const pending = deferred<boolean>()
    confirm.mockReturnValue(pending.promise)
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    await button('删除已选 (3)').trigger('click')
    rows.push(connection('4'))
    treeRefreshKey.value++
    await flushPromises()
    pending.resolve(true)
    await flushPromises()
    expect(deletes().map(([, args]) => args.id)).toEqual(['1', '2', '3'])
    expect(rows.map(row => row.id)).toEqual(['4'])
  })

  it('stops the batch after an in-flight deletion when the tree is unmounted', async () => {
    const pending = deferred<void>()
    const invokeNormally = invokeMock.getMockImplementation()!
    invokeMock.mockImplementation(async (command, args) => {
      if (command === 'delete_connection') await pending.promise
      return invokeNormally(command, args)
    })
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    await button('删除已选 (3)').trigger('click')
    await flushPromises()
    const previousTree = wrapper!
    wrapper!.unmount()
    wrapper = null
    pending.resolve()
    await flushPromises()
    expect(deletes()).toHaveLength(1)
    expect(previousTree.emitted('connection-deleted')).toBeUndefined()
    expect(alert).not.toHaveBeenCalled()
  })

  it('prunes removed checked IDs on refresh without selecting newly added connections', async () => {
    await render()
    await button('批量管理').trigger('click')
    await wrapper!.find('.batch-actions input').setValue(true)
    rows = [rows[1], connection('4')]
    treeRefreshKey.value++
    await flushPromises()
    expect(button('删除已选 (1)')).toBeDefined()
    const checks = wrapper!.findAll('.tree-node-group input')
    expect((checks[0].element as HTMLInputElement).checked).toBe(true)
    expect((checks[1].element as HTMLInputElement).checked).toBe(false)
  })

  it('also confirms single deletion from the context menu and emits its deleted ID', async () => {
    await render()
    await wrapper!.find('.tree-node').trigger('contextmenu', { clientX: 30, clientY: 50 })
    await button('删除连接').trigger('click')
    await flushPromises()
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining('192.0.2.1:2404'))
    expect(deletes()).toEqual([['delete_connection', { id: '1' }]])
    expect(wrapper!.emitted('connection-deleted')).toEqual([['1']])
    expect(alert).not.toHaveBeenCalled()
  })
})
