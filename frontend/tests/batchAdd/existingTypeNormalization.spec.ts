// issue #28 存量 bug:批量添加的"同类型已有 IOA"过滤因类型名格式不一致而完全失效。
// 父组件(DataPointTable)传的是显示名(M_SP_NA_1),表单值是 PascalCase(MSpNa1),
// 直接 === 比较永远不相等 → summary card、冲突提示、下一可用 IOA 按钮全部不出现。
// 修复后经 findAsduTypeOption 归一化为 typeId 再比较,两种格式都能命中。
import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import BatchAddModal from '../../src/components/BatchAddModal.vue'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

function mountModal(existingPoints: Array<{ ioa: number; asdu_type: string }>) {
  return mount(BatchAddModal, {
    props: { visible: true, serverId: 's1', commonAddress: 1, existingPoints },
    global: {
      provide: { [dialogKey as symbol]: { showAlert: () => Promise.resolve() } },
      stubs: { teleport: true },
    },
  })
}

describe('BatchAddModal existingSameTypeIoas 类型归一化', () => {
  it('显示名格式(M_SP_NA_1)的已有点位能命中默认表单类型 MSpNa1', () => {
    const wrapper = mountModal([
      { ioa: 1, asdu_type: 'M_SP_NA_1' },
      { ioa: 2, asdu_type: 'M_SP_NA_1' },
      { ioa: 3, asdu_type: 'M_ME_NC_1' },
    ])
    const card = wrapper.find('.summary-card')
    expect(card.exists()).toBe(true)
    // 仅统计同类型两点(IOA 1–2),不含 M_ME_NC_1
    expect(wrapper.find('.summary-card__ranges-value').text()).toBe('1–2')
  })

  it('默认范围 0+10 与已有 IOA 重叠时出现冲突提示', () => {
    const wrapper = mountModal([
      { ioa: 1, asdu_type: 'M_SP_NA_1' },
      { ioa: 2, asdu_type: 'M_SP_NA_1' },
    ])
    expect(wrapper.find('.summary-card__conflict').exists()).toBe(true)
  })

  it('PascalCase 格式的已有点位同样命中(归一化对两种格式等效)', () => {
    const wrapper = mountModal([{ ioa: 7, asdu_type: 'MSpNa1' }])
    expect(wrapper.find('.summary-card').exists()).toBe(true)
    expect(wrapper.find('.summary-card__ranges-value').text()).toBe('7')
  })

  it('仅有其他类型点位时不出现 summary card', () => {
    const wrapper = mountModal([{ ioa: 3, asdu_type: 'M_ME_NC_1' }])
    expect(wrapper.find('.summary-card').exists()).toBe(false)
  })
})
