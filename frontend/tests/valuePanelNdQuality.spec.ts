// add-m-me-nd-1 验证:M_ME_ND_1 无品质 —— ValuePanel 隐藏品质开关 + 类型清单含 ND。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import ValuePanel from '../src/components/ValuePanel.vue'
import { ASDU_TYPE_OPTIONS } from '../src/constants/asduTypes'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))

type Detail = {
  ioa: number; asdu_type: string; category: string; name: string; comment: string;
  value: string; timestamp: string | null;
  quality_ov: boolean; quality_bl: boolean; quality_sb: boolean; quality_nt: boolean; quality_iv: boolean;
}

function makeDetail(asdu_type: string): Detail {
  return {
    ioa: 5, asdu_type, category: 'NormalizedMeasured', name: 'p', comment: '',
    value: '0.5000', timestamp: null,
    quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false,
  }
}

async function mountSingle(detail: Detail) {
  invokeMock.mockResolvedValue(detail) // get_data_point 返回该点(单对象,非数组)
  const selectedPoints = ref([{ ioa: detail.ioa, asdu_type: detail.asdu_type, value: detail.value }])
  const w = mount(ValuePanel, {
    global: {
      provide: {
        selectedServerId: ref<string | null>('s1'),
        selectedCA: ref<number | null>(1),
        selectedPoints,
        [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
      },
    },
  })
  await flushPromises()
  return w
}

describe('M_ME_ND_1 无品质', () => {
  beforeEach(() => invokeMock.mockReset())

  it('类型清单含 M_ME_ND_1 (TypeID 21)', () => {
    const nd = ASDU_TYPE_OPTIONS.find((o) => o.value === 'MMeNd1')
    expect(nd).toBeDefined()
    expect(nd!.typeId).toBe(21)
  })

  it('选中 ND 点:隐藏全部品质开关,显示无品质占位', async () => {
    const w = await mountSingle(makeDetail('M_ME_ND_1'))
    expect(w.find('.quality-na').exists()).toBe(true)
    expect(w.find('.quality-na').text()).toContain('N/A')
    expect(w.findAll('.q-badge').length).toBe(0)
  })

  it('选中普通测量点(M_ME_NC_1):正常展示品质徽章,无占位', async () => {
    const w = await mountSingle(makeDetail('M_ME_NC_1'))
    expect(w.find('.quality-na').exists()).toBe(false)
    expect(w.findAll('.q-badge').length).toBeGreaterThan(0)
  })
})
