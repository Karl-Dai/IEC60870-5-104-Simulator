import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import CsvImportModeModal from '../src/components/CsvImportModeModal.vue'
import Toolbar from '../src/components/Toolbar.vue'

const invokeMock = vi.fn()
const openMock = vi.fn()
const saveMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: () => Promise.resolve() }))

const refreshTree = vi.fn()
const refreshData = vi.fn()
const resetData = vi.fn(() => Promise.resolve())
const showAlert = vi.fn(() => Promise.resolve())
const showConfirm = vi.fn(() => Promise.resolve(true))

function mountToolbar(commonAddress: number | null = 12, serverState = 'Stopped') {
  return mount(Toolbar, {
    global: {
      provide: {
        selectedServerId: ref<string | null>('s1'),
        selectedServerState: ref(serverState),
        selectedCA: ref<number | null>(commonAddress),
        refreshTree,
        refreshData,
        resetData,
        openParseFrame: () => {},
        openRuntimeParamsDrawer: () => {},
        checkUpdate: () => Promise.resolve(null),
        [dialogKey as symbol]: {
          showAlert,
          showPrompt: () => Promise.resolve(null),
          showConfirm,
        },
      },
      stubs: { teleport: true, AboutDialog: true, LangSwitch: true, VersionBadge: true, NewServerModal: true },
    },
  })
}

beforeEach(() => {
  useI18n().setLocale('en-US')
  invokeMock.mockReset()
  invokeMock.mockImplementation((command: string) => {
    if (command === 'import_point_config_csv') {
      return Promise.resolve({ imported: 2, total_points: 3, mutations_started: 1 })
    }
    if (command === 'save_point_config_csv') return Promise.resolve(3)
    return Promise.resolve()
  })
  openMock.mockReset()
  saveMock.mockReset()
  refreshTree.mockClear()
  refreshData.mockClear()
  resetData.mockClear()
  showAlert.mockClear()
  showConfirm.mockReset()
  showConfirm.mockResolvedValue(true)
})

describe('Toolbar station point CSV actions', () => {
  it('disables all CSV actions until a station is selected', () => {
    const wrapper = mountToolbar(null)
    for (const testId of [
      'import-point-csv',
      'export-point-csv',
      'download-point-csv-template',
    ]) {
      expect((wrapper.find(`[data-testid="${testId}"]`).element as HTMLButtonElement).disabled).toBe(true)
    }
    wrapper.unmount()
  })

  it('disables only Import while the selected server is running', () => {
    const wrapper = mountToolbar(12, 'Running')
    expect((wrapper.find('[data-testid="import-point-csv"]').element as HTMLButtonElement).disabled)
      .toBe(true)
    expect(wrapper.find('[data-testid="import-point-csv"]').attributes('title'))
      .toContain('Stop the selected server')
    expect((wrapper.find('[data-testid="export-point-csv"]').element as HTMLButtonElement).disabled)
      .toBe(false)
    expect((wrapper.find('[data-testid="download-point-csv-template"]').element as HTMLButtonElement).disabled)
      .toBe(false)
    wrapper.unmount()
  })

  it('opens an explicit mode dialog and imports Merge transactionally', async () => {
    openMock.mockResolvedValue('/tmp/points.csv')
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="import-point-csv"]').trigger('click')
    await flushPromises()
    const modeDialog = wrapper.findComponent(CsvImportModeModal)
    expect(modeDialog.props('visible')).toBe(true)

    modeDialog.vm.$emit('choose', 'merge')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('import_point_config_csv', {
      serverId: 's1',
      commonAddress: 12,
      path: '/tmp/points.csv',
      mode: 'merge',
    })
    expect(showConfirm).not.toHaveBeenCalled()
    expect(refreshTree).toHaveBeenCalledTimes(1)
    expect(resetData).toHaveBeenCalledTimes(1)
    expect(refreshData).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('requires a destructive confirmation for Replace and treats cancel as no action', async () => {
    openMock.mockResolvedValue('/tmp/replace.csv')
    showConfirm.mockResolvedValue(false)
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="import-point-csv"]').trigger('click')
    await flushPromises()
    wrapper.findComponent(CsvImportModeModal).vm.$emit('choose', 'replace')
    await flushPromises()

    expect(showConfirm).toHaveBeenCalledTimes(1)
    expect(invokeMock).not.toHaveBeenCalledWith('import_point_config_csv', expect.anything())
    expect(resetData).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('adds a localized transactional hint while preserving complete backend row details', async () => {
    useI18n().setLocale('zh-CN')
    openMock.mockResolvedValue('/tmp/invalid.csv')
    invokeMock.mockRejectedValueOnce(
      'CSV validation failed (51 error(s)):\nRow 52 [Type ID]: unrecognized IEC type ID 255',
    )
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="import-point-csv"]').trigger('click')
    await flushPromises()
    wrapper.findComponent(CsvImportModeModal).vm.$emit('choose', 'merge')
    await flushPromises()

    const message = String(showAlert.mock.calls.at(-1)?.[0])
    expect(message).toContain('本次导入未写入任何点位')
    expect(message).toContain('Row 52 [Type ID]')
    expect(resetData).not.toHaveBeenCalled()
    wrapper.unmount()
    useI18n().setLocale('en-US')
  })

  it('exports the selected station and downloads a CA-aware template', async () => {
    saveMock
      .mockResolvedValueOnce('/tmp/export.csv')
      .mockResolvedValueOnce('/tmp/template.csv')
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="export-point-csv"]').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('save_point_config_csv', {
      serverId: 's1',
      commonAddress: 12,
      path: '/tmp/export.csv',
    })

    await wrapper.find('[data-testid="download-point-csv-template"]').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('save_point_config_csv_template', {
      commonAddress: 12,
      path: '/tmp/template.csv',
    })
    wrapper.unmount()
  })
})

describe('CsvImportModeModal', () => {
  it('offers distinct Merge, Replace, and Cancel actions', async () => {
    const wrapper = mount(CsvImportModeModal, {
      props: { visible: true },
      global: { stubs: { teleport: true } },
    })
    expect(wrapper.find('[data-testid="csv-mode-merge"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="csv-mode-replace"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="csv-mode-cancel"]').exists()).toBe(true)

    await wrapper.find('[data-testid="csv-mode-merge"]').trigger('click')
    await wrapper.find('[data-testid="csv-mode-replace"]').trigger('click')
    await wrapper.find('[data-testid="csv-mode-cancel"]').trigger('click')
    expect(wrapper.emitted('choose')).toEqual([['merge'], ['replace']])
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    wrapper.unmount()
  })
})
