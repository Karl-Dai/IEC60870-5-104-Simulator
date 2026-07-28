import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick, ref, type Ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import DataPointTable from '../src/components/DataPointTable.vue'
import type { DataPointInfo, DataPointValueSnapshot } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }))

interface Refs {
  selectedServerId: Ref<string | null>
  selectedCA: Ref<number | null>
  selectedCategory: Ref<string | null>
  dataRefreshKey: Ref<number>
  categoryCounts: Ref<Map<string, number>>
}

function point(value: string, timestamp = '00:00:00.000'): DataPointInfo {
  return {
    ioa: 1,
    asdu_type: 'M_SP_NA_1',
    category: 'single_point',
    name: 'SP 1',
    comment: 'static metadata',
    mapping_common_address: 2,
    mapping_ioa: 101,
    mapping_asdu_type: 'M_SP_NA_1',
    value,
    quality_ov: false,
    quality_bl: false,
    quality_sb: false,
    quality_nt: false,
    quality_iv: false,
    timestamp,
  }
}

function valueSnapshot(value: string, timestamp: string): DataPointValueSnapshot {
  return {
    ioa: 1,
    asdu_type: 'M_SP_NA_1',
    value,
    quality_ov: false,
    quality_bl: false,
    quality_sb: false,
    quality_nt: false,
    quality_iv: false,
    timestamp,
  }
}

function mountTable() {
  const refs: Refs = {
    selectedServerId: ref<string | null>(null),
    selectedCA: ref<number | null>(null),
    selectedCategory: ref<string | null>(null),
    dataRefreshKey: ref(0),
    categoryCounts: ref(new Map()),
  }
  const wrapper = mount(DataPointTable, {
    global: {
      provide: {
        ...refs,
        [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
      },
      stubs: {
        DataPointModal: true,
        BatchAddModal: true,
        BatchWriteModal: true,
        BatchControlOptionsModal: true,
      },
    },
  })
  return { wrapper, refs }
}

async function settle() {
  await flushPromises()
  await nextTick()
}

async function selectStation(refs: Refs, serverId = 's1', ca = 1) {
  refs.selectedServerId.value = serverId
  refs.selectedCA.value = ca
  await settle()
}

function commandCalls(command: string) {
  return invokeMock.mock.calls.filter(([name]) => name === command)
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

function installBackend(getValues?: () => Promise<DataPointValueSnapshot[]>) {
  let active = true
  let fullPolls = 0
  invokeMock.mockImplementation((command: string) => {
    if (command === 'get_remote_operation_config') {
      return Promise.resolve({ sync_tb_by_category: {} })
    }
    if (command === 'list_data_points_since') {
      fullPolls += 1
      // The 2 s table poll samples an even number of 1 s flips and therefore
      // only ever sees OFF. The active-point poll must expose the intermediate ON.
      return Promise.resolve({
        points: [point('OFF', `${String(Date.now()).padStart(12, '0')}`)],
        seq: Math.max(1, Math.floor(Date.now() / 1000)),
        total_count: 1,
      })
    }
    if (command === 'list_point_mutations') {
      return Promise.resolve(active
        ? [{ ioa: 1, asdu_type: 'M_SP_NA_1', mode: 'flip', period_ms: 1000 }]
        : [])
    }
    if (command === 'get_data_point_values') {
      if (getValues) return getValues()
      const tick = Math.floor(Date.now() / 1000)
      const value = tick % 2 === 1 ? 'ON' : 'OFF'
      return Promise.resolve([valueSnapshot(value, `tick-${tick}`)])
    }
    return Promise.resolve()
  })
  return {
    stopMutation: () => { active = false },
    fullPollCount: () => fullPolls,
  }
}

describe('DataPointTable 活动变位轮询', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(0)
    invokeMock.mockReset()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('1000ms SP 即使被 2s 全表轮询混叠,仍逐次显示 ON/OFF', async () => {
    installBackend()
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      refreshActiveMutations: () => Promise<void>
      activeMutations: Map<string, string>
    }
    await vm.refreshActiveMutations()
    await settle()

    expect(vm.activeMutations.get('1:M_SP_NA_1')).toBe('flip')
    expect(wrapper.find('.value-text').text()).toBe('OFF')

    await vi.advanceTimersByTimeAsync(1000)
    await settle()
    expect(wrapper.find('.value-text').text()).toBe('ON')

    await vi.advanceTimersByTimeAsync(1000)
    await settle()
    expect(wrapper.find('.value-text').text()).toBe('OFF')
    expect(commandCalls('get_data_point_values').length).toBeGreaterThanOrEqual(8)

    // Lightweight snapshots must not erase definition metadata.
    const displayed = (wrapper.vm as unknown as { displayPoints: DataPointInfo[] }).displayPoints[0]
    expect(displayed.name).toBe('SP 1')
    expect(displayed.mapping_ioa).toBe(101)
    wrapper.unmount()
  })

  it('s1 慢响应占用 in-flight 时切到 s2,丢弃 s1 并在 finally 补载 s2', async () => {
    const delayedS1 = deferred<{ points: DataPointInfo[]; seq: number; total_count: number }>()
    invokeMock.mockImplementation((command: string, args?: { serverId?: string }) => {
      if (command === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: {} })
      }
      if (command === 'list_data_points_since') {
        if (args?.serverId === 's1') return delayedS1.promise
        return Promise.resolve({
          points: [{ ...point('S2'), name: 'server two' }],
          seq: 20,
          total_count: 1,
        })
      }
      if (command === 'list_point_mutations') return Promise.resolve([])
      return Promise.resolve()
    })
    const { wrapper, refs } = mountTable()

    refs.selectedServerId.value = 's1'
    refs.selectedCA.value = 1
    await nextTick()
    await Promise.resolve()
    expect(commandCalls('list_data_points_since')).toHaveLength(1)

    refs.selectedServerId.value = 's2'
    refs.selectedCA.value = 2
    await nextTick()
    await Promise.resolve()
    // s1 尚未结束，s2 此刻只登记 pending。
    expect(commandCalls('list_data_points_since')).toHaveLength(1)

    delayedS1.resolve({
      points: [{ ...point('STALE-S1'), name: 'server one' }],
      seq: 10,
      total_count: 1,
    })
    await settle()

    const calls = commandCalls('list_data_points_since')
    expect(calls).toHaveLength(2)
    expect(calls[1][1]).toMatchObject({ serverId: 's2', commonAddress: 2, sinceSeq: 0 })
    expect(wrapper.find('.value-text').text()).toBe('S2')
    expect((wrapper.vm as unknown as { displayPoints: DataPointInfo[] }).displayPoints[0].name)
      .toBe('server two')
    wrapper.unmount()
  })

  it('targeted 新快照先落地时,迟到慢增量只更新活动点静态字段', async () => {
    const delayedSlow = deferred<{ points: DataPointInfo[]; seq: number; total_count: number }>()
    let slowCalls = 0
    const p2 = { ...point('OFF'), ioa: 2, name: 'SP 2', mapping_ioa: 102 }
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: {} })
      }
      if (command === 'list_data_points_since') {
        slowCalls += 1
        if (slowCalls === 1) {
          return Promise.resolve({ points: [point('OFF'), p2], seq: 1, total_count: 2 })
        }
        return delayedSlow.promise
      }
      if (command === 'list_point_mutations') {
        return Promise.resolve([{ ioa: 1, asdu_type: 'M_SP_NA_1', mode: 'flip', period_ms: 1000 }])
      }
      if (command === 'get_data_point_values') {
        return Promise.resolve([{ ...valueSnapshot('ON', 'targeted-new'), quality_nt: true }])
      }
      return Promise.resolve()
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      refreshActiveMutations: () => Promise<void>
      displayPoints: DataPointInfo[]
    }
    await vm.refreshActiveMutations()
    await settle()

    refs.dataRefreshKey.value++
    await nextTick()
    await Promise.resolve()
    expect(slowCalls).toBe(2)

    await vi.advanceTimersByTimeAsync(250)
    await settle()
    expect(vm.displayPoints.find((p) => p.ioa === 1)?.value).toBe('ON')

    delayedSlow.resolve({
      points: [
        { ...point('OFF', 'slow-old'), name: 'renamed by slow poll' },
        { ...p2, value: 'ON', timestamp: 'slow-p2' },
      ],
      seq: 3,
      total_count: 2,
    })
    await settle()

    const active = vm.displayPoints.find((p) => p.ioa === 1)!
    expect(active.name).toBe('renamed by slow poll')
    expect(active.value).toBe('ON')
    expect(active.quality_nt).toBe(true)
    expect(active.timestamp).toBe('targeted-new')
    // 非活动点仍完整采用慢轮询返回的动态字段。
    expect(vm.displayPoints.find((p) => p.ioa === 2)?.value).toBe('ON')
    wrapper.unmount()
  })

  it('全量 resync 迟到时同样保留活动点 targeted 动态字段', async () => {
    const delayedFull = deferred<{ points: DataPointInfo[]; seq: number; total_count: number }>()
    let slowCalls = 0
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: {} })
      }
      if (command === 'list_data_points_since') {
        slowCalls += 1
        if (slowCalls === 1) {
          return Promise.resolve({ points: [point('OFF')], seq: 1, total_count: 1 })
        }
        if (slowCalls === 2) {
          // 触发删除/新增检测后的 sinceSeq=0 全量重建。
          return Promise.resolve({ points: [], seq: 2, total_count: 2 })
        }
        return delayedFull.promise
      }
      if (command === 'list_point_mutations') {
        return Promise.resolve([{ ioa: 1, asdu_type: 'M_SP_NA_1', mode: 'flip', period_ms: 1000 }])
      }
      if (command === 'get_data_point_values') {
        return Promise.resolve([valueSnapshot('ON', 'targeted-during-resync')])
      }
      return Promise.resolve()
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      refreshActiveMutations: () => Promise<void>
      displayPoints: DataPointInfo[]
    }
    await vm.refreshActiveMutations()
    await settle()

    refs.dataRefreshKey.value++
    await nextTick()
    await Promise.resolve()
    await Promise.resolve()
    expect(slowCalls).toBe(3)

    await vi.advanceTimersByTimeAsync(250)
    await settle()
    expect(vm.displayPoints[0].value).toBe('ON')

    delayedFull.resolve({
      points: [
        { ...point('OFF', 'slow-full-old'), name: 'renamed by resync' },
        { ...point('OFF'), ioa: 2, name: 'new point' },
      ],
      seq: 3,
      total_count: 2,
    })
    await settle()

    const active = vm.displayPoints.find((p) => p.ioa === 1)!
    expect(active.name).toBe('renamed by resync')
    expect(active.value).toBe('ON')
    expect(active.timestamp).toBe('targeted-during-resync')
    expect(vm.displayPoints.find((p) => p.ioa === 2)?.name).toBe('new point')
    wrapper.unmount()
  })

  it('前一次活动点请求未完成时不发起重叠请求', async () => {
    let resolveFirst!: (value: DataPointValueSnapshot[]) => void
    const first = new Promise<DataPointValueSnapshot[]>((resolve) => { resolveFirst = resolve })
    let calls = 0
    installBackend(() => {
      calls += 1
      return calls === 1 ? first : Promise.resolve([valueSnapshot('ON', 'later')])
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { refreshActiveMutations: () => Promise<void> }
    await vm.refreshActiveMutations()
    await settle()

    await vi.advanceTimersByTimeAsync(250)
    expect(commandCalls('get_data_point_values')).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1000)
    expect(commandCalls('get_data_point_values')).toHaveLength(1)

    resolveFirst([valueSnapshot('ON', 'first')])
    await settle()
    await vi.advanceTimersByTimeAsync(249)
    expect(commandCalls('get_data_point_values')).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(commandCalls('get_data_point_values')).toHaveLength(2)
    wrapper.unmount()
  })

  it('停止最后一个变位后清理快速轮询,保留原 2s 全表轮询', async () => {
    const backend = installBackend()
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      refreshActiveMutations: () => Promise<void>
      activeMutations: Map<string, string>
    }
    await vm.refreshActiveMutations()
    await settle()
    await vi.advanceTimersByTimeAsync(250)
    const activeCalls = commandCalls('get_data_point_values').length
    const fullBefore = backend.fullPollCount()

    backend.stopMutation()
    await vm.refreshActiveMutations()
    await settle()
    expect(vm.activeMutations.size).toBe(0)

    await vi.advanceTimersByTimeAsync(2000)
    await settle()
    expect(commandCalls('get_data_point_values')).toHaveLength(activeCalls)
    expect(backend.fullPollCount()).toBeGreaterThan(fullBefore)
    wrapper.unmount()
  })

  it('切站后丢弃旧 server/CA/epoch 的迟到快照', async () => {
    let resolveOld!: (value: DataPointValueSnapshot[]) => void
    const oldRequest = new Promise<DataPointValueSnapshot[]>((resolve) => { resolveOld = resolve })
    installBackend(() => oldRequest)
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { refreshActiveMutations: () => Promise<void> }
    await vm.refreshActiveMutations()
    await settle()
    await vi.advanceTimersByTimeAsync(250)
    expect(commandCalls('get_data_point_values')).toHaveLength(1)

    await selectStation(refs, 's2', 2)
    expect(wrapper.find('.value-text').text()).toBe('OFF')
    resolveOld([valueSnapshot('ON', 'late-from-s1')])
    await settle()

    expect(wrapper.find('.value-text').text()).toBe('OFF')
    expect((wrapper.vm as unknown as { displayPoints: DataPointInfo[] }).displayPoints[0].timestamp)
      .not.toBe('late-from-s1')
    wrapper.unmount()
  })

  it('卸载时取消 timer,并丢弃在途活动点响应且不再续轮询', async () => {
    let resolveRequest!: (value: DataPointValueSnapshot[]) => void
    const pending = new Promise<DataPointValueSnapshot[]>((resolve) => { resolveRequest = resolve })
    installBackend(() => pending)
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { refreshActiveMutations: () => Promise<void> }
    await vm.refreshActiveMutations()
    await settle()
    await vi.advanceTimersByTimeAsync(250)
    expect(commandCalls('get_data_point_values')).toHaveLength(1)

    wrapper.unmount()
    resolveRequest([valueSnapshot('ON', 'late')])
    await settle()
    await vi.advanceTimersByTimeAsync(2000)
    expect(commandCalls('get_data_point_values')).toHaveLength(1)
    expect(commandCalls('list_data_points_since')).toHaveLength(1)
  })
})
