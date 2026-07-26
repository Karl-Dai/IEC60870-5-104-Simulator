// issue #28:运行参数两个入口读的是不同的 serverId。
// 右键菜单「运行参数」把 contextMenu.serverId 交给弹窗(App 里独立的 ref),
// 而工具栏抽屉与数据表 +TB 徽标读的是【树选中的】selectedServerId。
// 多服务器时右键 B 改参数,抽屉/徽标仍显示 A;树上什么都没选时抽屉直接是空态。
// 这里在 App 层挂真实的树 + 抽屉 + 弹窗,断言右键入口会把树选中对齐过去。
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises, VueWrapper } from '@vue/test-utils'
import App from '../src/App.vue'
import { DEFAULT_PROTOCOL_TIMING, DEFAULT_REMOTE_OPS, type RemoteOperationConfig } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }))

// 两台服务器:s1 未开 sync-TB,s2 开了 sp —— 用勾选状态区分抽屉到底读了谁
const SERVERS = [
  { id: 's1', bind_address: '0.0.0.0', port: 2404, state: 'Stopped' },
  { id: 's2', bind_address: '0.0.0.0', port: 2405, state: 'Stopped' },
]
let opsByServer: Record<string, RemoteOperationConfig>

function setupInvokeMock() {
  invokeMock.mockReset()
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    const id = String(args?.serverId ?? '')
    switch (cmd) {
      case 'list_servers':
        return Promise.resolve(SERVERS)
      case 'list_stations':
        return Promise.resolve([{ common_address: 1, name: 'st1', point_count: 0 }])
      case 'list_data_points_since':
        return Promise.resolve({ points: [], seq: 0, total_count: 0 })
      case 'get_protocol_timing':
        return Promise.resolve({ ...DEFAULT_PROTOCOL_TIMING })
      case 'get_remote_operation_config':
        return Promise.resolve(JSON.parse(JSON.stringify(opsByServer[id] ?? DEFAULT_REMOTE_OPS)))
      case 'check_for_update':
        return Promise.resolve(null)
      default:
        return Promise.resolve(null)
    }
  })
}

type AppVm = {
  selectedServerId: string | null
  selectedCA: number | null
  selectedCategory: string | null
  openRuntimeParamsDrawer: () => void
}

let w: VueWrapper | null = null

function spCheckbox(root: VueWrapper, scope: string): HTMLInputElement {
  const label = root
    .findAll(`${scope} .rp-subgroup .rp-switch`)
    .find(l => l.text().includes('M_SP_NA_1'))
  if (!label) throw new Error(`sync-TB sp switch not found in ${scope}`)
  return label.find('input[type="checkbox"]').element as HTMLInputElement
}

// 右键第 idx 台服务器 → 点菜单里的「运行参数」
async function ctxEditRuntimeParams(root: VueWrapper, idx: number) {
  await root.findAll('.server-node')[idx].trigger('contextmenu')
  // 服务器菜单固定三项:启动/停止 · 运行参数 · 删除
  const items = root.findAll('.context-menu-item')
  expect(items).toHaveLength(3)
  await items[1].trigger('click')
  await flushPromises()
}

beforeEach(() => {
  opsByServer = {
    s1: JSON.parse(JSON.stringify(DEFAULT_REMOTE_OPS)),
    s2: { ...JSON.parse(JSON.stringify(DEFAULT_REMOTE_OPS)), sync_tb_by_category: { ...DEFAULT_REMOTE_OPS.sync_tb_by_category, sp: true } },
  }
  setupInvokeMock()
})
afterEach(() => {
  w?.unmount()
  w = null
})

async function mountApp(): Promise<VueWrapper> {
  w = mount(App, {
    global: {
      stubs: {
        Toolbar: true,
        ValuePanel: true,
        LogPanel: true,
        Splitter: true,
        AppDialog: true,
        UpdateDialog: true,
        ParseFrameDialog: true,
        teleport: true,
      },
    },
  })
  await flushPromises()
  return w
}

describe('右键「运行参数」与工具栏抽屉指向同一台服务器', () => {
  it('树上什么都没选时右键 s2:树选中对齐到 s2,抽屉不再是空态', async () => {
    const app = await mountApp()
    const vm = app.vm as unknown as AppVm
    expect(vm.selectedServerId).toBe(null) // 初始什么都没选

    await ctxEditRuntimeParams(app, 1) // 右键第二台服务器 s2

    expect(vm.selectedServerId).toBe('s2')
    expect(vm.selectedCA).toBe(null)
    expect(vm.selectedCategory).toBe(null)

    // 工具栏入口打开抽屉:必须是 s2 的参数,而不是"请先选服务器"空态
    vm.openRuntimeParamsDrawer()
    await flushPromises()
    expect(app.find('.rp-drawer .rp-empty').exists()).toBe(false)
    expect(spCheckbox(app, '.rp-drawer').checked).toBe(true) // s2 开了 sp
  })

  it('已选中 s1 后右键 s2:抽屉与徽标链路都切到 s2,不再停留在 s1', async () => {
    const app = await mountApp()
    const vm = app.vm as unknown as AppVm

    await app.findAll('.server-node')[0].trigger('click') // 先选 s1
    await flushPromises()
    expect(vm.selectedServerId).toBe('s1')

    await ctxEditRuntimeParams(app, 1)

    expect(vm.selectedServerId).toBe('s2')
    vm.openRuntimeParamsDrawer()
    await flushPromises()
    expect(spCheckbox(app, '.rp-drawer').checked).toBe(true) // s1 是 false
  })

  it('右键当前已选中的服务器:不清掉已选的站/分类', async () => {
    const app = await mountApp()
    const vm = app.vm as unknown as AppVm

    await app.findAll('.station-node')[0].trigger('click') // 选 s1 下的站
    await flushPromises()
    expect(vm.selectedServerId).toBe('s1')
    expect(vm.selectedCA).toBe(1)

    await ctxEditRuntimeParams(app, 0) // 右键 s1(已是选中的服务器)

    expect(vm.selectedServerId).toBe('s1')
    expect(vm.selectedCA).toBe(1) // 视图不该被无谓地重置
  })
})
