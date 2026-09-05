import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'
import { mount, flushPromises, DOMWrapper } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import Toolbar from '../src/components/Toolbar.vue'
import CsvImportModeModal from '../src/components/CsvImportModeModal.vue'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
const openMock = vi.fn()
const saveMock = vi.fn()
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...a: unknown[]) => openMock(...a),
  save: (...args: unknown[]) => saveMock(...args),
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: () => Promise.resolve() }))

const refreshTree = vi.fn()
const refreshData = vi.fn()
const resetWorkspaceView = vi.fn()
const showAlert = vi.fn(() => Promise.resolve())
const showPrompt = vi.fn()

const selectedServerId = ref<string | null>(null)
const selectedServerState = ref('Stopped')
const selectedCA = ref<number | null>(1)

function mountToolbar() {
  return mount(Toolbar, {
    attachTo: document.body,
    global: {
      provide: {
        selectedServerId,
        selectedServerState,
        selectedCA,
        refreshTree,
        refreshData,
        resetData: () => Promise.resolve(),
        resetWorkspaceView,
        openParseFrame: () => {},
        openRuntimeParamsDrawer: () => {},
        checkUpdate: () => Promise.resolve(null),
        [dialogKey as symbol]: {
          showAlert,
          showPrompt,
          showConfirm: () => Promise.resolve(false),
        },
      },
      stubs: { teleport: false, AboutDialog: true, LangSwitch: true, VersionBadge: true, NewServerModal: true },
    },
  })
}


beforeEach(() => {
  vi.clearAllMocks()
  invokeMock.mockReset()
  openMock.mockReset()
  saveMock.mockReset()
  selectedServerId.value = 's1'
  selectedServerState.value = 'Stopped'
  selectedCA.value = 1
  showPrompt.mockResolvedValue(null)
  useI18n().setLocale('en-US')
})

const item = (_w: ReturnType<typeof mountToolbar>, id: string) => new DOMWrapper(document.querySelector(`[data-testid="${id}"]`)!)

describe('Toolbar grouped menus and operation coordination', () => {
  it('opens one menu at a time, follows keyboard navigation and restores focus on Escape', async () => {
    const w = mountToolbar()
    expect(item(w, 'open-config').isVisible()).toBe(false)
    await item(w, 'menu-config').trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    expect(item(w, 'open-config').isVisible()).toBe(true)
    expect(document.activeElement?.getAttribute('data-testid')).toBe('open-config')
    await item(w, 'open-config').trigger('keydown', { key: 'ArrowDown' })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('open-config-by-path')
    await item(w, 'open-config-by-path').trigger('keydown', { key: 'End' })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('save-config')
    await item(w, 'save-config').trigger('keydown', { key: 'Escape' })
    expect(item(w, 'open-config').isVisible()).toBe(false)
    expect(document.activeElement?.getAttribute('data-testid')).toBe('menu-config')
    await item(w, 'menu-config').trigger('keydown', { key: 'ArrowUp' })
    await flushPromises()
    expect(document.activeElement?.getAttribute('data-testid')).toBe('save-config')
    await item(w, 'menu-points').trigger('click')
    expect(item(w, 'open-config').isVisible()).toBe(false)
    expect(item(w, 'export-point-csv').isVisible()).toBe(true)
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await flushPromises()
    expect(item(w, 'export-point-csv').isVisible()).toBe(false)
    w.unmount()
  })

  it('skips disabled menu items and keeps bulk controls outside menus', async () => {
    selectedServerState.value = 'Running'
    const w = mountToolbar()
    await item(w, 'menu-points').trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    expect(document.activeElement?.getAttribute('data-testid')).toBe('export-point-csv')
    expect(item(w, 'start-all-servers').isVisible()).toBe(true)
    expect(item(w, 'stop-all-servers').isVisible()).toBe(true)
    expect(item(w, 'check-update').isVisible()).toBe(false)
    await item(w, 'menu-help').trigger('click')
    expect(item(w, 'check-update').isVisible()).toBe(true)
    expect(item(w, 'about').isVisible()).toBe(true)
    w.unmount()
  })

  it('locks all mutations from the beginning of file selection and releases on cancel', async () => {
    let finish!: (value: null) => void
    openMock.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    const w = mountToolbar()
    await item(w, 'menu-config').trigger('click')
    await item(w, 'open-config').trigger('click')
    await flushPromises()
    for (const id of ['start-server', 'start-all-servers', 'stop-all-servers', 'import-point-csv', 'save-config', 'new-server']) {
      expect(item(w, id).attributes('disabled')).toBeDefined()
      await item(w, id).trigger('click')
    }
    expect(invokeMock).not.toHaveBeenCalled()
    expect(saveMock).not.toHaveBeenCalled()
    expect(openMock).toHaveBeenCalledTimes(1)
    finish(null)
    await flushPromises()
    expect(item(w, 'start-server').attributes('disabled')).toBeUndefined()
    w.unmount()
  })

  it('keeps the CSV target fixed across the picker and mode dialog', async () => {
    let finish!: (value: string) => void
    openMock.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    invokeMock.mockResolvedValue({ imported: 1, total_points: 1, mutations_started: 0 })
    const w = mountToolbar()
    await item(w, 'menu-points').trigger('click')
    await item(w, 'import-point-csv').trigger('click')
    selectedServerId.value = 's2'
    selectedCA.value = 20
    finish('/tmp/original.csv')
    await flushPromises()
    expect(item(w, 'start-all-servers').attributes('disabled')).toBeDefined()
    w.findComponent(CsvImportModeModal).vm.$emit('choose', 'merge')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('import_point_config_csv', {
      serverId: 's1', commonAddress: 1, path: '/tmp/original.csv', mode: 'merge',
    })
    expect(item(w, 'start-all-servers').attributes('disabled')).toBeUndefined()
    w.unmount()
  })

  it('does not overwrite a newly selected server state when the previous start finishes', async () => {
    let finish!: () => void
    invokeMock.mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve }))
    const w = mountToolbar()
    await item(w, 'start-server').trigger('click')
    selectedServerId.value = 's2'
    selectedServerState.value = 'Stopped'
    finish()
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('start_server', { id: 's1' })
    expect(selectedServerState.value).toBe('Stopped')
    w.unmount()
  })

  it('reports picker errors and unlocks the toolbar', async () => {
    saveMock.mockRejectedValueOnce(new Error('picker failed'))
    const w = mountToolbar()
    await item(w, 'menu-config').trigger('click')
    await item(w, 'save-config').trigger('click')
    await flushPromises()
    expect(showAlert.mock.calls.at(-1)?.[0]).toContain('picker failed')
    expect(item(w, 'start-all-servers').attributes('disabled')).toBeUndefined()
    w.unmount()
  })
})
