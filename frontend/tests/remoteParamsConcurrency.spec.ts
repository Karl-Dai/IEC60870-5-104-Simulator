import { beforeEach, describe, it, expect, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import RemoteParamsDrawer from '../src/components/RemoteParamsDrawer.vue'
import RemoteParamsModal from '../src/components/RemoteParamsModal.vue'
import DataPointTable from '../src/components/DataPointTable.vue'
import { dialogKey } from '@shared/composables/useDialog'
import {
  DEFAULT_PROTOCOL_TIMING,
  DEFAULT_REMOTE_OPS,
  type ProtocolTimingConfig,
  type RemoteOperationConfig,
} from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((r, j) => {
    resolve = r
    reject = j
  })
  return { promise, resolve, reject }
}

function cloneOps(sp: boolean): RemoteOperationConfig {
  const value = JSON.parse(JSON.stringify(DEFAULT_REMOTE_OPS)) as RemoteOperationConfig
  value.sync_tb_by_category.sp = sp
  return value
}

describe('运行参数异步竞态保护', () => {
  beforeEach(() => invokeMock.mockReset())

  it('drawer ignores a late response from the previously selected server', async () => {
    const pending = new Map<string, ReturnType<typeof deferred<unknown>>>()
    for (const id of ['s1', 's2']) {
      pending.set(`get_protocol_timing:${id}`, deferred<ProtocolTimingConfig>())
      pending.set(`get_remote_operation_config:${id}`, deferred<RemoteOperationConfig>())
    }
    invokeMock.mockImplementation((cmd: string, args?: { serverId?: string }) => {
      const request = pending.get(`${cmd}:${args?.serverId}`)
      if (!request) return Promise.resolve(null)
      return request.promise
    })

    const selectedServerId = ref<string | null>('s1')
    const wrapper = mount(RemoteParamsDrawer, {
      props: { visible: false },
      global: {
        stubs: { teleport: true },
        provide: { selectedServerId },
      },
    })

    selectedServerId.value = 's2'
    await nextTick()

    pending.get('get_protocol_timing:s2')!.resolve({ ...DEFAULT_PROTOCOL_TIMING, t0: 22 })
    pending.get('get_remote_operation_config:s2')!.resolve(cloneOps(true))
    await flushPromises()

    // The old s1 request completes last. It must not overwrite the selected s2.
    pending.get('get_protocol_timing:s1')!.resolve({ ...DEFAULT_PROTOCOL_TIMING, t0: 11 })
    pending.get('get_remote_operation_config:s1')!.resolve(cloneOps(false))
    await flushPromises()

    await wrapper.setProps({ visible: true })
    await flushPromises()

    const spLabel = wrapper
      .findAll('.rp-subgroup .rp-switch')
      .find((label) => label.text().includes('M_SP_NA_1'))!
    expect((spLabel.find('input').element as HTMLInputElement).checked).toBe(true)
    expect(wrapper.find('.rp-btn-ghost').exists()).toBe(false)
    wrapper.unmount()
  })

  it('an old drawer save cannot clear or overwrite the current server load error', async () => {
    const timingWrite = deferred<null>()
    const opsWrite = deferred<null>()
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      const id = String(args?.serverId ?? '')
      if (cmd === 'get_protocol_timing') {
        return id === 's2'
          ? Promise.reject(new Error('s2 load failed'))
          : Promise.resolve({ ...DEFAULT_PROTOCOL_TIMING })
      }
      if (cmd === 'get_remote_operation_config') return Promise.resolve(cloneOps(false))
      if (cmd === 'set_protocol_timing') return timingWrite.promise
      if (cmd === 'set_remote_operation_config') return opsWrite.promise
      return Promise.resolve(null)
    })

    const selectedServerId = ref<string | null>('s1')
    const wrapper = mount(RemoteParamsDrawer, {
      props: { visible: false },
      global: {
        stubs: { teleport: true },
        provide: { selectedServerId },
      },
    })
    await flushPromises()
    await wrapper.setProps({ visible: true })
    await flushPromises()

    const spLabel = wrapper
      .findAll('.rp-subgroup .rp-switch')
      .find((label) => label.text().includes('M_SP_NA_1'))!
    await spLabel.find('input').setValue(true)
    void wrapper.find('.rp-btn-primary').trigger('click')
    await nextTick()

    selectedServerId.value = 's2'
    await flushPromises()
    expect(wrapper.find('.rp-error').text()).toContain('s2 load failed')

    // A 的 timing 成功后会继续发起 ops。旧实现会在调用 applyOps 时清掉
    // B 的加载错误；修复后错误通道只归当前选择/会话所有。
    timingWrite.resolve(null)
    await flushPromises()
    expect(invokeMock.mock.calls.some((call) => call[0] === 'set_remote_operation_config')).toBe(true)
    expect(wrapper.find('.rp-error').text()).toContain('s2 load failed')

    // A 的 ops 最终失败也不能把错误覆盖成 A 的保存错误。
    opsWrite.reject(new Error('s1 save failed'))
    await flushPromises()
    expect(wrapper.find('.rp-error').text()).toContain('s2 load failed')
    expect(wrapper.find('.rp-error').text()).not.toContain('s1 save failed')
    wrapper.unmount()
  })

  it('closing a save in flight cannot redirect the remaining writes to another server', async () => {
    const timingWrite = deferred<null>()
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      const id = String(args?.serverId ?? '')
      if (cmd === 'get_protocol_timing') {
        return Promise.resolve({ ...DEFAULT_PROTOCOL_TIMING, t0: id === 's1' ? 11 : 22 })
      }
      if (cmd === 'get_remote_operation_config') {
        return Promise.resolve(cloneOps(id === 's2'))
      }
      if (cmd === 'list_servers') {
        return Promise.resolve([
          { id: 's1', bind_address: '0.0.0.0', port: 2404, state: 'Stopped' },
          { id: 's2', bind_address: '0.0.0.0', port: 2405, state: 'Stopped' },
        ])
      }
      if (cmd === 'set_protocol_timing') return timingWrite.promise
      if (cmd === 'set_remote_operation_config') return Promise.resolve(null)
      return Promise.resolve(null)
    })

    const wrapper = mount(RemoteParamsModal, {
      props: { visible: false, serverId: 's1', serverLabel: 's1' },
      global: { stubs: { teleport: true } },
    })
    await wrapper.setProps({ visible: true })
    await flushPromises()

    void wrapper.find('.btn-primary').trigger('click')
    await nextTick()
    expect(invokeMock.mock.calls.some((call) => call[0] === 'set_protocol_timing')).toBe(true)

    // 保存过程中所有用户关闭入口都必须失效。
    await wrapper.find('.btn-close').trigger('click')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await wrapper.find('.modal-backdrop').trigger('click')
    await nextTick()
    expect(wrapper.emitted('close')).toBeUndefined()

    // 即使父组件因外部原因强制换到 s2，旧保存会话也必须固定写 s1，
    // 且完成后不能再 emit close 把新弹窗关掉。
    await wrapper.setProps({ visible: false })
    await wrapper.setProps({ visible: true, serverId: 's2', serverLabel: 's2' })
    await flushPromises()

    timingWrite.resolve(null)
    await flushPromises()

    const opsWrite = invokeMock.mock.calls.find((call) => call[0] === 'set_remote_operation_config')
    expect(opsWrite?.[1]).toMatchObject({
      request: {
        server_id: 's1',
        ops: { sync_tb_by_category: { sp: false } },
      },
    })
    expect(wrapper.emitted('close')).toBeUndefined()
    wrapper.unmount()
  })

  it('an old modal save error cannot leak into a newly opened server session', async () => {
    const timingWrite = deferred<null>()
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      const id = String(args?.serverId ?? '')
      if (cmd === 'get_protocol_timing') {
        return Promise.resolve({ ...DEFAULT_PROTOCOL_TIMING, t0: id === 's1' ? 11 : 22 })
      }
      if (cmd === 'get_remote_operation_config') {
        return Promise.resolve(cloneOps(id === 's2'))
      }
      if (cmd === 'list_servers') {
        return Promise.resolve([
          { id: 's1', bind_address: '0.0.0.0', port: 2404, state: 'Stopped' },
          { id: 's2', bind_address: '0.0.0.0', port: 2405, state: 'Stopped' },
        ])
      }
      if (cmd === 'set_protocol_timing') return timingWrite.promise
      return Promise.resolve(null)
    })

    const wrapper = mount(RemoteParamsModal, {
      props: { visible: false, serverId: 's1', serverLabel: 's1' },
      global: { stubs: { teleport: true } },
    })
    await wrapper.setProps({ visible: true })
    await flushPromises()

    void wrapper.find('.btn-primary').trigger('click')
    await nextTick()
    await wrapper.setProps({ visible: false })
    await wrapper.setProps({ visible: true, serverId: 's2', serverLabel: 's2' })
    await flushPromises()
    expect(wrapper.find('.error').exists()).toBe(false)

    timingWrite.reject(new Error('s1 save failed'))
    await flushPromises()

    expect(wrapper.find('.error').exists()).toBe(false)
    expect(wrapper.emitted('close')).toBeUndefined()
    wrapper.unmount()
  })

  it('+TB refresh ignores a late response from the previously selected server', async () => {
    const oldS1 = deferred<{ sync_tb_by_category: { sp: boolean } }>()
    const firstS2 = deferred<{ sync_tb_by_category: { sp: boolean } }>()
    const refreshS2 = deferred<{ sync_tb_by_category: { sp: boolean } }>()
    let s2Reads = 0
    invokeMock.mockImplementation((cmd: string, args?: { serverId?: string }) => {
      if (cmd === 'get_remote_operation_config') {
        if (args?.serverId === 's1') return oldS1.promise
        return s2Reads++ === 0 ? firstS2.promise : refreshS2.promise
      }
      if (cmd === 'list_data_points_since') {
        return Promise.resolve({
          points: [{
            ioa: 1,
            asdu_type: 'M_SP_NA_1',
            category: 'single_point',
            name: 'p1',
            comment: '',
            value: '0',
            quality_ov: false,
            quality_bl: false,
            quality_sb: false,
            quality_nt: false,
            quality_iv: false,
            timestamp: null,
          }],
          seq: 1,
          total_count: 1,
        })
      }
      return Promise.resolve(null)
    })

    const selectedServerId = ref<string | null>('s1')
    const selectedCA = ref<number | null>(1)
    const selectedCategory = ref<string | null>(null)
    const dataRefreshKey = ref(0)
    const wrapper = mount(DataPointTable, {
      global: {
        provide: {
          selectedServerId,
          selectedCA,
          selectedCategory,
          dataRefreshKey,
          categoryCounts: ref(new Map()),
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
    await flushPromises()

    selectedServerId.value = 's2'
    await nextTick()
    dataRefreshKey.value++
    await nextTick()

    firstS2.resolve({ sync_tb_by_category: { sp: true } })
    refreshS2.resolve({ sync_tb_by_category: { sp: true } })
    await flushPromises()
    expect(wrapper.findAll('.tb-badge')).toHaveLength(1)

    oldS1.resolve({ sync_tb_by_category: { sp: false } })
    await flushPromises()
    expect(wrapper.findAll('.tb-badge')).toHaveLength(1)
    wrapper.unmount()
  })

})
