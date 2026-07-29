import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import ValuePanel from '../src/components/ValuePanel.vue'
import type { DataPointInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

beforeEach(() => {
  invokeMock.mockReset()
  useI18n().setLocale('en-US')
})

function detail(overrides: Partial<DataPointInfo> = {}): DataPointInfo {
  return {
    ioa: 10,
    asdu_type: 'C_SC_NA_1',
    category: 'single_command',
    name: 'trip',
    comment: 'bay 1',
    mapping_common_address: 2,
    mapping_ioa: 200,
    mapping_asdu_type: 'M_SP_TB_1',
    command_qualifier: 1,
    select_before_operate: true,
    value: '1',
    quality_ov: false,
    quality_bl: false,
    quality_sb: false,
    quality_nt: false,
    quality_iv: false,
    timestamp: null,
    ...overrides,
  }
}

async function mountDetail(point: DataPointInfo) {
  invokeMock.mockResolvedValue(point)
  wrapper = mount(ValuePanel, {
    global: {
      provide: {
        selectedServerId: ref<string | null>('server-1'),
        selectedCA: ref<number | null>(1),
        selectedPoints: ref([{
          ioa: point.ioa,
          asdu_type: point.asdu_type,
          category: point.category,
          value: point.value,
        }]),
        [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
      },
    },
  })
  await flushPromises()
  return wrapper
}

describe('Point Details command semantics', () => {
  it('shows Type IDs, full mapping, QOC meaning, and SBO in both languages', async () => {
    const panel = await mountDetail(detail())
    expect(panel.text()).toContain('C_SC_NA_1 (Type ID: 45)')
    expect(panel.text()).toContain('CA 2 · IOA 200 · M_SP_TB_1 (Type ID: 30)')
    expect(panel.text()).toContain('QOC / QU Qualifier')
    expect(panel.text()).toContain('1 — Short pulse')
    expect(panel.text()).toContain('Execution Mode (S/E)')
    expect(panel.text()).toContain('Select before operate (SBO)')

    useI18n().setLocale('zh-CN')
    await nextTick()
    expect(panel.text()).toContain('QOC / QU 限定词')
    expect(panel.text()).toContain('1 — 短脉冲')
    expect(panel.text()).toContain('执行模式 (S/E)')
    expect(panel.text()).toContain('选择后执行 (SBO)')
  })

  it('identifies setpoint QL values and direct execution', async () => {
    const panel = await mountDetail(detail({
      asdu_type: 'C_SE_NA_1',
      category: 'normalized_setpoint',
      command_qualifier: 5,
      select_before_operate: false,
    }))
    expect(panel.text()).toContain('C_SE_NA_1 (Type ID: 48)')
    expect(panel.text()).toContain('QL Qualifier')
    expect(panel.text()).toContain('5 — Reserved or application-defined')
    expect(panel.text()).toContain('Direct execute')
  })
})
