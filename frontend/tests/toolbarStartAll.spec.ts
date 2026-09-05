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

const selectedServerId = ref<string | null>(null)
const selectedServerState = ref('Stopped')

function mountToolbar() {
  return mount(Toolbar, {
    global: {
      provide: {
        selectedServerId,
        selectedServerState,
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
      stubs: { AboutDialog: true, LangSwitch: true, VersionBadge: true, NewServerModal: true },
    },
  })
}


const server = (id: string, state = 'Stopped') => ({
  id, state, bind_address: '127.0.0.1', port: 2404, station_count: 2, client_count: 0,
})
const button = (w: ReturnType<typeof mountToolbar>) => w.get('[data-testid="start-all-servers"]')

beforeEach(() => {
  vi.clearAllMocks()
  invokeMock.mockReset()
  selectedServerId.value = null
  selectedServerState.value = 'Stopped'
  showPrompt.mockResolvedValue(null)
  useI18n().setLocale('en-US')
})

describe('Toolbar start all servers', () => {
  it('starts all stopped servers without a selection and skips running servers', async () => {
    invokeMock.mockResolvedValueOnce([server('s1'), server('s2', 'Running'), server('s3')])
    const w = mountToolbar()
    await button(w).trigger('click')
    await flushPromises()
    expect(invokeMock.mock.calls).toEqual([
      ['list_servers'], ['start_server', { id: 's1' }], ['start_server', { id: 's3' }],
    ])
    expect(showAlert).toHaveBeenCalledWith('Start all completed: 2 started, 1 already running, 0 failed.')
    expect(refreshTree).toHaveBeenCalled()
    expect(button(w).attributes('disabled')).toBeUndefined()
    w.unmount()
  })

  it('continues after a failure and reports the failed endpoint while updating the selected server', async () => {
    selectedServerId.value = 's3'
    invokeMock.mockResolvedValueOnce([server('s1'), server('s2'), server('s3')])
      .mockResolvedValueOnce(undefined).mockRejectedValueOnce('port occupied').mockResolvedValueOnce(undefined)
    const w = mountToolbar()
    await button(w).trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('start_server', { id: 's3' })
    expect(selectedServerState.value).toBe('Running')
    expect(showAlert).toHaveBeenCalledTimes(1)
    expect(showAlert.mock.calls[0]?.[0]).toContain('2 started, 0 already running, 1 failed')
    expect(showAlert.mock.calls[0]?.[0]).toContain('127.0.0.1:2404 (s2): port occupied')
    w.unmount()
  })

  it.each([{ servers: [] }, { servers: [server('s1', 'Running')] }])('does not send start commands when nothing needs starting: $servers', async ({ servers }) => {
    invokeMock.mockResolvedValueOnce(servers)
    const w = mountToolbar()
    await button(w).trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledTimes(1)
    expect(showAlert).toHaveBeenCalledWith(servers.length
      ? 'Start all completed: 0 started, 1 already running, 0 failed.'
      : 'No servers. Create a server or load a configuration first.')
    w.unmount()
  })

  it('shows progress, serializes starts and blocks repeated starts and config replacement', async () => {
    let finish!: () => void
    invokeMock.mockResolvedValueOnce([server('s1'), server('s2')])
      .mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve }))
      .mockResolvedValueOnce(undefined)
    const w = mountToolbar()
    await button(w).trigger('click')
    await flushPromises()
    expect(button(w).text()).toBe('Starting 0/2')
    expect(button(w).attributes('aria-busy')).toBe('true')
    expect(w.get('[data-testid="open-config"]').attributes('disabled')).toBeDefined()
    expect(w.get('[data-testid="open-config-by-path"]').attributes('disabled')).toBeDefined()
    await button(w).trigger('click')
    await w.get('[data-testid="open-config"]').trigger('click')
    expect(invokeMock).toHaveBeenCalledTimes(2)
    expect(openMock).not.toHaveBeenCalled()
    selectedServerId.value = 'unrelated'
    finish()
    await flushPromises()
    expect(selectedServerState.value).toBe('Stopped')
    expect(invokeMock).toHaveBeenCalledTimes(3)
    expect(button(w).attributes('aria-busy')).toBe('false')
    expect(w.get('[data-testid="open-config"]').attributes('disabled')).toBeUndefined()
    w.unmount()
  })

  it('recovers from a list failure and permits retry', async () => {
    invokeMock.mockRejectedValueOnce('unavailable').mockResolvedValueOnce([server('s1')])
    const w = mountToolbar()
    await button(w).trigger('click')
    await flushPromises()
    expect(showAlert).toHaveBeenCalledWith('Unable to list servers: unavailable')
    expect(button(w).attributes('disabled')).toBeUndefined()
    await button(w).trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenLastCalledWith('start_server', { id: 's1' })
    w.unmount()
  })
})
