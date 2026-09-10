// issue #64: opening a config replaces the complete master workspace. The
// previous selection and component caches must be discarded only after the
// backend accepts the file, and before the new tree/data are refreshed.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, DOMWrapper } from '@vue/test-utils'
import { defineComponent, nextTick, ref } from 'vue'
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
const editConnectionMock = vi.fn()
const openNewConnectionMock = vi.fn()
const NewConnectionModalStub = defineComponent({
  setup(_, { expose }) {
    expose({
      openEditConnection: editConnectionMock,
      openNew: openNewConnectionMock,
    })
    return () => null
  },
})

function mountToolbar(
  selectedConnectionId = ref<string | null>(null),
  selectedConnectionState = ref('Disconnected'),
) {
  return mount(Toolbar, {
    attachTo: document.body,
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
        NewConnectionModal: NewConnectionModalStub,
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
  editConnectionMock.mockClear()
  openNewConnectionMock.mockClear()
  useI18n().setLocale('en-US')
})

describe('Toolbar full-workspace config loading', () => {
  it('resets the old workspace before refreshing the newly loaded snapshot', async () => {
    openMock.mockResolvedValue('/tmp/master.json')
    const wrapper = mountToolbar()

    await wrapper.find('[data-testid="menu-config"]').trigger('click')
    await new DOMWrapper(document.querySelector('[data-testid="open-config"]')!).trigger('click')
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

    await wrapper.find('[data-testid="menu-config"]').trigger('click')
    await new DOMWrapper(document.querySelector('[data-testid="open-config"]')!).trigger('click')
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

    await wrapper.find('[data-testid="menu-config"]').trigger('click')
    await new DOMWrapper(document.querySelector('[data-testid="open-config"]')!).trigger('click')
    await flushPromises()

    expect(resetWorkspaceView).not.toHaveBeenCalled()
    expect(refreshTree).not.toHaveBeenCalled()
    expect(refreshData).not.toHaveBeenCalled()
    expect(showAlert.mock.calls.at(-1)?.[0]).toContain('Open failed')
    wrapper.unmount()
  })
})

describe('Toolbar connection actions across workspace replacement', () => {
  it('provides a visible edit action for the selected connection', async () => {
    const selectedConnectionId = ref<string | null>('conn-secure')
    const wrapper = mountToolbar(selectedConnectionId, ref('Connected'))
    await nextTick()

    await wrapper.find('[data-testid="menu-connection"]').trigger('click')
    const editButton = new DOMWrapper(document.querySelector('[data-testid="edit-connection"]')!)
    expect(editButton.exists()).toBe(true)
    expect(editButton.attributes('disabled')).toBeUndefined()
    await editButton.trigger('click')

    expect(editConnectionMock).toHaveBeenCalledWith('conn-secure')
    wrapper.unmount()
  })

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

    await wrapper.find('[data-testid="menu-quick-gi"]').trigger('click')
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

const menuItem = (id: string) => new DOMWrapper(document.querySelector(`[data-testid="${id}"]`)!)
function connectedToolbar(cas = [1, 7]) {
  const id = ref<string | null>('selected')
  const state = ref('Connected')
  invokeMock.mockImplementation((command: string) => command === 'list_connections'
    ? Promise.resolve([{ id: 'selected', common_addresses: cas, broadcast_address: 0xFF00 }])
    : Promise.resolve())
  return { wrapper: mountToolbar(id, state), id, state }
}

describe('Master grouped command menus', () => {
  it('keeps one menu open, supports keyboard navigation, and restores focus', async () => {
    const { wrapper } = connectedToolbar()
    await flushPromises()
    await menuItem('menu-config').trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    expect(document.activeElement?.getAttribute('data-testid')).toBe('open-config')
    await menuItem('open-config').trigger('keydown', { key: 'End' })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('save-config')
    await menuItem('save-config').trigger('keydown', { key: 'Escape' })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('menu-config')
    expect(menuItem('open-config').isVisible()).toBe(false)
    await menuItem('menu-connection').trigger('click')
    await menuItem('menu-broadcast').trigger('click')
    await flushPromises()
    expect(menuItem('new-connection').isVisible()).toBe(false)
    expect(menuItem('broadcast-gi').isVisible()).toBe(true)
    expect(document.querySelector('#broadcast-menu')?.textContent).toContain('0xFF00')
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await flushPromises()
    expect(menuItem('broadcast-gi').isVisible()).toBe(false)
    wrapper.unmount()
  })

  it.each([
    ['gi', 'send_interrogation'],
    ['stop-gi', 'send_interrogation_deactivation'],
    ['counter', 'send_counter_read'],
    ['stop-counter', 'send_counter_read_deactivation'],
  ])('selects a specific CA for %s without sending to another CA', async (item, command) => {
    const { wrapper } = connectedToolbar()
    await flushPromises()
    await menuItem('menu-commands').trigger('click')
    await menuItem(item).trigger('click')
    await flushPromises()
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === command)).toEqual([])
    expect(menuItem('ca-7').isVisible()).toBe(true)
    await menuItem('ca-7').trigger('click')
    await flushPromises()
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === command)).toEqual([
      [command, { id: 'selected', commonAddress: 7 }],
    ])
    expect(document.querySelector('#commands-trigger')?.getAttribute('aria-expanded')).toBe('false')
    wrapper.unmount()
  })

  it('keeps single-CA GI direct and fans out only when All CAs is chosen', async () => {
    const single = connectedToolbar([4])
    await flushPromises()
    await menuItem('menu-quick-gi').trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('send_interrogation', { id: 'selected', commonAddress: 4 })
    single.wrapper.unmount()
    const { wrapper } = connectedToolbar()
    await flushPromises()
    invokeMock.mockClear()
    await menuItem('menu-commands').trigger('click')
    await menuItem('counter').trigger('click')
    await flushPromises()
    await menuItem('ca-all').trigger('click')
    await flushPromises()
    expect(invokeMock.mock.calls.filter(([cmd]) => cmd === 'send_counter_read')).toEqual([
      ['send_counter_read', { id: 'selected', commonAddress: 1 }],
      ['send_counter_read', { id: 'selected', commonAddress: 7 }],
    ])
    wrapper.unmount()
  })

  it('returns from the CA chooser and invalidates a pending lookup when a menu closes', async () => {
    const { wrapper } = connectedToolbar()
    await flushPromises()
    await menuItem('menu-commands').trigger('click')
    await menuItem('gi').trigger('click')
    await flushPromises()
    await menuItem('ca-7').trigger('keydown', { key: 'ArrowLeft' })
    await flushPromises()
    expect(menuItem('gi').isVisible()).toBe(true)
    let resolve!: (value: unknown) => void
    invokeMock.mockImplementationOnce(() => new Promise(r => { resolve = r }))
    await menuItem('gi').trigger('click')
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await flushPromises()
    resolve([{ id: 'selected', common_addresses: [1, 7] }])
    await flushPromises()
    expect(document.querySelector('#commands-trigger')?.getAttribute('aria-expanded')).toBe('false')
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'send_interrogation')).toBe(false)
    wrapper.unmount()
  })

  it('closes CA choices on disconnect and disables only connection-dependent commands', async () => {
    const { wrapper, state } = connectedToolbar()
    await flushPromises()
    await menuItem('menu-commands').trigger('click')
    await menuItem('gi').trigger('click')
    await flushPromises()
    state.value = 'Disconnected'
    await flushPromises()
    expect(document.querySelector('#commands-trigger')?.getAttribute('aria-expanded')).toBe('false')
    await menuItem('menu-commands').trigger('click')
    expect(menuItem('gi').attributes('disabled')).toBeDefined()
    await menuItem('menu-tools').trigger('click')
    expect(menuItem('parse-frame').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it.each([
    ['broadcast-gi', 'send_broadcast_gi'],
    ['broadcast-counter', 'send_broadcast_counter_read'],
    ['broadcast-stop-gi', 'send_broadcast_gi_deactivation'],
    ['broadcast-stop-counter', 'send_broadcast_counter_read_deactivation'],
  ])('routes %s through the existing broadcast command', async (item, command) => {
    const { wrapper } = connectedToolbar()
    await flushPromises()
    await menuItem('menu-broadcast').trigger('click')
    await menuItem(item).trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith(command, { id: 'selected' })
    wrapper.unmount()
  })
})


it('closes a quick CA popup when a resize can hide its trigger', async () => {
  const { wrapper } = connectedToolbar()
  await flushPromises()
  await menuItem('menu-quick-gi').trigger('click')
  await flushPromises()
  expect(menuItem('ca-7').isVisible()).toBe(true)
  window.dispatchEvent(new Event('resize'))
  await flushPromises()
  expect(document.querySelector('#quick-gi-trigger')?.getAttribute('aria-expanded')).toBe('false')
  wrapper.unmount()
})
