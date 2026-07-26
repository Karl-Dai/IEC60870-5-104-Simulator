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

  // 关掉抽屉保留草稿是原有设计(dirty 时才出现的 Discard 按钮就是为此)。
  // 打开时无条件 load() 会让 Esc / 点背景关掉再打开 = 静默 Discard,无确认无提示。
  it('有未保存编辑时关闭再打开:草稿保留,不静默回读覆盖', async () => {
    const ww = mountDrawer(true)
    await flushPromises()

    await spSwitch(ww).setValue(true) // 用户改了但没保存
    expect(ww.find('.rp-btn-ghost').exists()).toBe(true) // dirty → Discard 按钮出现
    const before = opsCalls()

    await ww.setProps({ visible: false }) // Esc / 点背景关闭
    await ww.setProps({ visible: true })
    await flushPromises()

    expect(opsCalls()).toBe(before) // 没有回读 → 没覆盖草稿
    expect(spCheckbox(ww).checked).toBe(true) // 编辑仍在
    expect(ww.find('.rp-btn-ghost').exists()).toBe(true) // 仍可显式 Discard
  })

  it('显式 Discard 后回到后端值,基线重置(Discard 按钮消失)', async () => {
    const ww = mountDrawer(true)
    await flushPromises()

    await spSwitch(ww).setValue(true)
    await ww.find('.rp-btn-ghost').trigger('click')
    await flushPromises()

    expect(spCheckbox(ww).checked).toBe(false)
    expect(ww.find('.rp-btn-ghost').exists()).toBe(false)
  })

  // useRemoteParams.load() 在 id 为 null 时早退、不翻转 loading,抽屉又借 watch(loading)
  // 重置基线 —— 若不兜底,基线会残留上一台服务器的快照,留下点了也没用的幽灵 Discard。
  it('选中服务器被清空(如删除服务器)不留幽灵 Discard 按钮', async () => {
    // 后端值必须与 DEFAULT_REMOTE_OPS 不同,否则"基线残留"与"重置为默认"
    // 快照相同,测不出问题
    backendOps.sync_tb_by_category.dp = true
    const sel = ref<string | null>('s1')
    w = mount(RemoteParamsDrawer, {
      props: { visible: true },
      global: { stubs: { teleport: true }, provide: { selectedServerId: sel } },
    })
    await flushPromises()
    await spSwitch(w).setValue(true)
    expect(w.find('.rp-btn-ghost').exists()).toBe(true)

    sel.value = null
    await flushPromises()

    expect(w.find('.rp-btn-ghost').exists()).toBe(false)
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

  // 弹窗是「取消 / 保存」语义:没有 Discard 按钮、也没有 dirty 指示,
  // 所以「取消」必须真的取消。曾经为了保留草稿而跳过回读,导致用户放弃的改动
  // 在下次保存时被静默写回后端(issue #28 审查发现),这两条用例守住反向行为。
  it('取消再打开:未保存的改动被丢弃,回读后端值', async () => {
    w = mount(RemoteParamsModal, {
      props: { visible: true, serverId: 's1', serverLabel: '0.0.0.0:2404' },
      global: { stubs: { teleport: true } },
    })
    await flushPromises()

    await spSwitch(w).setValue(true) // 改了但点了「取消」
    const before = opsCalls()

    await w.setProps({ visible: false })
    await w.setProps({ visible: true })
    await flushPromises()

    expect(opsCalls()).toBeGreaterThan(before) // 重新回读了后端
    expect(spCheckbox(w).checked).toBe(false) // 放弃的改动没留下
  })

  it('取消端口改动后再打开:端口回到后端值,且保存不会写回被放弃的端口', async () => {
    // 注意:loadTransport 只在 visible false→true 时跑,所以先关着挂载再打开
    w = mount(RemoteParamsModal, {
      props: { visible: false, serverId: 's1', serverLabel: '0.0.0.0:2404' },
      global: { stubs: { teleport: true } },
    })
    await w.setProps({ visible: true })
    await flushPromises()
    // visible 每次 false→true 都会重建 modal 的 DOM,元素引用不能跨开关复用
    const portValue = () =>
      (w!.find('.rp-conn-grid input[type="number"]').element as HTMLInputElement).value
    expect(portValue()).toBe('2404') // 后端值

    await w.find('.rp-conn-grid input[type="number"]').setValue(2410) // 改端口但点了取消
    await w.setProps({ visible: false })
    await w.setProps({ visible: true })
    await flushPromises()

    expect(portValue()).toBe('2404') // 草稿被丢弃,回到后端值

    // 关键回归:此时为改别的设置而点保存,不能把已放弃的 2410 写进后端
    await spSwitch(w).setValue(true)
    await w.find('.btn-primary').trigger('click')
    await flushPromises()
    const transportCalls = invokeMock.mock.calls.filter(c => c[0] === 'update_server_transport')
    expect(transportCalls).toHaveLength(0)
  })

  it('保存成功后再打开:仍会回读后端最新值', async () => {
    w = mount(RemoteParamsModal, {
      props: { visible: true, serverId: 's1', serverLabel: '0.0.0.0:2404' },
      global: { stubs: { teleport: true } },
    })
    await flushPromises()

    await spSwitch(w).setValue(true)
    await w.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(w.emitted('saved')).toHaveLength(1)

    // 保存已落库(后端也随之为 true),重开必须回读后端
    backendOps.sync_tb_by_category.sp = true
    const before = opsCalls()
    await w.setProps({ visible: false })
    await w.setProps({ visible: true })
    await flushPromises()

    expect(opsCalls()).toBeGreaterThan(before)
    expect(spCheckbox(w).checked).toBe(true)
  })
})
