// fix-data-display-stability 验证项 4.4 / 4.2(主站计数部分):
// 切换分类只改变过滤结果,底层 dataMap 不丢数据;categoryCounts 实时派生。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, nextTick, type Ref } from 'vue'
import DataTable from '../src/components/DataTable.vue'
import { useI18n } from '@shared/i18n'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))

function pt(ioa: number, category: string, value: string, ca = 1, asduType = 'M_X') {
  return { ioa, asdu_type: asduType, asdu_type_id: 3, value, category, common_address: ca, quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false, timestamp: null }
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

describe('DataTable 分类筛选 (4.4 / 4.2)', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    vi.useFakeTimers() // 阻止 1s 轮询真正触发,保持确定性
    useI18n().setLocale('en-US')
  })

  it('切换分类只过滤,dataMap 不丢数据;来回切换可复原', async () => {
    const points = [pt(1, '单点', 'on'), pt(2, '单点', 'off'), pt(3, '浮点', '1.5')]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const provide = provideRefs()
    const wrapper = mount(DataTable, { global: { provide } })
    await flushPromises() // onMounted -> fetchData
    await nextTick()

    const vm = wrapper.vm as unknown as { filteredPoints: unknown[] }
    expect(vm.filteredPoints.length).toBe(3) // 无筛选:全部

    provide.selectedCategory.value = '单点'
    await nextTick()
    expect(vm.filteredPoints.length).toBe(2) // 仅单点

    provide.selectedCategory.value = null // 来回切换
    await nextTick()
    expect(vm.filteredPoints.length).toBe(3) // 数据未丢失,复原

    wrapper.unmount()
  })

  it('categoryCounts 按 conn->ca->category 实时派生', async () => {
    const points = [pt(1, '单点', 'on'), pt(2, '单点', 'off'), pt(3, '浮点', '1.5'), pt(1, '单点', 'on', 2)]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const provide = provideRefs()
    const wrapper = mount(DataTable, { global: { provide } })
    await flushPromises()
    await nextTick()

    const byCa = provide.categoryCounts.value.get('conn-1') as Map<number, Map<string, number>>
    expect(byCa.get(1)!.get('单点')).toBe(2)
    expect(byCa.get(1)!.get('浮点')).toBe(1)
    expect(byCa.get(2)!.get('单点')).toBe(1) // 不同 CA 独立计数
    wrapper.unmount()
  })

  it('按当前语言显示 DPI 0/3,并用协议值标记右键控制当前态', async () => {
    const points = [
      pt(1, 'double_point', '0', 1, 'M_DP_NA_1'),
      pt(2, 'double_point', '3', 1, 'M_DP_NA_1'),
    ]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const wrapper = mount(DataTable, { global: { provide: provideRefs() } })
    await flushPromises()
    await nextTick()

    expect(wrapper.findAll('.value-text').map(node => node.text())).toEqual([
      'Intermediate',
      'Indeterminate',
    ])
    expect(wrapper.findAll('thead .dp-help')).toHaveLength(1)
    expect(wrapper.findAll('tbody .dp-help')).toHaveLength(0)

    await wrapper.findAll('tbody tr')[0].trigger('contextmenu')
    expect(wrapper.find('.ctx-active').exists()).toBe(true)

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.findAll('.value-text').map(node => node.text())).toEqual([
      '中间',
      '不确定',
    ])
    wrapper.unmount()
  })

  it('不会把步位置的数值误判为双点控制当前态', async () => {
    const points = [pt(1, 'step_position', '1', 1, 'M_ST_NA_1')]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const wrapper = mount(DataTable, { global: { provide: provideRefs() } })
    await flushPromises()
    await nextTick()

    await wrapper.find('tbody tr').trigger('contextmenu')
    expect(wrapper.findAll('.ctx-active')).toHaveLength(0)
    wrapper.unmount()
  })

  it('统一当前视图计数、类型格式与无时标占位', async () => {
    const points = [
      pt(1, 'double_point', 'OFF', 1, 'M_DP_NA_1'),
      pt(2, 'double_point', 'ON', 1, 'M_DP_NA_1'),
      pt(3, 'single_point', 'OFF', 1, 'M_SP_NA_1'),
    ]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const provide = provideRefs()
    const wrapper = mount(DataTable, { global: { provide } })
    await flushPromises()
    await nextTick()

    expect(wrapper.find('.point-count').text()).toBe('3 points')
    provide.selectedCategory.value = 'double_point'
    await nextTick()
    expect(wrapper.find('.point-count').text()).toBe('2 points')

    await wrapper.find('.search-input').setValue('2')
    expect(wrapper.find('.point-count').text()).toBe('1 / 2 points')
    expect(wrapper.find('tbody .col-type').text()).toBe('M_DP_NA_1 (Type ID: 3)')
    expect(wrapper.find('tbody .col-timestamp').text()).toBe('-')
    wrapper.unmount()
  })

  it('默认隐藏复选框，右键多选后才显示并可退出', async () => {
    const points = [
      pt(1, 'double_point', 'OFF', 1, 'M_DP_NA_1'),
      pt(2, 'double_point', 'ON', 1, 'M_DP_NA_1'),
    ]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const wrapper = mount(DataTable, { global: { provide: provideRefs() } })
    await flushPromises()
    await nextTick()
    expect(wrapper.findAll('tbody input[type="checkbox"]')).toHaveLength(0)

    await wrapper.findAll('tbody tr')[0].trigger('contextmenu')
    const enter = wrapper.findAll('.ctx-item').find(node => node.text() === 'Multi-select')
    expect(enter).toBeDefined()
    await enter!.trigger('click')
    await nextTick()
    expect(wrapper.findAll('tbody input[type="checkbox"]')).toHaveLength(2)

    await wrapper.findAll('tbody input[type="checkbox"]')[1].trigger('click')
    expect(wrapper.find('.multi-select-count').text()).toBe('2 selected')
    await wrapper.find('.multi-select-btn.exit').trigger('click')
    expect(wrapper.findAll('tbody input[type="checkbox"]')).toHaveLength(0)
    expect(wrapper.emitted('point-select')?.at(-1)?.[0]).toEqual([])
    wrapper.unmount()
  })

  it('表头选择区分同 IOA 的 CA 和类型，并保留搜索范围外的选择', async () => {
    const points = [
      pt(1001, 'single_point', '0', 1, 'M_SP_NA_1'),
      pt(1001, 'single_point', '1', 2, 'M_SP_NA_1'),
      pt(1001, 'float_measured', '1.5', 1, 'M_ME_NC_1'),
      pt(2, 'single_point', '0', 1, 'M_SP_NA_1'),
    ]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })
    const wrapper = mount(DataTable, { global: { provide: provideRefs() } })
    await flushPromises()
    await wrapper.findAll('tbody tr').find(row => row.find('.col-ioa').text() === '2')!.trigger('contextmenu')
    await wrapper.findAll('.ctx-item').find(node => node.text() === 'Multi-select')!.trigger('click')
    await wrapper.find('.search-input').setValue('1001')
    const header = wrapper.find('thead input.select-all-checkbox')
    await header.setValue(true)
    expect(wrapper.emitted('point-select')?.at(-1)?.[0]).toHaveLength(4)
    await header.setValue(false)
    expect(wrapper.emitted('point-select')?.at(-1)?.[0]).toEqual([points[3]])
    await wrapper.find('.search-input').setValue('')
    expect((header.element as HTMLInputElement).indeterminate).toBe(true)
    wrapper.unmount()
  })

  it('右键多选后表头复选框显示半选状态并可全选或取消当前筛选', async () => {
    const points = [
      pt(1, 'double_point', 'OFF', 1, 'M_DP_NA_1'),
      pt(2, 'double_point', 'ON', 1, 'M_DP_NA_1'),
    ]
    invokeMock.mockResolvedValueOnce({ points, seq: 1 }).mockResolvedValue({ points: [], seq: 1 })

    const wrapper = mount(DataTable, { global: { provide: provideRefs() } })
    await flushPromises()
    await nextTick()
    await wrapper.findAll('tbody tr')[0].trigger('contextmenu')
    const enter = wrapper.findAll('.ctx-item').find(node => node.text() === 'Multi-select')
    await enter!.trigger('click')
    await nextTick()

    const headerCheckbox = wrapper.find('thead input.select-all-checkbox')
    expect(headerCheckbox.exists()).toBe(true)
    expect((headerCheckbox.element as HTMLInputElement).checked).toBe(false)
    expect((headerCheckbox.element as HTMLInputElement).indeterminate).toBe(true)

    await headerCheckbox.setValue(true)
    await nextTick()
    expect((headerCheckbox.element as HTMLInputElement).checked).toBe(true)
    expect((headerCheckbox.element as HTMLInputElement).indeterminate).toBe(false)
    expect(wrapper.find('.multi-select-count').text()).toBe('2 selected')

    await headerCheckbox.setValue(false)
    await nextTick()
    expect(wrapper.find('.multi-select-count').exists()).toBe(false)
    expect(wrapper.emitted('point-select')?.at(-1)?.[0]).toEqual([])
    wrapper.unmount()
  })
})
