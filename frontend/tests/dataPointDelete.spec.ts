// 删除点位:单点 + 多选批量。验证作用于 selectedRows 而非仅右键行。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, nextTick, type Ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import DataPointTable from '../src/components/DataPointTable.vue'
import ConnectionTree from '../src/components/ConnectionTree.vue'
import type { DataPointInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))

function dp(ioa: number, asdu_type: string, category: string, value: string): DataPointInfo {
  return { ioa, asdu_type, category, name: `p${ioa}`, comment: '', value, quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false, timestamp: null }
}

interface Refs {
  selectedServerId: Ref<string | null>
  selectedCA: Ref<number | null>
  selectedCategory: Ref<string | null>
  dataRefreshKey: Ref<number>
  categoryCounts: Ref<Map<string, number>>
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
      provide: { ...refs, [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      stubs: { DataPointModal: true, BatchAddModal: true },
    },
  })
  return { wrapper, refs }
}

async function selectStation(refs: Refs) {
  refs.selectedServerId.value = 's1'
  refs.selectedCA.value = 1
  await flushPromises()
  await nextTick()
}

const A = dp(1, 'M_SP_NA_1', '单点 (SP)', 'on')
const B = dp(2, 'M_SP_NA_1', '单点 (SP)', '0')
const C = dp(3, 'M_ME_NC_1', '浮点 (ME_NC)', '1.5')

// 用可变后端模拟 batch_remove_data_points + list_data_points_since
function wireBackend(initial: DataPointInfo[]) {
  let backend = initial.slice()
  const seq = 99
  invokeMock.mockImplementation((cmd: string, args: any) => {
    if (cmd === 'list_data_points_since') {
      const points = args.sinceSeq === 0 ? backend.slice() : (args.sinceSeq < seq ? backend.slice() : [])
      return Promise.resolve({ points, seq, total_count: backend.length })
    }
    if (cmd === 'batch_remove_data_points') {
      const targets: Array<{ ioa: number; asdu_type: string }> = args.points
      backend = backend.filter(p => !targets.some(t => t.ioa === p.ioa && t.asdu_type === p.asdu_type))
      return Promise.resolve(targets.length)
    }
    if (cmd === 'list_servers') return Promise.resolve([{ id: 's1', bind_address: '127.0.0.1', port: 12404, state: 'Stopped', station_count: 1, client_count: 0 }])
    if (cmd === 'list_stations') return Promise.resolve([{
      common_address: 1, name: 'Test', point_count: backend.length,
      category_counts: { single_point: backend.filter(p => p.asdu_type === 'M_SP_NA_1').length, float_measured: backend.filter(p => p.asdu_type === 'M_ME_NC_1').length },
    }])
    if (cmd === 'list_point_mutations') return Promise.resolve([])
    return Promise.resolve()
  })
  return { lastCall: () => invokeMock.mock.calls.find(c => c[0] === 'batch_remove_data_points'), size: () => backend.length }
}

describe('DataPointTable 删除', () => {
  beforeEach(() => invokeMock.mockReset())

  it('右键单行删除该点', async () => {
    const be = wireBackend([A, B, C])
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { filteredPoints: DataPointInfo[] }
    expect(vm.filteredPoints.length).toBe(3)

    await wrapper.find('tbody tr').trigger('contextmenu')
    await nextTick()
    await wrapper.find('.context-menu-item.danger').trigger('click')
    await flushPromises()
    await nextTick()

    const call = be.lastCall()
    expect(call?.[1].points).toEqual([{ ioa: 1, asdu_type: 'M_SP_NA_1' }])
    expect(vm.filteredPoints.length).toBe(2)
    wrapper.unmount()
  })

  it('多选后右键批量删除全部选中', async () => {
    const be = wireBackend([A, B, C])
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      filteredPoints: DataPointInfo[]
      selectedRows: DataPointInfo[]
    }

    // 右键进入多选后勾选 A + B
    const rows = wrapper.findAll('tbody tr')
    await rows[0].trigger('contextmenu')
    const multiSelect = wrapper.findAll('.context-menu-item').find(node => node.text() === 'Multi-select')
    expect(multiSelect).toBeDefined()
    await multiSelect!.trigger('click')
    await nextTick()
    await wrapper.findAll('tbody input[type="checkbox"]')[1].trigger('click')
    await nextTick()
    expect(vm.selectedRows.length).toBe(2)

    // 右键已选中的行 -> 保留多选 -> 删除两点
    await rows[1].trigger('contextmenu')
    await nextTick()
    await wrapper.find('.context-menu-item.danger').trigger('click')
    await flushPromises()
    await nextTick()

    const call = be.lastCall()
    expect(call?.[1].points).toHaveLength(2)
    expect(be.size()).toBe(1)
    expect(vm.filteredPoints.length).toBe(1)
    expect(vm.filteredPoints[0].ioa).toBe(3)
    wrapper.unmount()
  })

  it('Delete 键删除选中行', async () => {
    const be = wireBackend([A, B, C])
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { filteredPoints: DataPointInfo[] }

    const rows = wrapper.findAll('tbody tr')
    await rows[2].trigger('click')
    await nextTick()
    await wrapper.find('.table-scroll-container').trigger('keydown', { key: 'Delete' })
    await flushPromises()
    await nextTick()

    expect(be.lastCall()?.[1].points).toEqual([{ ioa: 3, asdu_type: 'M_ME_NC_1' }])
    expect(vm.filteredPoints.length).toBe(2)
    wrapper.unmount()
  })

  it.each([1, 2])('deleting %i points refreshes the tree category and station totals', async (count) => {
    wireBackend([A, B, C])
    const { wrapper, refs } = mountTable()
    const tree = mount(ConnectionTree, {
      global: { provide: { ...refs, treeRefreshKey: ref(0), [dialogKey as symbol]: { showAlert: () => Promise.resolve() } } },
    })
    try {
      await selectStation(refs)
      const spNode = () => tree.findAll('.category-node')[0]!
      expect(spNode().find('.node-badge').text()).toBe('2')
      expect(tree.find('.station-node .node-badge').text()).toBe('3')
      const vm = wrapper.vm as unknown as { selectedRows: DataPointInfo[] }
      vm.selectedRows = [A, B].slice(0, count)
      await wrapper.find('.table-scroll-container').trigger('keydown', { key: 'Delete' })
      await flushPromises()
      expect(tree.find('.station-node .node-badge').text()).toBe(String(3 - count))
      if (count === 2) expect(spNode().find('.node-badge').exists()).toBe(false)
      else expect(spNode().find('.node-badge').text()).toBe('1')
      expect(tree.findAll('.category-node')[6]!.find('.node-badge').text()).toBe('1')
    } finally {
      tree.unmount()
      wrapper.unmount()
    }
  })
})
