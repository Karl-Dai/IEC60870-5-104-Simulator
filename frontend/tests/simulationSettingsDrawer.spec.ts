import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import SimulationSettingsDrawer from '../src/components/SimulationSettingsDrawer.vue'
import type { DataPointInfo, PointMutationRow } from '../src/types'

const invokeMock = vi.fn()
const alertMock = vi.fn(() => Promise.resolve())
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

function point(ioa: number, asduType: string, value: string): DataPointInfo {
  return {
    ioa,
    asdu_type: asduType,
    category: 'float_measured',
    name: `P${ioa}`,
    comment: '',
    value,
    quality_ov: false,
    quality_bl: false,
    quality_sb: false,
    quality_nt: false,
    quality_iv: false,
    timestamp: null,
  }
}

function activeRow(overrides: Partial<PointMutationRow> = {}): PointMutationRow {
  return {
    ioa: 10,
    asdu_type: 'M_ME_NC_1',
    mode: 'increment',
    period_ms: 250,
    step: 2,
    min: -10,
    max: 10,
    value: '4',
    ...overrides,
  }
}

function mountDrawer(
  selectedPoints: DataPointInfo[],
  activeRows: PointMutationRow[],
) {
  return mount(SimulationSettingsDrawer, {
    props: {
      visible: true,
      serverId: 's1',
      commonAddress: 1,
      selectedPoints,
      activeRows,
    },
    global: {
      provide: {
        [dialogKey as symbol]: { showAlert: alertMock },
      },
      stubs: { Teleport: true },
    },
  })
}

describe('SimulationSettingsDrawer', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockResolvedValue(undefined)
    alertMock.mockClear()
    useI18n().setLocale('en-US')
  })

  it('回显活动模拟完整参数和当前值，并用原参数更新选中点', async () => {
    const selected = point(10, 'M_ME_NC_1', '4')
    const wrapper = mountDrawer([selected], [activeRow()])

    expect(wrapper.find('.sim-current-value').text()).toBe('4')
    expect(wrapper.text()).toContain('250 ms')
    expect(wrapper.text()).toContain('-10 / 10')

    const inputs = wrapper.findAll('.sim-form input')
    expect((inputs[0].element as HTMLInputElement).value).toBe('250')
    expect((inputs[1].element as HTMLInputElement).value).toBe('2')
    expect((inputs[2].element as HTMLInputElement).value).toBe('-10')
    expect((inputs[3].element as HTMLInputElement).value).toBe('10')

    await wrapper.find('.sim-btn-primary').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('start_point_mutation', {
      serverId: 's1',
      commonAddress: 1,
      ioa: 10,
      asduType: 'M_ME_NC_1',
      periodMs: 250,
      mode: 'increment',
      step: 2,
      min: -10,
      max: 10,
    })
    expect(wrapper.emitted('changed')).toHaveLength(1)
    wrapper.unmount()
  })

  it('混合活动参数给出提示，离散点始终按 flip 启动', async () => {
    const analog = point(10, 'M_ME_NC_1', '4')
    const discrete = {
      ...point(11, 'M_SP_NA_1', 'OFF'),
      category: 'single_point',
    }
    const wrapper = mountDrawer(
      [analog, discrete],
      [activeRow(), activeRow({ ioa: 11, asdu_type: 'M_SP_NA_1', mode: 'flip' })],
    )

    expect(wrapper.find('.sim-warning').exists()).toBe(true)
    await wrapper.findAll('.sim-mode-buttons button')[1].trigger('click')
    await wrapper.find('.sim-btn-primary').trigger('click')
    await flushPromises()

    const starts = invokeMock.mock.calls.filter(([command]) => command === 'start_point_mutation')
    expect(starts).toHaveLength(2)
    expect(starts[0][1]).toMatchObject({ ioa: 10, mode: 'increment' })
    expect(starts[1][1]).toMatchObject({ ioa: 11, mode: 'flip' })
    wrapper.unmount()
  })

  it('无选中点时仍显示活动任务，并可逐项停止', async () => {
    const wrapper = mountDrawer([], [activeRow({
      asdu_type: 'M_DP_NA_1',
      mode: 'flip',
      value: '3',
    })])

    expect(wrapper.text()).not.toContain('No active simulations for this station.')
    expect(wrapper.find('.sim-current-value').text()).toBe('Indeterminate')
    expect(wrapper.find('.sim-btn-primary').exists()).toBe(false)
    await wrapper.find('.sim-row-stop').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('stop_point_mutation', {
      serverId: 's1',
      commonAddress: 1,
      ioa: 10,
      asduType: 'M_DP_NA_1',
    })
    expect(wrapper.emitted('changed')).toHaveLength(1)
    wrapper.unmount()
  })

  it('随机模式按周期在 Min/Max 范围内生成且不要求 Step', async () => {
    const wrapper = mountDrawer([point(10, 'M_ME_NC_1', '4')], [])

    const modeButtons = wrapper.findAll('.sim-mode-buttons button')
    expect(modeButtons.map(button => button.text())).toContain('Random')
    await modeButtons[3].trigger('click')

    const labels = wrapper.findAll('.sim-form label').map(label => label.text())
    expect(labels.some(label => label.startsWith('Step'))).toBe(false)
    expect(labels.some(label => label.startsWith('Min'))).toBe(true)
    expect(labels.some(label => label.startsWith('Max'))).toBe(true)

    await wrapper.find('.sim-btn-primary').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('start_point_mutation', expect.objectContaining({
      mode: 'random',
      min: -96,
      max: 104,
    }))
    expect(alertMock).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
