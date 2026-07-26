// issue #28:同 CASDU 内跨类型重复 IOA 校验(仅警示不阻断)。
// 规则:同方向(监视/控制)且不同分类共用 IOA 才提示;
// NA/TA/TB 同分类变体、控制↔监视配对(兼容自动映射)不算。
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, flushPromises, VueWrapper } from '@vue/test-utils'
import { ref, nextTick, type Ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import DataPointModal from '../src/components/DataPointModal.vue'
import BatchAddModal from '../src/components/BatchAddModal.vue'
import DataPointTable from '../src/components/DataPointTable.vue'
// 原样文本:用于断言选中态的冲突样式覆盖存在(jsdom 不套用 scoped CSS)。
import dataPointTableSrc from '../src/components/DataPointTable.vue?raw'

const invokeMock = vi.fn().mockResolvedValue([])
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))

// 本站已有点位:DP(1)、SP(2)、SP-TB(3)、C_SC(4)
const existing = [
  { ioa: 1, asdu_type: 'M_DP_NA_1', category: 'double_point' },
  { ioa: 2, asdu_type: 'M_SP_NA_1', category: 'single_point' },
  { ioa: 3, asdu_type: 'M_SP_TB_1', category: 'single_point' },
  { ioa: 4, asdu_type: 'C_SC_NA_1', category: 'single_command' },
]

let w: VueWrapper | null = null
afterEach(() => { w?.unmount(); w = null })

function mountPointModal(): VueWrapper {
  w = mount(DataPointModal, {
    props: { visible: true, serverId: 's1', commonAddress: 1, existingPoints: existing },
    global: {
      stubs: { teleport: true },
      provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
    },
  })
  return w
}

const warnEl = (ww: VueWrapper) => ww.find('.form-hint--warn')

async function setIoaAndType(ww: VueWrapper, ioa: number, type: string) {
  await ww.find('select.form-select').setValue(type)
  await ww.find('input[type="number"]').setValue(ioa)
  await flushPromises()
}

describe('DataPointModal 跨类型重复 IOA 警告', () => {
  it('监视类型撞监视类型(SP 撞 DP):显示警告且不禁用确认', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 1, 'MSpNa1')
    expect(warnEl(ww).exists()).toBe(true)
    expect(warnEl(ww).text()).toContain('M_DP_NA_1')
    const confirm = ww.find('.btn-primary').element as HTMLButtonElement
    expect(confirm.disabled).toBe(false)
  })

  it('NA/TB 同分类变体共用 IOA(SP-NA 撞 SP-TB):不警告', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 3, 'MSpNa1')
    expect(warnEl(ww).exists()).toBe(false)
  })

  it('控制点撞监视点(C_DC 撞 M_DP):跨方向合法配对,不警告', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 1, 'CDcNa1')
    expect(warnEl(ww).exists()).toBe(false)
  })

  it('控制点撞控制点(C_SE_NA 撞 C_SC):警告', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 4, 'CSeNa1')
    expect(warnEl(ww).exists()).toBe(true)
    expect(warnEl(ww).text()).toContain('C_SC_NA_1')
  })

  it('无冲突 IOA:不警告', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 100, 'MSpNa1')
    expect(warnEl(ww).exists()).toBe(false)
  })
})

describe('BatchAddModal 跨类型重复 IOA 警告', () => {
  function mountBatch(): VueWrapper {
    w = mount(BatchAddModal, {
      props: { visible: true, serverId: 's1', commonAddress: 1, existingPoints: existing },
      global: {
        stubs: { teleport: true },
        provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      },
    })
    return w
  }

  it('SP 批量 0-9 覆盖 DP(1):警示但确认可用;同类冲突提示照常', async () => {
    const ww = mountBatch()
    await ww.find('select.form-select').setValue('MSpNa1')
    await flushPromises()
    // 起始 IOA 默认已自动避让(F3),这里手动改回 0 → 0-9 覆盖 IOA 1(DP 跨类型)与 2(SP 同类)
    await ww.findAll('.form-row input')[0].setValue(0)
    await flushPromises()
    const warn = ww.find('.summary-card__conflict--warn')
    expect(warn.exists()).toBe(true)
    expect(warn.text()).toContain('1')
    const confirm = ww.find('.btn-primary').element as HTMLButtonElement
    expect(confirm.disabled).toBe(false)
  })

  it('表达式模式命中跨类型 IOA:警示', async () => {
    const ww = mountBatch()
    await ww.find('select.form-select').setValue('MSpNa1')
    await ww.findAll('.mode-toggle button')[1].trigger('click')
    await ww.find('input.form-input[type="text"]').setValue('1, 100')
    await flushPromises()
    const warn = ww.find('.summary-card__conflict--warn')
    expect(warn.exists()).toBe(true)
    expect(warn.text()).toContain('1')
  })

  it('不命中跨类型 IOA:无警示', async () => {
    const ww = mountBatch()
    await ww.find('select.form-select').setValue('MSpNa1')
    await ww.findAll('.mode-toggle button')[1].trigger('click')
    await ww.find('input.form-input[type="text"]').setValue('100-110')
    await flushPromises()
    expect(ww.find('.summary-card__conflict--warn').exists()).toBe(false)
  })
})

// ---------------------------------------------------------------------------
// F1:同 (IOA, 类型) 重复 = 拒绝(后端 add_point_strict 会报错,不再静默覆盖)
// ---------------------------------------------------------------------------
describe('DataPointModal 同类型重复 IOA 拒绝', () => {
  const errEl = (ww: VueWrapper) => ww.find('.form-hint--error')

  it('同 (IOA, 类型) 已存在:红色错误提示 + 确认按钮禁用', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 2, 'MSpNa1') // 已有 M_SP_NA_1 @ IOA 2
    expect(errEl(ww).exists()).toBe(true)
    const confirm = ww.find('.btn-primary').element as HTMLButtonElement
    expect(confirm.disabled).toBe(true)
    // 拒绝优先:同类型报错时不再叠加跨类型的橙色警示
    expect(warnEl(ww).exists()).toBe(false)
  })

  it('同分类不同类型(SP-NA 撞 SP-TB):不算重复,可保存', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 3, 'MSpNa1') // IOA 3 上只有 M_SP_TB_1
    expect(errEl(ww).exists()).toBe(false)
    expect((ww.find('.btn-primary').element as HTMLButtonElement).disabled).toBe(false)
  })

  it('跨类型重复(SP 撞 DP)只警示,不禁用确认', async () => {
    const ww = mountPointModal()
    await setIoaAndType(ww, 1, 'MSpNa1')
    expect(errEl(ww).exists()).toBe(false)
    expect(warnEl(ww).exists()).toBe(true)
    expect((ww.find('.btn-primary').element as HTMLButtonElement).disabled).toBe(false)
  })

  it('编辑模式未改址:自身不算重复', async () => {
    w = mount(DataPointModal, {
      props: {
        visible: true,
        serverId: 's1',
        commonAddress: 1,
        existingPoints: existing,
        point: { ioa: 2, asdu_type: 'M_SP_NA_1', category: 'single_point', name: 'p2', comment: '', value: '0', quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false, timestamp: null },
      },
      global: {
        stubs: { teleport: true },
        provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      },
    })
    await flushPromises()
    expect(errEl(w).exists()).toBe(false)
    expect((w.find('.btn-primary').element as HTMLButtonElement).disabled).toBe(false)
    // 改址到另一个已存在的同类型 IOA(此处无第二个 SP,构造:改到 2 以外不存在的地址)
    await w.find('input[type="number"]').setValue(99)
    await flushPromises()
    expect(errEl(w).exists()).toBe(false)
  })
})

// ---------------------------------------------------------------------------
// F3:批量添加默认起始 IOA 跨分类避让 —— README 快速开始不再自己触发警示
// ---------------------------------------------------------------------------
describe('BatchAddModal 默认起始 IOA 跨分类避让', () => {
  // README 快速开始:新服务器空点表 → 依次批量添加覆盖全部 8 种监视类型。
  const MONITOR_CATEGORIES = [
    'single_point', 'double_point', 'step_position', 'bitstring',
    'normalized_measured', 'scaled_measured', 'float_measured', 'integrated_totals',
  ]

  it('依次为 8 个监视分类批量添加:全程无跨类型重复警示,地址段互不重叠', async () => {
    const accumulated: Array<{ ioa: number; asdu_type: string; category: string }> = []
    const usedRanges: Array<[number, number, string]> = []

    for (const category of MONITOR_CATEGORIES) {
      const ww = mount(BatchAddModal, {
        props: {
          visible: true,
          serverId: 's1',
          commonAddress: 1,
          category,
          existingPoints: [...accumulated],
        },
        global: {
          stubs: { teleport: true },
          provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
        },
      })
      await flushPromises()
      const vm = ww.vm as unknown as { startIoa: number; count: number; formAsduType: string }
      const start = vm.startIoa
      const count = vm.count
      // 新功能不能把用户导进它自己的警告状态
      expect(ww.find('.summary-card__conflict--warn').exists()).toBe(false)
      expect(ww.find('.summary-card__conflict').exists()).toBe(false)
      for (const [lo, hi, cat] of usedRanges) {
        expect(
          start > hi || start + count - 1 < lo,
          `${category} 的默认段 ${start}-${start + count - 1} 与 ${cat} 的 ${lo}-${hi} 重叠`,
        ).toBe(true)
      }
      usedRanges.push([start, start + count - 1, category])
      for (let i = 0; i < count; i++) {
        accumulated.push({ ioa: start + i, asdu_type: vm.formAsduType, category })
      }
      ww.unmount()
    }
    // 8 个分类 × 10 点,全部落在互不重叠的连续段上
    expect(accumulated.length).toBe(80)
    expect(usedRanges[0][0]).toBe(0)
    expect(usedRanges[7][0]).toBe(70)
  })

  it('手动改过起始 IOA 后不再被自动避让覆盖', async () => {
    w = mount(BatchAddModal, {
      props: { visible: true, serverId: 's1', commonAddress: 1, existingPoints: existing },
      global: {
        stubs: { teleport: true },
        provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      },
    })
    await flushPromises()
    await w.findAll('.form-row input')[0].setValue(500)
    await w.find('select.form-select').setValue('MDpNa1') // 换类型
    await flushPromises()
    expect((w.vm as unknown as { startIoa: number }).startIoa).toBe(500)
  })

  // 创建后弹窗不自动关闭:父组件刷新点表 → 未手动改过时顺势推到下一段空闲地址,
  // 否则会停在刚创建的那一段上(整段冲突,再点创建全部被跳过)。
  it('点表刷新后(创建完成)自动推进到下一段空闲地址', async () => {
    w = mount(BatchAddModal, {
      props: { visible: true, serverId: 's1', commonAddress: 1, existingPoints: [] },
      global: {
        stubs: { teleport: true },
        provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      },
    })
    await flushPromises()
    expect((w.vm as unknown as { startIoa: number }).startIoa).toBe(0)
    // 模拟刚创建的 0–9 被父组件刷新进来
    await w.setProps({
      existingPoints: Array.from({ length: 10 }, (_, i) => ({
        ioa: i, asdu_type: 'M_SP_NA_1', category: 'single_point',
      })),
    })
    await flushPromises()
    expect((w.vm as unknown as { startIoa: number }).startIoa).toBe(10)
    expect(w.find('.summary-card__conflict').exists()).toBe(false)
  })

  it('空点表:默认仍从 0 开始(无可避让地址)', async () => {
    w = mount(BatchAddModal, {
      props: { visible: true, serverId: 's1', commonAddress: 1, existingPoints: [] },
      global: {
        stubs: { teleport: true },
        provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      },
    })
    await flushPromises()
    expect((w.vm as unknown as { startIoa: number }).startIoa).toBe(0)
  })
})

// ---------------------------------------------------------------------------
// F2:数据表冲突标红在「选中行」下必须依然可辨
// (原实现 `tr.selected .col-ioa` 特异性 0,3,2 压过 `.col-ioa.ioa-dup` 0,2,0,
//  点中冲突行红色就消失,而"点中行→改 IOA"正是修冲突的操作路径)
// ---------------------------------------------------------------------------
describe('DataPointTable 冲突行标记(含选中态)', () => {
  function dp(ioa: number, asdu_type: string, category: string) {
    return { ioa, asdu_type, category, name: `p${ioa}`, comment: '', value: '0', quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false, timestamp: null }
  }

  async function mountTable(): Promise<VueWrapper> {
    const refs = {
      selectedServerId: ref<string | null>(null),
      selectedCA: ref<number | null>(null),
      selectedCategory: ref<string | null>(null),
      dataRefreshKey: ref(0),
      categoryCounts: ref(new Map<string, number>()),
    }
    // 同一 IOA 1 上的 SP 与 DP:同方向、不同分类 → 冲突
    invokeMock.mockResolvedValue({
      points: [dp(1, 'M_SP_NA_1', 'single_point'), dp(1, 'M_DP_NA_1', 'double_point')],
      seq: 1,
      total_count: 2,
    })
    w = mount(DataPointTable, {
      global: {
        provide: { ...refs, [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
        stubs: { DataPointModal: true, BatchAddModal: true, BatchWriteModal: true, BatchControlOptionsModal: true },
      },
    })
    refs.selectedServerId.value = 's1'
    refs.selectedCA.value = 1
    await flushPromises()
    await nextTick()
    return w
  }

  afterEach(() => { invokeMock.mockReset(); invokeMock.mockResolvedValue([]) })

  it('冲突行的 IOA 单元格带 ioa-dup 类与 ⚠ 徽标;选中后标记依然保留', async () => {
    const ww = await mountTable()
    expect(ww.findAll('td.col-ioa.ioa-dup').length).toBe(2)
    expect(ww.findAll('.dup-ioa-badge').length).toBe(2)

    // 点中冲突行(修冲突的入口操作:点中行 → 编辑 → 改 IOA)
    await ww.findAll('tbody tr')[0].trigger('click')
    await nextTick()
    const selected = ww.findAll('tbody tr.selected')
    // 注:isSelected() 只比 IOA,同 IOA 的两行会一起进选中态(既有行为,见报告)
    expect(selected.length).toBeGreaterThan(0)
    for (const row of selected) {
      const cell = row.find('td.col-ioa')
      expect(cell.classes()).toContain('ioa-dup')              // 类未被清掉
      expect(cell.find('.dup-ioa-badge').exists()).toBe(true)  // ⚠ 仍在
    }
  })

  it('选中态存在特异性更高的冲突样式覆盖(防特异性回归)', () => {
    // jsdom 不套用 scoped CSS,读不到计算色值:直接守住样式表里的覆盖规则本身。
    // 真实浏览器下的颜色由 Playwright 实测(见报告)。
    expect(dataPointTableSrc).toMatch(/tr\.selected\s+\.col-ioa\.ioa-dup\s*\{/)
  })
})
