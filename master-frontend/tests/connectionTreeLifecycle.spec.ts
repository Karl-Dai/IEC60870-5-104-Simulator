// issue #64: a tree from the replaced workspace can still have a pending
// list_connections request. Its late response must not overwrite the new tree
// or prune the new App instance's injected category/flash maps.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import ConnectionTree from '../src/components/ConnectionTree.vue'
import type { ChangedCategoriesMap, CategoryCountsMap, ConnectionInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}

function connection(id: string): ConnectionInfo {
  return {
    id,
    target_address: '192.0.2.1',
    port: 2404,
    state: 'Disconnected',
    common_addresses: [1],
  } as ConnectionInfo
}

function mountTree(
  categoryCounts = ref<CategoryCountsMap>(new Map()),
  changedCategories = ref<ChangedCategoriesMap>(new Map()),
) {
  const treeRefreshKey = ref(0)
  const wrapper = mount(ConnectionTree, {
    global: {
      provide: {
        treeRefreshKey,
        refreshTree: vi.fn(),
        openEditConnection: vi.fn(),
        categoryCounts,
        changedCategories,
      },
    },
  })
  return { wrapper, treeRefreshKey, categoryCounts, changedCategories }
}

describe('ConnectionTree workspace lifecycle', () => {
  let wrapper: VueWrapper | null = null

  beforeEach(() => {
    invokeMock.mockReset()
  })

  afterEach(() => {
    wrapper?.unmount()
    wrapper = null
  })

  it('does not commit or prune injected maps after unmount', async () => {
    const pending = deferred<ConnectionInfo[]>()
    invokeMock.mockReturnValue(pending.promise)
    const categoryCounts = ref<CategoryCountsMap>(new Map([
      ['new-workspace', new Map()],
    ]))
    const changedCategories = ref<ChangedCategoriesMap>(new Map([
      ['new-workspace', new Map([[1, new Set(['single_point'])]])],
    ]))
    const mounted = mountTree(categoryCounts, changedCategories)
    wrapper = mounted.wrapper
    await nextTick()
    const vm = wrapper.vm as unknown as { connections: unknown[] }

    wrapper.unmount()
    wrapper = null
    pending.resolve([connection('old-workspace')])
    await flushPromises()

    expect(vm.connections).toEqual([])
    expect(categoryCounts.value.has('new-workspace')).toBe(true)
    expect(changedCategories.value.has('new-workspace')).toBe(true)
  })

  it('lets only the newest overlapping list request update the tree', async () => {
    const stale = deferred<ConnectionInfo[]>()
    let requestCount = 0
    invokeMock.mockImplementation(() => {
      requestCount++
      return requestCount === 1
        ? stale.promise
        : Promise.resolve([connection('fresh')])
    })
    const mounted = mountTree()
    wrapper = mounted.wrapper
    await nextTick()

    mounted.treeRefreshKey.value++
    await nextTick()
    await flushPromises()
    expect(wrapper.find('.node-label').text()).toContain('192.0.2.1')

    stale.resolve([connection('stale')])
    await flushPromises()

    const vm = wrapper.vm as unknown as { connections: Array<{ info: ConnectionInfo }> }
    expect(vm.connections.map(item => item.info.id)).toEqual(['fresh'])
  })
})
