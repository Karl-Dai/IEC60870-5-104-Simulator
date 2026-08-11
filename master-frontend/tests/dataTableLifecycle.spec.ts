// issue #64: workspaceEpoch remounts must retire the old table completely.
// A delayed IPC response may neither repaint injected state nor resurrect the
// old instance's polling interval after it has been reset/unmounted.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick, ref, type Ref } from 'vue'
import DataTable from '../src/components/DataTable.vue'
import type { IncrementalDataResponse, ReceivedDataPointInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}

function point(ioa: number, category: string): ReceivedDataPointInfo {
  return {
    ioa,
    asdu_type: 'M_SP_NA_1',
    asdu_type_id: 1,
    value: 'on',
    category,
    common_address: 1,
    quality_ov: false,
    quality_bl: false,
    quality_sb: false,
    quality_nt: false,
    quality_iv: false,
    timestamp: null,
  }
}

function provideRefs() {
  return {
    selectedConnectionId: ref<string | null>('conn-1') as Ref<string | null>,
    selectedCA: ref<number | null>(null) as Ref<number | null>,
    selectedCategory: ref<string | null>(null) as Ref<string | null>,
    dataRefreshKey: ref(0),
    changedCategories: ref(new Map()),
    categoryCounts: ref(new Map()),
  }
}

describe('DataTable workspace lifecycle', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('ignores a deferred fetch after unmount and does not restart polling', async () => {
    const stale = deferred<IncrementalDataResponse>()
    invokeMock.mockReturnValue(stale.promise)
    const provided = provideRefs()
    const wrapper = mount(DataTable, { global: { provide: provided } })
    await nextTick()
    const vm = wrapper.vm as unknown as { displayPoints: ReceivedDataPointInfo[] }

    wrapper.unmount()
    stale.resolve({ points: [point(1, 'stale')], seq: 1 })
    await flushPromises()

    expect(vm.displayPoints).toEqual([])
    expect(provided.categoryCounts.value.size).toBe(0)
    expect(provided.changedCategories.value.size).toBe(0)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('ignores the previous workspace request after reset and same-id reselection', async () => {
    const stale = deferred<IncrementalDataResponse>()
    let fetchCount = 0
    invokeMock.mockImplementation((command: string) => {
      if (command !== 'get_received_data_since') return Promise.resolve(undefined)
      fetchCount++
      if (fetchCount === 1) return stale.promise
      return Promise.resolve({ points: [point(2, 'fresh')], seq: 1 })
    })
    const provided = provideRefs()
    const wrapper = mount(DataTable, { global: { provide: provided } })
    await nextTick()

    provided.selectedConnectionId.value = null
    await nextTick()
    provided.selectedConnectionId.value = 'conn-1'
    await nextTick()
    await flushPromises()

    const vm = wrapper.vm as unknown as { displayPoints: ReceivedDataPointInfo[] }
    expect(vm.displayPoints.map(item => item.ioa)).toEqual([2])

    stale.resolve({ points: [point(1, 'stale')], seq: 99 })
    await flushPromises()

    expect(vm.displayPoints.map(item => item.ioa)).toEqual([2])
    expect(provided.categoryCounts.value.get('conn-1')?.get(1)?.has('stale')).toBe(false)
    wrapper.unmount()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('serializes slow polling and replays only one pending refresh', async () => {
    const slow = deferred<IncrementalDataResponse>()
    let fetchCount = 0
    invokeMock.mockImplementation((command: string) => {
      if (command !== 'get_received_data_since') return Promise.resolve(undefined)
      fetchCount++
      if (fetchCount === 1) return Promise.resolve({ points: [], seq: 0 })
      if (fetchCount === 2) return slow.promise
      return Promise.resolve({ points: [], seq: 1 })
    })
    const provided = provideRefs()
    const wrapper = mount(DataTable, { global: { provide: provided } })
    await flushPromises()
    expect(fetchCount).toBe(1)

    vi.advanceTimersByTime(1_000)
    await nextTick()
    expect(fetchCount).toBe(2)

    // Three more ticks occur while request #2 is pending. They must coalesce
    // instead of creating newer requests that make every slow response stale.
    vi.advanceTimersByTime(3_000)
    await nextTick()
    expect(fetchCount).toBe(2)

    slow.resolve({ points: [point(3, 'slow-but-current')], seq: 1 })
    await flushPromises()

    const vm = wrapper.vm as unknown as { displayPoints: ReceivedDataPointInfo[] }
    expect(vm.displayPoints.map(item => item.ioa)).toEqual([3])
    expect(fetchCount).toBe(3)
    wrapper.unmount()
  })
})
