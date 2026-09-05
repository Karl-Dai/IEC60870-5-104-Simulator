import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { useI18n } from '@shared/i18n'
import SimulationPacingSettings from '../src/components/SimulationPacingSettings.vue'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
const config = (batch_size = 2000, delay_ms = 50) => ({ random_pacing: { batch_size, delay_ms } })
const mountSettings = () => mount(SimulationPacingSettings, { props: { serverId: 's1', visible: true } })

describe('simulation upload pacing settings', () => {
  beforeEach(() => {
    useI18n().setLocale('en-US')
    invoke.mockReset()
    invoke.mockResolvedValue(config())
  })

  it('loads persisted values, saves only pacing, and reloads on reopen', async () => {
    const wrapper = mountSettings()
    await flushPromises()
    expect((wrapper.get('.pacing-batch').element as HTMLInputElement).value).toBe('2000')
    await wrapper.get('.pacing-batch').setValue(150)
    await wrapper.get('.pacing-delay').setValue(0)
    invoke.mockResolvedValueOnce({ batch_size: 150, delay_ms: 0 })
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(invoke).toHaveBeenLastCalledWith('set_simulation_pacing', {
      serverId: 's1', pacing: { batch_size: 150, delay_ms: 0 },
    })
    expect(wrapper.get('[role="status"]').text()).toContain('Saved')
    await wrapper.setProps({ visible: false })
    invoke.mockResolvedValueOnce(config(150, 0))
    await wrapper.setProps({ visible: true })
    await flushPromises()
    expect((wrapper.get('.pacing-delay').element as HTMLInputElement).value).toBe('0')
    wrapper.unmount()
  })

  it.each([['', 50], [0, 50], [1.5, 50], [100001, 50], [1, -1], [1, 60001], [1, '']])(
    'rejects invalid count=%s delay=%s', async (batch, delay) => {
      const wrapper = mountSettings()
      await flushPromises()
      await wrapper.get('.pacing-batch').setValue(batch)
      await wrapper.get('.pacing-delay').setValue(delay)
      expect(wrapper.get('.pacing-save').attributes('disabled')).toBeDefined()
      await wrapper.get('form').trigger('submit')
      expect(invoke).toHaveBeenCalledTimes(1)
      wrapper.unmount()
    },
  )

  it('ignores stale load and save responses after selecting a different server', async () => {
    let finishLoad!: (v: unknown) => void
    invoke.mockImplementationOnce(() => new Promise(resolve => { finishLoad = resolve }))
    const wrapper = mountSettings()
    invoke.mockResolvedValueOnce(config(20, 10))
    await wrapper.setProps({ serverId: 's2' })
    await flushPromises()
    finishLoad(config(999, 999))
    await flushPromises()
    expect((wrapper.get('.pacing-batch').element as HTMLInputElement).value).toBe('20')
    let finishSave!: (v: unknown) => void
    invoke.mockImplementationOnce(() => new Promise(resolve => { finishSave = resolve }))
    await wrapper.get('form').trigger('submit')
    invoke.mockResolvedValueOnce(config(30, 40))
    await wrapper.setProps({ serverId: 's3' })
    await flushPromises()
    finishSave({ batch_size: 20, delay_ms: 10 })
    await flushPromises()
    expect((wrapper.get('.pacing-batch').element as HTMLInputElement).value).toBe('30')
    expect(wrapper.find('[role="status"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('shows load/save failures without falsely reporting applied settings', async () => {
    invoke.mockRejectedValueOnce('load failed')
    const wrapper = mountSettings()
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toBe('load failed')
    expect(wrapper.get('.pacing-save').attributes('disabled')).toBeDefined()
    await wrapper.get('button[type="button"]').trigger('click')
    await flushPromises()
    await wrapper.get('.pacing-batch').setValue(5)
    invoke.mockRejectedValueOnce('save failed')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toBe('save failed')
    expect(wrapper.find('[role="status"]').exists()).toBe(false)
    expect((wrapper.get('.pacing-batch').element as HTMLInputElement).value).toBe('5')
    wrapper.unmount()
  })
})
