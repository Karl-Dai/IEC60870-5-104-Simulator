// fix-slave-data-display 验证项 8.2 / 8.3 / 8.4:
// 8.2 变化的点位行高亮(changedKeys 仅含值变化的点)
// 8.3 categoryCounts 实时计算(按 category 派生)
// 8.4 分类切换数据不丢失(filteredPoints 过滤,dataMap 不动)
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, nextTick, type Ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import DataPointTable from '../src/components/DataPointTable.vue'
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
      provide: {
        ...refs,
        [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
      },
      stubs: { DataPointModal: true, BatchAddModal: true },
    },
  })
  return { wrapper, refs }
}

// 选定站 + CA 触发首批加载
async function selectStation(refs: Refs) {
  refs.selectedServerId.value = 's1'
  refs.selectedCA.value = 1
  await flushPromises()
  await nextTick()
}

const A = dp(1, 'M_SP_NA_1', '单点 (SP)', 'on')
const B = dp(2, 'M_SP_NA_1', '单点 (SP)', '0')
const C = dp(3, 'M_ME_NC_1', '浮点 (ME_NC)', '1.5')

describe('DataPointTable 子站数据表', () => {
  beforeEach(() => invokeMock.mockReset())

  it('8.3 categoryCounts 按分类实时派生', async () => {
    invokeMock.mockResolvedValue({ points: [A, B, C], seq: 1, total_count: 3 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    expect(refs.categoryCounts.value.get('单点 (SP)')).toBe(2)
    expect(refs.categoryCounts.value.get('浮点 (ME_NC)')).toBe(1)
    wrapper.unmount()
  })

  it('8.4 切换分类只过滤,dataMap 不丢数据,可复原', async () => {
    invokeMock.mockResolvedValue({ points: [A, B, C], seq: 1, total_count: 3 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    const vm = wrapper.vm as unknown as { filteredPoints: unknown[] }
    expect(vm.filteredPoints.length).toBe(3) // 全部

    refs.selectedCategory.value = '单点 (SP)' // 按 M_SP_ 前缀过滤
    await nextTick()
    expect(vm.filteredPoints.length).toBe(2)

    refs.selectedCategory.value = null // 切回
    await nextTick()
    expect(vm.filteredPoints.length).toBe(3) // 未丢失
    wrapper.unmount()
  })

  it('首批加载不逐点高亮(避免 N 个 setTimeout 定时器风暴)', async () => {
    // 切站后 dataMap 为空,首批返回的全部点都是"新点"。这些不是值变化,
    // 不应触发高亮——否则 2000 点/类型时会瞬间挂起数千个 3s setTimeout。
    invokeMock.mockResolvedValue({ points: [A, B, C], seq: 1, total_count: 3 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    const vm = wrapper.vm as unknown as { changedKeys: Set<string>; displayPoints: unknown[] }
    expect(vm.displayPoints.length).toBe(3) // 数据已加载
    expect(vm.changedKeys.size).toBe(0) // 但无一被标记高亮
    wrapper.unmount()
  })

  it('8.2 仅值变化的点位被标记高亮', async () => {
    // 首批:全部新点(首批不闪,见上一用例)
    invokeMock.mockResolvedValueOnce({ points: [A, B, C], seq: 1, total_count: 3 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    const vm = wrapper.vm as unknown as { changedKeys: Set<string> }
    vm.changedKeys.clear() // 与增量隔离(首批本就不闪)

    // 增量:仅 B 的值从 '0' -> '1'
    const Bchanged = dp(2, 'M_SP_NA_1', '单点 (SP)', '1')
    invokeMock.mockResolvedValueOnce({ points: [Bchanged], seq: 2, total_count: 3 })
    refs.dataRefreshKey.value++ // 触发再次加载
    await flushPromises()
    await nextTick()

    expect(vm.changedKeys.has('2:M_SP_NA_1')).toBe(true)  // B 高亮
    expect(vm.changedKeys.has('1:M_SP_NA_1')).toBe(false) // A 未闪
    expect(vm.changedKeys.has('3:M_ME_NC_1')).toBe(false) // C 未闪
    expect(vm.changedKeys.size).toBe(1)
    wrapper.unmount()
  })

  it('品质列渲染多位徽章(NT 高亮 / 正常显示 OK)', async () => {
    const ntPoint: DataPointInfo = { ...dp(1, 'M_SP_NA_1', '单点 (SP)', 'off'), quality_nt: true }
    const goodPoint = dp(2, 'M_SP_NA_1', '单点 (SP)', 'off')
    invokeMock.mockResolvedValue({ points: [ntPoint, goodPoint], seq: 1, total_count: 2 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    // NT 点:表格行内出现高亮 NT 徽章
    const litLetters = wrapper.findAll('.q-badge.lit').map((b) => b.text())
    expect(litLetters).toEqual(['NT'])
    // 正常点:紧凑模式显示 OK
    expect(wrapper.find('.q-ok').exists()).toBe(true)
    wrapper.unmount()
  })
})
