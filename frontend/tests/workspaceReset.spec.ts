// issue #64: a configuration file is a complete slave-workspace snapshot.
// Repeated loads must invalidate every view instance and retain no selection
// that belonged to the previous workspace.
import { describe, expect, it, vi } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import App from '../src/App.vue'

vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(null)) }))

interface AppVm {
  selectedServerId: string | null
  selectedServerState: string
  selectedCA: number | null
  selectedStationName: string
  selectedCategory: string | null
  selectedPoints: unknown[]
  workspaceEpoch: number
  runtimeParamsModalVisible: boolean
  runtimeParamsDrawerVisible: boolean
  openRuntimeParamsModal: (serverId: string, label: string) => void
  openRuntimeParamsDrawer: () => void
  resetWorkspaceView: () => void
}

describe('slave workspace view reset', () => {
  it('clears old selection/editors and advances the epoch on every full load', () => {
    const wrapper = shallowMount(App)
    const vm = wrapper.vm as unknown as AppVm
    vm.selectedServerId = 'server-A'
    vm.selectedServerState = 'Running'
    vm.selectedCA = 7
    vm.selectedStationName = 'station A'
    vm.selectedCategory = 'single_point'
    vm.selectedPoints = [{ ioa: 1 }]
    vm.openRuntimeParamsModal('server-A', 'server A')
    vm.openRuntimeParamsDrawer()
    const initialEpoch = vm.workspaceEpoch

    vm.resetWorkspaceView()

    expect(vm.selectedServerId).toBeNull()
    expect(vm.selectedServerState).toBe('Stopped')
    expect(vm.selectedCA).toBeNull()
    expect(vm.selectedStationName).toBe('')
    expect(vm.selectedCategory).toBeNull()
    expect(vm.selectedPoints).toEqual([])
    expect(vm.runtimeParamsModalVisible).toBe(false)
    expect(vm.runtimeParamsDrawerVisible).toBe(false)
    expect(vm.workspaceEpoch).toBe(initialEpoch + 1)

    vm.resetWorkspaceView()
    expect(vm.workspaceEpoch).toBe(initialEpoch + 2)
    wrapper.unmount()
  })
})
