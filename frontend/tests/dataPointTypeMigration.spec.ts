import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import DataPointModal from '../src/components/DataPointModal.vue'
import BatchTypeMigrationModal from '../src/components/BatchTypeMigrationModal.vue'
import type { DataPointInfo } from '../src/types'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))

const point: DataPointInfo = {
  ioa: 100,
  asdu_type: 'M_SP_NA_1',
  category: 'single_point',
  name: 'breaker',
  comment: 'bay 1',
  value: '1',
  quality_ov: false,
  quality_bl: false,
  quality_sb: false,
  quality_nt: false,
  quality_iv: false,
  timestamp: null,
}

let wrapper: VueWrapper | null = null

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
})

beforeEach(() => {
  invokeMock.mockReset()
  invokeMock.mockResolvedValue(undefined)
})

function mountEditor(existingPoints: DataPointInfo[] = [point]) {
  wrapper = mount(DataPointModal, {
    props: {
      visible: true,
      serverId: 'server-1',
      commonAddress: 1,
      point,
      existingPoints,
    },
    global: {
      stubs: { teleport: true },
      provide: {
        [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
      },
    },
  })
  return wrapper
}

describe('point ASDU type migration', () => {
  it('allows compatible edit types and sends both source and target types', async () => {
    const editor = mountEditor()
    const typeSelect = editor.findAll('select.form-select')[0]

    expect(typeSelect.findAll('option').map(option => option.attributes('value')))
      .toEqual(['MSpNa1', 'MSpTa1', 'MSpTb1'])

    await typeSelect.setValue('MSpTb1')
    await editor.find('.btn-primary').trigger('click')
    await flushPromises()

    const call = invokeMock.mock.calls.find(([command]) =>
      command === 'update_data_point_definition'
    )
    expect(call?.[1]).toMatchObject({
      request: {
        server_id: 'server-1',
        common_address: 1,
        ioa: 100,
        new_ioa: 100,
        asdu_type: 'MSpNa1',
        new_asdu_type: 'MSpTb1',
        name: 'breaker',
        comment: 'bay 1',
      },
    })
  })

  it('blocks migration when the target key already exists', async () => {
    const editor = mountEditor([
      point,
      { ...point, asdu_type: 'M_SP_TB_1', name: 'existing target' },
    ])
    await editor.findAll('select.form-select')[0].setValue('MSpTb1')
    await flushPromises()

    expect(editor.find('.form-hint--error').exists()).toBe(true)
    expect((editor.find('.btn-primary').element as HTMLButtonElement).disabled).toBe(true)
    expect(invokeMock).not.toHaveBeenCalledWith(
      'update_data_point_definition',
      expect.anything(),
    )
  })

  it('submits a selected monitor collection through the batch migration command', async () => {
    invokeMock.mockImplementation((command: string) =>
      Promise.resolve(command === 'batch_migrate_data_point_types' ? 2 : undefined)
    )
    wrapper = mount(BatchTypeMigrationModal, {
      props: {
        visible: true,
        serverId: 'server-1',
        commonAddress: 1,
        points: [point, { ...point, ioa: 101, name: 'breaker 2' }],
      },
      global: {
        stubs: { teleport: true },
        provide: {
          [dialogKey as symbol]: { showAlert: () => Promise.resolve() },
        },
      },
    })

    await wrapper.find('select').setValue('MSpTb1')
    await wrapper.find('.btn-primary').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('batch_migrate_data_point_types', {
      request: {
        server_id: 'server-1',
        common_address: 1,
        points: [
          { ioa: 100, asdu_type: 'M_SP_NA_1' },
          { ioa: 101, asdu_type: 'M_SP_NA_1' },
        ],
        target_asdu_type: 'MSpTb1',
      },
    })
    expect(wrapper.emitted('applied')).toHaveLength(1)
  })
})
