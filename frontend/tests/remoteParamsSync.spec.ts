// issue #28(v1.15.0 反馈):运行参数弹窗/抽屉互不同步。
// 根因:useRemoteParams 只在 selectedServerId 变化时 load,打开面板不会回读后端。
// 断言:1) 抽屉每次打开重载并反映后端最新值;2) 抽屉保存成功后 emit('saved')
//      (App 据此 bump dataRefreshKey 刷新 +TB 徽标);3) 弹窗同 serverId 二次打开重载。
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref } from 'vue'
import { mount, flushPromises, VueWrapper } from '@vue/test-utils'
import RemoteParamsDrawer from '../src/components/RemoteParamsDrawer.vue'
import RemoteParamsModal from '../src/components/RemoteParamsModal.vue'
import { DEFAULT_PROTOCOL_TIMING, DEFAULT_REMOTE_OPS } from '../src/types'
import type { RemoteOperationConfig } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))

// 后端侧的"当前配置"。每次返回深拷贝,避免表单 v-model 直接改到这里。
let backendOps: RemoteOperationConfig

function setupInvokeMock() {
  invokeMock.mockReset()
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'get_protocol_timing':
        return Promise.resolve({ ...DEFAULT_PROTOCOL_TIMING })
      case 'get_remote_operation_config':
        return Promise.resolve(JSON.parse(JSON.stringify(backendOps)))
      case 'list_servers':
        return Promise.resolve([
          { id: 's1', bind_address: '0.0.0.0', port: 2404, state: 'Stopped' },
        ])
      case 'set_protocol_timing':
      case 'set_remote_operation_config':
        return Promise.resolve(null)
      default:
        return Promise.resolve(null)
    }
  })
}

const opsCalls = () =>
  invokeMock.mock.calls.filter(c => c[0] === 'get_remote_operation_config').length

// sp 分类的 sync-TB 复选框:按映射标签文本定位(.rp-switch 里还有别的开关,
// 如默认为 true 的 General Interrogation,直接取第一个会拿错)。
function spSwitch(w: VueWrapper) {
  const label = w
    .findAll('.rp-subgroup .rp-switch')
    .find(l => l.text().includes('M_SP_NA_1'))
  if (!label) throw new Error('sync-TB sp switch not found')
  return label.find('input[type="checkbox"]')
}
const spCheckbox = (w: VueWrapper) => spSwitch(w).element as HTMLInputElement

let w: VueWrapper | null = null

beforeEach(() => {
  backendOps = JSON.parse(JSON.stringify(DEFAULT_REMOTE_OPS))
  setupInvokeMock()
})
afterEach(() => {
  w?.unmount()
  w = null
})

describe('RemoteParamsDrawer 打开时重载', () => {
  function mountDrawer(visible = false): VueWrapper {
    w = mount(RemoteParamsDrawer, {
      props: { visible },
      global: {
        stubs: { teleport: true },
        provide: { selectedServerId: ref('s1') },
      },
    })
    return w
  }

  it('打开抽屉回读后端:期间被弹窗改过的 sync-TB 勾选立即可见', async () => {
    const ww = mountDrawer(false)
    await flushPromises()
    const callsAfterMount = opsCalls() // 挂载时 selectedServerId watch 已 load 一次

    // 模拟:用户经右键弹窗把 sp 的 sync-TB 勾上并保存(后端已变,抽屉内存是旧快照)
    backendOps.sync_tb_by_category.sp = true

    await ww.setProps({ visible: true })
    await flushPromises()

    expect(opsCalls()).toBeGreaterThan(callsAfterMount) // 打开必须重新 load
    expect(spCheckbox(ww).checked).toBe(true) // 不再显示过期的 false
  })

  it('保存成功后 emit saved,且 dirty 基线随重载重置', async () => {
    const ww = mountDrawer(true)
    await flushPromises()

    // 打开即重载 → 基线为后端最新值,未改动时不应是 dirty(没有 Save 高亮误导)
    expect(ww.emitted('saved')).toBeUndefined()

    await spSwitch(ww).setValue(true)
    await ww.find('.rp-btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock.mock.calls.some(c => c[0] === 'set_remote_operation_config')).toBe(true)
    expect(ww.emitted('saved')).toHaveLength(1)
  })
})

describe('RemoteParamsModal 二次打开重载', () => {
  it('同一 serverId 关闭再打开:期间被抽屉改过的勾选立即可见', async () => {
    w = mount(RemoteParamsModal, {
      props: { visible: true, serverId: 's1', serverLabel: '0.0.0.0:2404' },
      global: { stubs: { teleport: true } },
    })
    await flushPromises()
    expect(spCheckbox(w).checked).toBe(false)

    await w.setProps({ visible: false })
    // 模拟:用户经抽屉把 sp 勾上并 Save All(serverId 不变,composable watch 不会触发)
    backendOps.sync_tb_by_category.sp = true
    const before = opsCalls()

    await w.setProps({ visible: true })
    await flushPromises()

    expect(opsCalls()).toBeGreaterThan(before)
    expect(spCheckbox(w).checked).toBe(true)
  })
})
