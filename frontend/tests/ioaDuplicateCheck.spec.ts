// issue #28:同 CASDU 内跨类型重复 IOA 校验(仅警示不阻断)。
// 规则:同方向(监视/控制)且不同分类共用 IOA 才提示;
// NA/TA/TB 同分类变体、控制↔监视配对(兼容自动映射)不算。
import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, flushPromises, VueWrapper } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import DataPointModal from '../src/components/DataPointModal.vue'
import BatchAddModal from '../src/components/BatchAddModal.vue'

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
    // 默认 range 模式 startIoa=0 count=10 → 覆盖 IOA 1(DP)与 2(SP 同类冲突)
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
