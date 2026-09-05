// issue #64: configuration files are complete workspace snapshots. A
// successful open must reset every workspace-bound view before refreshing it;
// cancellation or a rejected load must leave the current view untouched.
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ref } from 'vue'
import { mount, flushPromises } from '@vue/test-utils'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import Toolbar from '../src/components/Toolbar.vue'

const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
const openMock = vi.fn()
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...a: unknown[]) => openMock(...a),
  save: () => Promise.resolve(null),
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: () => Promise.resolve() }))

const refreshTree = vi.fn()
const refreshData = vi.fn()
const resetWorkspaceView = vi.fn()
const showAlert = vi.fn(() => Promise.resolve())
const showPrompt = vi.fn()

function mountToolbar() {
  return mount(Toolbar, {
    global: {
      provide: {
        selectedServerId: ref<string | null>('s1'),
        selectedServerState: ref('Stopped'),
        selectedCA: ref<number | null>(1),
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
      stubs: { teleport: true, AboutDialog: true, LangSwitch: true, VersionBadge: true, NewServerModal: true },
    },
  })
}

// 「保存配置 · 加载配置」是同一组里的两个按钮,加载在后
function loadConfigButton(w: ReturnType<typeof mountToolbar>) {
  return w.find('[data-testid="open-config"]')
}

beforeEach(() => {
  invokeMock.mockReset()
  invokeMock.mockResolvedValue(3)
  openMock.mockReset()
  refreshTree.mockClear()
  refreshData.mockClear()
  resetWorkspaceView.mockClear()
  showAlert.mockClear()
  showPrompt.mockReset()
  showPrompt.mockResolvedValue(null)
  useI18n().setLocale('en-US')
})

describe('Toolbar full-workspace config loading', () => {
  it('loads a pasted path without invoking the native file picker', async () => {
    showPrompt.mockResolvedValue('  "/tmp/配置 files/cfg.json"  ')
    const w = mountToolbar()

    await w.find('[data-testid="open-config-by-path"]').trigger('click')
    await flushPromises()

    expect(openMock).not.toHaveBeenCalled()
    expect(invokeMock).toHaveBeenCalledWith('load_config', { path: '/tmp/配置 files/cfg.json' })
    expect(resetWorkspaceView).toHaveBeenCalledTimes(1)
    expect(resetWorkspaceView.mock.invocationCallOrder[0])
      .toBeLessThan(refreshTree.mock.invocationCallOrder[0])
    expect(refreshData).toHaveBeenCalledTimes(1)
    expect(showAlert).toHaveBeenCalledWith('Loaded 3 server(s)')
    w.unmount()
  })

  it.each([null, '   ', '""'])( 'does not load a cancelled or empty path: %s', async (input) => {
    showPrompt.mockResolvedValue(input)
    const w = mountToolbar()

    await w.find('[data-testid="open-config-by-path"]').trigger('click')
    await flushPromises()

    expect(openMock).not.toHaveBeenCalled()
    expect(invokeMock).not.toHaveBeenCalled()
    expect(resetWorkspaceView).not.toHaveBeenCalled()
    expect(refreshTree).not.toHaveBeenCalled()
    expect(showAlert).toHaveBeenCalledTimes(input === null ? 0 : 1)
    w.unmount()
  })

  it('retains a rejected path for correction and leaves the workspace untouched', async () => {
    showPrompt.mockResolvedValueOnce('/tmp/missing.json')
    invokeMock.mockRejectedValueOnce(new Error('file not found'))
    const w = mountToolbar()

    await w.find('[data-testid="open-config-by-path"]').trigger('click')
    await flushPromises()
    expect(showAlert.mock.calls.at(-1)?.[0]).toContain('file not found')
    expect(resetWorkspaceView).not.toHaveBeenCalled()
    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()

    await w.find('[data-testid="open-config-by-path"]').trigger('click')
    await flushPromises()
    expect(showPrompt.mock.calls.at(-1)?.[1]).toBe('/tmp/missing.json')
    expect(openMock).not.toHaveBeenCalled()
    w.unmount()
  })

  it('prevents overlapping imports while the direct load is pending', async () => {
    showPrompt.mockResolvedValue('/tmp/cfg.json')
    let finishLoad!: (count: number) => void
    invokeMock.mockImplementationOnce(() => new Promise<number>((resolve) => { finishLoad = resolve }))
    const w = mountToolbar()

    await w.find('[data-testid="open-config-by-path"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="open-config-by-path"]').attributes('aria-busy')).toBe('true')
    expect(loadConfigButton(w).attributes('disabled')).toBeDefined()
    await w.find('[data-testid="open-config-by-path"]').trigger('click')
    await loadConfigButton(w).trigger('click')
    expect(invokeMock).toHaveBeenCalledTimes(1)
    expect(openMock).not.toHaveBeenCalled()

    finishLoad(3)
    await flushPromises()
    expect(w.find('[data-testid="open-config-by-path"]').attributes('disabled')).toBeUndefined()
    w.unmount()
  })

  it('resets the old workspace before refreshing the newly loaded snapshot', async () => {
    openMock.mockResolvedValue('/tmp/cfg.json')
    const w = mountToolbar()

    await loadConfigButton(w).trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('load_config', { path: '/tmp/cfg.json' })
    expect(resetWorkspaceView).toHaveBeenCalledTimes(1)
    expect(refreshTree).toHaveBeenCalledTimes(1)
    expect(refreshData).toHaveBeenCalledTimes(1)
    expect(resetWorkspaceView.mock.invocationCallOrder[0])
      .toBeLessThan(refreshTree.mock.invocationCallOrder[0])
    expect(resetWorkspaceView.mock.invocationCallOrder[0])
      .toBeLessThan(refreshData.mock.invocationCallOrder[0])
    expect(showAlert).toHaveBeenCalledWith('Loaded 3 server(s)')
    w.unmount()
  })

  it('does not reset or refresh when file selection is cancelled', async () => {
    openMock.mockResolvedValue(null)
    const w = mountToolbar()

    await loadConfigButton(w).trigger('click')
    await flushPromises()

    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()
    expect(resetWorkspaceView).not.toHaveBeenCalled()
    w.unmount()
  })

  it('keeps the current workspace view when the file is rejected', async () => {
    openMock.mockResolvedValue('/tmp/bad.json')
    invokeMock.mockRejectedValueOnce(new Error('wrong app'))
    const w = mountToolbar()

    await loadConfigButton(w).trigger('click')
    await flushPromises()

    expect(resetWorkspaceView).not.toHaveBeenCalled()
    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()
    expect(showAlert.mock.calls.at(-1)?.[0]).toContain('Open failed')
    w.unmount()
  })
})
