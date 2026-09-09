import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
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

const showAlert = vi.fn(() => Promise.resolve())
const showConfirm = vi.fn(() => Promise.resolve(true))

function mountToolbar(serverState = 'Running', commonAddress: number | null = 12) {
  return mount(Toolbar, {
    global: {
      provide: {
        selectedServerId: ref<string | null>('s1'),
        selectedServerState: ref(serverState),
        selectedCA: ref<number | null>(commonAddress),
        refreshTree: () => {},
        refreshData: () => {},
        resetData: () => Promise.resolve(),
        openParseFrame: () => {},
        openRuntimeParamsDrawer: () => {},
        openServerSettings: () => {},
        checkUpdate: () => Promise.resolve(null),
        [dialogKey as symbol]: {
          showAlert,
          showPrompt: () => Promise.resolve(null),
          showConfirm,
        },
      },
      stubs: {
        teleport: true,
        AboutDialog: true,
        LangSwitch: true,
        VersionBadge: true,
        NewServerModal: true,
      },
    },
  })
}

beforeEach(() => {
  useI18n().setLocale('en-US')
  vi.clearAllMocks()
  openMock.mockResolvedValue('/tmp/events.json')
  saveMock.mockResolvedValue('/tmp/example.json')
  showConfirm.mockResolvedValue(true)
  invokeMock.mockImplementation((command: string) => {
    if (command === 'inspect_point_event_schedule') {
      return Promise.resolve({ event_count: 2, point_count: 1, duration_ms: 1000 })
    }
    if (command === 'save_point_event_schedule_example') return Promise.resolve(8)
    return Promise.resolve({ task_id: 'task-1' })
  })
})

describe('Toolbar point-event JSON actions', () => {
  it('enables import only for a running selected station', () => {
    const stopped = mountToolbar('Stopped')
    expect((stopped.find('[data-testid="import-point-event-json"]').element as HTMLButtonElement).disabled).toBe(true)
    expect((stopped.find('[data-testid="download-point-event-example"]').element as HTMLButtonElement).disabled).toBe(false)
    stopped.unmount()

    const running = mountToolbar('Running')
    expect((running.find('[data-testid="import-point-event-json"]').element as HTMLButtonElement).disabled).toBe(false)
    running.unmount()

    const noStation = mountToolbar('Running', null)
    expect((noStation.find('[data-testid="import-point-event-json"]').element as HTMLButtonElement).disabled).toBe(true)
    expect((noStation.find('[data-testid="download-point-event-example"]').element as HTMLButtonElement).disabled).toBe(true)
    noStation.unmount()
  })

  it('inspects, confirms, and starts the selected file', async () => {
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="import-point-event-json"]').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenNthCalledWith(1, 'inspect_point_event_schedule', {
      serverId: 's1',
      commonAddress: 12,
      path: '/tmp/events.json',
    })
    expect(showConfirm).toHaveBeenCalledWith(expect.stringContaining('2 event(s)'))
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'start_point_event_schedule', {
      serverId: 's1',
      commonAddress: 12,
      path: '/tmp/events.json',
    })
    wrapper.unmount()
  })

  it('does not start when confirmation is cancelled', async () => {
    showConfirm.mockResolvedValue(false)
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="import-point-event-json"]').trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledTimes(1)
    expect(invokeMock).toHaveBeenCalledWith('inspect_point_event_schedule', expect.any(Object))
    wrapper.unmount()
  })

  it('saves a station-based example file', async () => {
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="download-point-event-example"]').trigger('click')
    await flushPromises()

    expect(saveMock).toHaveBeenCalledWith(expect.objectContaining({
      defaultPath: 'iec104-point-events-ca-12.json',
    }))
    expect(invokeMock).toHaveBeenCalledWith('save_point_event_schedule_example', {
      serverId: 's1',
      commonAddress: 12,
      path: '/tmp/example.json',
    })
    expect(showAlert).toHaveBeenCalledWith(expect.stringContaining('8 event(s)'))
    wrapper.unmount()
  })
})
