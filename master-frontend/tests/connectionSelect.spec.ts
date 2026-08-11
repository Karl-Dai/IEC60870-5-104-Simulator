// fix-data-display-stability 验证项 4.5:
// 点击连接节点(handleConnectionSelect)仅在连接 ID 真正变化时才清空
// selectedCategory;点击当前已选连接不应重置分类筛选。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import App from '../src/App.vue'

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])) }))

interface AppVm {
  selectedCategory: string | null
  selectedConnectionId: string | null
  selectedConnectionState: string
  selectedCA: number | null
  selectedPoints: unknown[]
  workspaceEpoch: number
  handleConnectionSelect: (id: string, state: string) => void
  handleCategorySelect: (connId: string, category: string, ca: number | null) => void
  handlePointSelect: (points: unknown[]) => void
  resetWorkspaceView: () => void
}

describe('App.handleConnectionSelect 分类筛选稳定 (4.5)', () => {
  let vm: AppVm
  beforeEach(() => {
    const wrapper = shallowMount(App)
    vm = wrapper.vm as unknown as AppVm
  })

  it('点击当前已选连接节点不清空 selectedCategory', () => {
    vm.handleCategorySelect('conn-A', '单点', 1)
    expect(vm.selectedCategory).toBe('单点')

    vm.handleConnectionSelect('conn-A', 'Connected') // 同一连接,changed=false
    expect(vm.selectedCategory).toBe('单点') // 未被清空
    expect(vm.selectedConnectionId).toBe('conn-A')
  })

  it('切换到不同连接才清空 selectedCategory', () => {
    vm.handleCategorySelect('conn-A', '单点', 1)
    vm.handleConnectionSelect('conn-B', 'Connected') // 不同连接,changed=true
    expect(vm.selectedCategory).toBeNull()
    expect(vm.selectedCA).toBeNull()
  })

  it('全量加载后清空旧选择并为每次加载切换工作区世代', () => {
    vm.handleCategorySelect('conn-A', 'single_point', 1)
    vm.handleConnectionSelect('conn-A', 'Connected')
    vm.handlePointSelect([{ ioa: 1 }])
    const initialEpoch = vm.workspaceEpoch

    vm.resetWorkspaceView()

    expect(vm.selectedConnectionId).toBeNull()
    expect(vm.selectedConnectionState).toBe('Disconnected')
    expect(vm.selectedCA).toBeNull()
    expect(vm.selectedCategory).toBeNull()
    expect(vm.selectedPoints).toEqual([])
    expect(vm.workspaceEpoch).toBe(initialEpoch + 1)

    // Loading the same full snapshot again still gets a fresh component tree;
    // no selection/cache from the first load is eligible for reuse.
    vm.resetWorkspaceView()
    expect(vm.workspaceEpoch).toBe(initialEpoch + 2)
  })
})
