// issue #64: opening a config replaces the complete master workspace. The
// previous selection and component caches must be discarded only after the
// backend accepts the file, and before the new tree/data are refreshed.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import Toolbar from '../src/components/Toolbar.vue'

const invokeMock = vi.fn()
const openMock = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: () => Promise.resolve(null),
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: () => Promise.resolve() }))

const refreshTree = vi.fn()
const refreshData = vi.fn()
const resetWorkspaceView = vi.fn()
const showAlert = vi.fn(() => Promise.resolve())

function mountToolbar(
  selectedConnectionId = ref<string | null>(null),
  selectedConnectionState = ref('Disconnected'),
) {
  return mount(Toolbar, {
    global: {
      provide: {
        selectedConnectionId,
        selectedConnectionState,
        refreshTree,
        refreshData,
        resetWorkspaceView,
        openParseFrame: () => {},
        checkUpdate: () => Promise.resolve(null),
        [dialogKey as symbol]: {
          showAlert,
          showConfirm: () => Promise.resolve(false),
        },
      },
      stubs: {
        AboutDialog: true,
        ControlDialog: true,
        LangSwitch: true,
        VersionBadge: true,
        NewConnectionModal: true,
      },
    },
  })
}

beforeEach(() => {
  invokeMock.mockReset()
  invokeMock.mockResolvedValue(2)
  openMock.mockReset()
  refreshTree.mockClear()
  refreshData.mockClear()
  resetWorkspaceView.mockClear()
  showAlert.mockClear()
  useI18n().setLocale('en-US')
})

describe('Toolbar full-workspace config loading', () => {
  it('resets the old workspace before refreshing the newly loaded snapshot', async () => {
    openMock.mockResolvedValue('/tmp/master.json')
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="open-config"]').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('load_config', { path: '/tmp/master.json' })
    expect(resetWorkspaceView).toHaveBeenCalledTimes(1)
    expect(refreshTree).toHaveBeenCalledTimes(1)
    expect(refreshData).toHaveBeenCalledTimes(1)
    expect(resetWorkspaceView.mock.invocationCallOrder[0])
      .toBeLessThan(refreshTree.mock.invocationCallOrder[0])
    expect(resetWorkspaceView.mock.invocationCallOrder[0])
      .toBeLessThan(refreshData.mock.invocationCallOrder[0])
    expect(showAlert).toHaveBeenCalledWith('Loaded 2 connection(s)')
    wrapper.unmount()
  })

  it('does not reset or refresh when file selection is cancelled', async () => {
    openMock.mockResolvedValue(null)
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="open-config"]').trigger('click')
    await flushPromises()

    expect(invokeMock).not.toHaveBeenCalledWith('load_config', expect.anything())
    expect(resetWorkspaceView).not.toHaveBeenCalled()
    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('keeps the current workspace view when the file is rejected', async () => {
    openMock.mockResolvedValue('/tmp/bad.json')
    invokeMock.mockRejectedValueOnce(new Error('wrong app'))
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="open-config"]').trigger('click')
    await flushPromises()

    expect(resetWorkspaceView).not.toHaveBeenCalled()
    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()
    expect(showAlert.mock.calls.at(-1)?.[0]).toContain('Open failed')
    wrapper.unmount()
  })
})

describe('Toolbar connection actions across workspace replacement', () => {
  it('ignores a pending disconnect failure after the workspace selection is reset', async () => {
    let rejectDisconnect!: (reason?: unknown) => void
    const pendingDisconnect = new Promise<void>((_resolve, reject) => {
      rejectDisconnect = reject
    })
    invokeMock.mockImplementation((command: string) => {
      if (command === 'disconnect_master') return pendingDisconnect
      if (command === 'list_connections') {
        return Promise.resolve([
          { id: 'old-connection', common_addresses: [1], broadcast_address: 0xFFFF },
        ])
      }
      return Promise.resolve()
    })
    const selectedConnectionId = ref<string | null>('old-connection')
    const selectedConnectionState = ref('Connected')
    const wrapper = mountToolbar(selectedConnectionId, selectedConnectionState)
    await flushPromises()
    refreshTree.mockClear()
    showAlert.mockClear()

    await wrapper.find('.btn-stop').trigger('click')
    expect(invokeMock).toHaveBeenCalledWith('disconnect_master', { id: 'old-connection' })

    // Equivalent to App.resetWorkspaceView() while the old IPC request is pending.
    selectedConnectionId.value = null
    selectedConnectionState.value = 'Disconnected'
    rejectDisconnect(new Error('old workspace failed'))
    await flushPromises()

    expect(selectedConnectionId.value).toBeNull()
    expect(selectedConnectionState.value).toBe('Disconnected')
    expect(refreshTree).not.toHaveBeenCalled()
    expect(showAlert).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('does not redirect a pending GI lookup to a newly selected connection', async () => {
    const oldConnection = {
      id: 'old-connection',
      common_addresses: [1],
      broadcast_address: 0xFFFF,
    }
    const newConnection = {
      id: 'new-connection',
      common_addresses: [2],
      broadcast_address: 0xFFFE,
    }
    invokeMock.mockImplementation((command: string) => {
      if (command === 'list_connections') return Promise.resolve([oldConnection, newConnection])
      return Promise.resolve()
    })
    const selectedConnectionId = ref<string | null>('old-connection')
    const selectedConnectionState = ref('Connected')
    const wrapper = mountToolbar(selectedConnectionId, selectedConnectionState)
    await flushPromises()

    let resolveLookup!: (connections: typeof oldConnection[]) => void
    const pendingLookup = new Promise<typeof oldConnection[]>((resolve) => {
      resolveLookup = resolve
    })
    invokeMock.mockImplementationOnce((command: string) => {
      expect(command).toBe('list_connections')
      return pendingLookup
    })

    await wrapper.find('.gi-btn-wrap > .toolbar-btn').trigger('click')
    selectedConnectionId.value = null
    selectedConnectionState.value = 'Disconnected'
    await nextTick()
    selectedConnectionId.value = 'new-connection'
    selectedConnectionState.value = 'Connected'
    await nextTick()

    resolveLookup([oldConnection])
    await flushPromises()

    expect(invokeMock.mock.calls.filter(([command]) => command === 'send_interrogation')).toEqual([])
    expect(showAlert).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
