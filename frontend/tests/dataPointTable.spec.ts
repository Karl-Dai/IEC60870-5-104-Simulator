// fix-slave-data-display 验证项 8.2 / 8.3 / 8.4:
// 8.2 变化的点位行高亮(changedKeys 仅含值变化的点)
// 8.3 categoryCounts 实时计算(按 category 派生)
// 8.4 分类切换数据不丢失(filteredPoints 过滤,dataMap 不动)
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, nextTick, type Ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
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
const CONTROL = dp(4, 'C_SC_NA_1', 'single_command', 'false')

describe('DataPointTable 子站数据表', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    useI18n().setLocale('en-US')
  })

  it('控制点分类解释其设计意图，监视分类不占用提示空间', async () => {
    invokeMock.mockResolvedValue({ points: [A, CONTROL], seq: 1, total_count: 2 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    // "全部数据点"中实际含控制点时也给出解释。
    expect(wrapper.find('.control-point-intent').text()).toContain('Why does the Slave show control points?')
    expect(wrapper.find('.control-direction').text()).toBe('MASTER → SLAVE')
    expect(wrapper.find('.control-point-intent').text()).toContain('not included in GI')

    refs.selectedCategory.value = 'single_point'
    await nextTick()
    expect(wrapper.find('.control-point-intent').exists()).toBe(false)

    refs.selectedCategory.value = 'single_command'
    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.find('.control-point-intent').text()).toContain('为什么子站显示控制点？')
    expect(wrapper.find('.control-point-intent').text()).toContain('不参与总召、周期发送或自发上送')
    wrapper.unmount()
  })

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

  it('双击非值单元格直接打开该点编辑对话框', async () => {
    invokeMock.mockResolvedValue({ points: [A, B], seq: 1, total_count: 2 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      selectedRows: DataPointInfo[]
      editingPointDefinition: DataPointInfo | null
      showEditModal: boolean
    }

    await wrapper.findAll('tbody tr')[1].find('.col-name').trigger('dblclick')

    expect(vm.selectedRows).toEqual([B])
    expect(vm.editingPointDefinition).toEqual(B)
    expect(vm.showEditModal).toBe(true)
    expect(wrapper.emitted('point-select')?.at(-1)?.[0]).toEqual([{
      ioa: B.ioa,
      asdu_type: B.asdu_type,
      category: B.category,
      value: B.value,
    }])
    wrapper.unmount()
  })

  it('双击 Value 单元格仍只启动内联改值', async () => {
    invokeMock.mockResolvedValue({ points: [A], seq: 1, total_count: 1 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      editingCell: { ioa: number; asduType: string } | null
      editingPointDefinition: DataPointInfo | null
      showEditModal: boolean
    }

    await wrapper.find('td.col-value').trigger('dblclick')

    expect(vm.editingCell).toEqual({ ioa: A.ioa, asduType: A.asdu_type })
    expect(wrapper.find('input.edit-input').exists()).toBe(true)
    expect(vm.editingPointDefinition).toBeNull()
    expect(vm.showEditModal).toBe(false)
    wrapper.unmount()
  })

  it('右键未选中行仍保留菜单与选中行语义', async () => {
    invokeMock.mockResolvedValue({ points: [A, B], seq: 1, total_count: 2 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as {
      selectedRows: DataPointInfo[]
      contextMenu: { show: boolean; x: number; y: number }
      editingPointDefinition: DataPointInfo | null
      showEditModal: boolean
    }

    await wrapper.findAll('tbody tr')[1].trigger('contextmenu', { clientX: 12, clientY: 34 })

    expect(vm.selectedRows).toEqual([B])
    expect(vm.contextMenu).toEqual({ show: true, x: 12, y: 34 })
    expect(wrapper.find('.context-menu').exists()).toBe(true)
    const editItem = wrapper.findAll('.context-menu-item').find(
      item => item.text() === useI18n().t('table.editPoint'),
    )
    expect(editItem).toBeDefined()
    await editItem!.trigger('click')
    expect(vm.contextMenu.show).toBe(false)
    expect(vm.editingPointDefinition).toEqual(B)
    expect(vm.showEditModal).toBe(true)
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

  // issue #28:改完运行参数保存后 +TB 徽标不出现 —— 徽标依赖 syncTbFlags,
  // 而它只在 selectedServerId 变化或 dataRefreshKey bump 时回读后端。
  // 保存运行参数的两个入口都必须 bump dataRefreshKey(App.onRuntimeParamsSaved),
  // 这里守住"bump 后徽标立刻按新配置重算"这一环。
  it('+TB 徽标随 dataRefreshKey 重算(保存 sync_tb_by_category 后立刻可见)', async () => {
    let syncSp = false
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: { sp: syncSp } })
      }
      return Promise.resolve({ points: [A, B], seq: 1, total_count: 2 })
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    expect(wrapper.findAll('.tb-badge')).toHaveLength(0) // 开关关着:无徽标

    syncSp = true // 用户在运行参数里开了 sp 的变位同步上送 TB 并保存
    refs.dataRefreshKey.value++
    await flushPromises()
    await nextTick()

    const badges = wrapper.findAll('.tb-badge')
    expect(badges).toHaveLength(2) // 两个 M_SP_NA_1 点位都补上派生徽标
    expect(badges[0].text()).toBe('+M_SP_TB_1 (30)')
    expect(badges[0].element.parentElement?.classList.contains('type-cell-content')).toBe(true)
    expect(badges[0].attributes('title')).toContain('M_SP_TB_1 (30)')
    expect(wrapper.find('.type-label').attributes('title')).toContain('M_SP_NA_1')
    expect(wrapper.find('.name-text').attributes('title')).toBe('p1')
    wrapper.unmount()
  })

  it('六个数据列可调整宽度，表头和虚拟表体同步且都执行最小宽度', async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: {} })
      }
      return Promise.resolve({ points: [A, B], seq: 1, total_count: 2 })
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    const handles = wrapper.findAll('.column-resizer')
    expect(handles.map(handle => handle.attributes('data-column')))
      .toEqual(['ioa', 'type', 'name', 'value', 'quality', 'timestamp'])

    const minimumWidths = ['64px', '160px', '120px', '90px', '90px', '110px']
    for (const handle of handles) {
      await handle.trigger('mousedown', { clientX: 1000 })
      window.dispatchEvent(new MouseEvent('mousemove', { clientX: 0 }))
      window.dispatchEvent(new MouseEvent('mouseup'))
      await nextTick()
    }

    const tables = wrapper.findAll('table.table')
    expect(tables).toHaveLength(2)
    for (const table of tables) {
      const widths = table.findAll('col').slice(1).map(col => col.element.style.width)
      expect(widths).toEqual(minimumWidths)
    }

    const vm = wrapper.vm as unknown as { sortKey: string }
    expect(vm.sortKey).toBe('ioa')
    await handles[1].trigger('mousedown', { clientX: 300 })
    window.dispatchEvent(new MouseEvent('mousemove', { clientX: 260 }))
    window.dispatchEvent(new MouseEvent('mouseup'))
    await wrapper.find('th.col-type').trigger('click')
    expect(vm.sortKey).toBe('ioa')

    // Keyboard resizing uses the same width state and a data refresh must not
    // rebuild that state or move the body columns out of alignment.
    await handles[2].trigger('keydown', { key: 'ArrowRight' })
    expect(tables[0].findAll('col')[3].element.style.width).toBe('132px')
    expect(tables[1].findAll('col')[3].element.style.width).toBe('132px')

    refs.dataRefreshKey.value++
    await flushPromises()
    await nextTick()
    expect(tables[0].findAll('col')[3].element.style.width).toBe('132px')
    expect(tables[1].findAll('col')[3].element.style.width).toBe('132px')
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

  it('DPI 0/3 按当前语言显示且 1/2 保持 OFF/ON', async () => {
    const points = [
      dp(10, 'M_DP_NA_1', 'double_point', '0'),
      dp(11, 'M_DP_NA_1', 'double_point', '1'),
      dp(12, 'M_DP_NA_1', 'double_point', '2'),
      dp(13, 'M_DP_NA_1', 'double_point', '3'),
    ]
    invokeMock.mockResolvedValue({ points, seq: 1, total_count: 4 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    expect(wrapper.findAll('.value-text').map((node) => node.text()))
      .toEqual(['Intermediate', 'OFF', 'ON', 'Indeterminate'])

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(wrapper.findAll('.value-text').map((node) => node.text()))
      .toEqual(['中间', 'OFF', 'ON', '不确定'])
    wrapper.unmount()
  })

  it('工具栏模拟入口使用当前选区打开统一设置抽屉', async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: {} })
      }
      if (command === 'list_point_mutations') {
        return Promise.resolve([{
          ioa: 3,
          asdu_type: 'M_ME_NC_1',
          mode: 'increment',
          period_ms: 500,
          step: 1,
          min: -100,
          max: 100,
        }])
      }
      return Promise.resolve({ points: [C], seq: 1, total_count: 1 })
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    await wrapper.find('tbody tr').trigger('click')
    await wrapper.find('.add-btn.simulation').trigger('click')
    await flushPromises()
    await nextTick()

    const drawer = document.body.querySelector('.sim-drawer')
    expect(drawer).not.toBeNull()
    expect(drawer?.textContent).toContain('Point Simulation Settings')
    expect(drawer?.textContent).toContain('1.5')

    refs.selectedServerId.value = 's2'
    refs.selectedCA.value = 2
    await flushPromises()
    await nextTick()
    expect(document.body.querySelector('.sim-drawer')).toBeNull()
    wrapper.unmount()
  })

  it('同一 IOA 的不同 Type ID 可通过首列复选框分别选择', async () => {
    const sameIoa = [
      dp(10, 'M_SP_NA_1', 'single_point', '0'),
      dp(10, 'M_SP_TB_1', 'single_point', '1'),
    ]
    invokeMock.mockResolvedValue({ points: sameIoa, seq: 1, total_count: 2 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { selectedRows: DataPointInfo[] }
    const checkboxes = wrapper.findAll('tbody input[type="checkbox"]')

    await checkboxes[0].trigger('click')
    await checkboxes[1].trigger('click')
    expect(vm.selectedRows.map(point => point.asdu_type))
      .toEqual(['M_SP_NA_1', 'M_SP_TB_1'])

    await checkboxes[0].trigger('click')
    expect(vm.selectedRows.map(point => point.asdu_type)).toEqual(['M_SP_TB_1'])
    wrapper.unmount()
  })

  it('全选、反选和清空只作用于当前筛选结果', async () => {
    const points = [
      dp(1, 'M_SP_NA_1', 'single_point', '0'),
      dp(2, 'M_SP_NA_1', 'single_point', '1'),
      dp(3, 'M_DP_NA_1', 'double_point', '2'),
    ]
    invokeMock.mockResolvedValue({ points, seq: 1, total_count: 3 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    refs.selectedCategory.value = 'single_point'
    await nextTick()
    const vm = wrapper.vm as unknown as { selectedRows: DataPointInfo[] }
    const actions = wrapper.findAll('.selection-btn')

    await actions[0].trigger('click')
    expect(vm.selectedRows.map(point => point.ioa)).toEqual([1, 2])

    await wrapper.findAll('tbody input[type="checkbox"]')[0].trigger('click')
    await actions[1].trigger('click')
    expect(vm.selectedRows.map(point => point.ioa)).toEqual([1])

    await actions[2].trigger('click')
    expect(vm.selectedRows).toEqual([])
    wrapper.unmount()
  })

  it('IOA、名称、类型和值列支持稳定的升降序排列', async () => {
    const points = [
      { ...dp(3, 'M_ME_NC_1', 'float_measured', '10'), name: 'same' },
      { ...dp(1, 'M_SP_NA_1', 'single_point', '2'), name: 'same' },
      { ...dp(2, 'M_DP_NA_1', 'double_point', '1'), name: 'alpha' },
    ]
    invokeMock.mockResolvedValue({ points, seq: 1, total_count: 3 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    const vm = wrapper.vm as unknown as { filteredPoints: DataPointInfo[] }
    const sortable = wrapper.findAll('th.sortable')

    expect(vm.filteredPoints.map(point => point.ioa)).toEqual([1, 2, 3])
    await sortable[0].trigger('click')
    expect(vm.filteredPoints.map(point => point.ioa)).toEqual([3, 2, 1])

    await sortable[2].trigger('click')
    expect(vm.filteredPoints.map(point => point.ioa)).toEqual([2, 1, 3])
    await sortable[2].trigger('click')
    expect(vm.filteredPoints.map(point => point.ioa)).toEqual([1, 3, 2])

    await sortable[3].trigger('click')
    expect(vm.filteredPoints.map(point => point.ioa)).toEqual([2, 1, 3])
    wrapper.unmount()
  })

  it('工具栏使用明确按钮名称，并从选中监视点打开批量设置', async () => {
    invokeMock.mockResolvedValue({ points: [A, B], seq: 1, total_count: 2 })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)

    expect(wrapper.findAll('.add-btn.batch:not(.simulation)').map(button => button.text()))
      .toEqual(['Add Batch Points', 'Set Values', 'Batch Settings'])

    await wrapper.findAll('tbody input[type="checkbox"]')[0].trigger('click')
    const settings = wrapper.find('.add-btn.settings')
    expect((settings.element as HTMLButtonElement).disabled).toBe(false)
    await settings.trigger('click')
    expect((wrapper.vm as unknown as { showBatchTypeModal: boolean }).showBatchTypeModal).toBe(true)
    wrapper.unmount()
  })

  it('编辑保存后按新 IOA/Type 重新选中并立即向详情面板发出刷新', async () => {
    let backend = [A]
    invokeMock.mockImplementation((command: string) => {
      if (command === 'list_data_points_since') {
        return Promise.resolve({ points: backend, seq: 1, total_count: backend.length })
      }
      if (command === 'get_remote_operation_config') {
        return Promise.resolve({ sync_tb_by_category: {} })
      }
      return Promise.resolve([])
    })
    const { wrapper, refs } = mountTable()
    await selectStation(refs)
    await wrapper.find('tbody input[type="checkbox"]').trigger('click')

    backend = [{ ...A, ioa: 20, asdu_type: 'M_SP_TB_1', name: 'migrated' }]
    const vm = wrapper.vm as unknown as {
      handlePointEdited: (target: { ioa: number; asdu_type: string }) => Promise<void>
      selectedRows: DataPointInfo[]
    }
    await vm.handlePointEdited({ ioa: 20, asdu_type: 'MSpTb1' })
    await flushPromises()

    expect(vm.selectedRows).toHaveLength(1)
    expect(vm.selectedRows[0]).toMatchObject({ ioa: 20, asdu_type: 'M_SP_TB_1' })
    const lastSelection = wrapper.emitted('point-select')?.at(-1)?.[0]
    expect(lastSelection).toEqual([{
      ioa: 20,
      asdu_type: 'M_SP_TB_1',
      category: '单点 (SP)',
      value: 'on',
    }])
    wrapper.unmount()
  })
})
