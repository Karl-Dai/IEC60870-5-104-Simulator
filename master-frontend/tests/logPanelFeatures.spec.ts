import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import type { ConnectionInfo, LogEntry } from '../src/types'
import LogPanel from '../src/components/LogPanel.vue'

const invokeMock = vi.fn()
const saveMock = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: (...args: unknown[]) => saveMock(...args),
}))

function entry(
  index: number,
  direction = 'rx',
  frameLabel: LogEntry['frame_label'] = { i_frame: 'M_SP_NA_1' },
  detail = `COT=3 IOA=${index}`,
): LogEntry {
  return {
    timestamp: `2026-08-04T09:10:${String(index % 60).padStart(2, '0')}.456Z`,
    direction,
    frame_label: frameLabel,
    detail,
    raw_bytes: [0x68, index & 0xff],
  }
}

type ConnectionStub = Pick<ConnectionInfo, 'id' | 'target_address' | 'port'>

let snapshot: LogEntry[] = []
let connections: ConnectionStub[] = []
let wrapper: VueWrapper | null = null
let selectedConnectionId = ref<string | null>('conn-1')

async function mountPanel() {
  invokeMock.mockImplementation((command: string) => {
    if (command === 'list_connections') return Promise.resolve(connections.slice())
    if (command === 'get_communication_logs') return Promise.resolve(snapshot.slice())
    return Promise.resolve(undefined)
  })
  wrapper = mount(LogPanel, {
    props: { expanded: true },
    global: {
      provide: {
        selectedConnectionId,
        openParseFrame: vi.fn(),
        [dialogKey as symbol]: { showAlert: vi.fn(() => Promise.resolve()) },
      },
    },
  })
  await flushPromises()
  return wrapper
}

async function flushScrollFrame() {
  await new Promise(resolve => window.setTimeout(resolve, 0))
  await nextTick()
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(done => { resolve = done })
  return { promise, resolve }
}

describe('master communication log enhancements', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    saveMock.mockReset()
    snapshot = []
    connections = [
      { id: 'conn-1', target_address: '192.0.2.20', port: 2404 },
      { id: 'conn-2', target_address: '192.0.2.21', port: 2404 },
    ]
    selectedConnectionId = ref('conn-1')
    useI18n().setLocale('en-US')
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => window.setTimeout(callback, 0))
    vi.stubGlobal('cancelAnimationFrame', (id: number) => window.clearTimeout(id))
  })

  afterEach(() => {
    wrapper?.unmount()
    wrapper = null
    vi.unstubAllGlobals()
  })

  it('combines filters/search, renders milliseconds, and exports the visible selection', async () => {
    const selected = entry(7, 'rx', { i_frame: 'M_ME_NC_1' }, 'COT=3 IOA=7 peer=192.0.2.20')
    const wrongFrame = entry(8, 'rx', 's_frame', 'COT=3 ACK')
    const wrongDirection = entry(9, 'tx', { i_frame: 'M_ME_NC_1' }, 'COT=6 IOA=9')
    snapshot = [selected, wrongFrame, wrongDirection]
    const panel = await mountPanel()

    expect(panel.find('td.col-time').text()).toMatch(/\.456/)
    await panel.find('.dir-filter').setValue('rx')
    await panel.find('.frame-filter').setValue('kind:i')
    await panel.find('.log-search').setValue('192.0.2.20')
    expect(panel.findAll('tbody tr:not(.log-spacer)')).toHaveLength(1)
    expect(panel.find('td.col-detail').text()).toContain('IOA=7')

    saveMock.mockResolvedValue('/tmp/master-filtered.csv')
    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      connectionId: 'conn-1',
      path: '/tmp/master-filtered.csv',
      entries: [selected],
    })
  })

  it('keeps the authoritative full-export call when no filters exist despite a populated cache', async () => {
    snapshot = [entry(1)]
    const panel = await mountPanel()
    saveMock.mockResolvedValue('/tmp/master-all.csv')

    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      connectionId: 'conn-1',
      path: '/tmp/master-all.csv',
    })
  })

  it('ignores a stale request that resolves after switching connections', async () => {
    snapshot = [entry(1, 'rx', { i_frame: 'M_SP_NA_1' }, 'initial')]
    const panel = await mountPanel()
    const stale = deferred<LogEntry[]>()
    const fresh = entry(2, 'rx', { i_frame: 'M_SP_NA_1' }, 'fresh connection')
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === 'get_communication_logs') {
        return args?.connectionId === 'conn-1' ? stale.promise : Promise.resolve([fresh])
      }
      return Promise.resolve(undefined)
    })

    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    await panel.find('.conn-select').setValue('conn-2')
    await flushPromises()
    expect(panel.find('td.col-detail').text()).toContain('fresh connection')

    stale.resolve([entry(3, 'rx', { i_frame: 'M_SP_NA_1' }, 'stale connection')])
    await flushPromises()
    expect(panel.find('td.col-detail').text()).toContain('fresh connection')
    expect(panel.text()).not.toContain('stale connection')
  })

  it('coalesces overlapping polls for one connection so a slow response can still land', async () => {
    snapshot = [entry(1, 'rx', { i_frame: 'M_SP_NA_1' }, 'initial')]
    const panel = await mountPanel()
    invokeMock.mockClear()
    const slow = deferred<LogEntry[]>()
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_communication_logs') return slow.promise
      return Promise.resolve(undefined)
    })

    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    expect(invokeMock.mock.calls.filter(([command]) => command === 'get_communication_logs')).toHaveLength(1)

    slow.resolve([entry(2, 'rx', { i_frame: 'M_SP_NA_1' }, 'slow response')])
    await flushPromises()
    expect(panel.find('td.col-detail').text()).toContain('slow response')
  })

  it('blocks polling during clear so an old snapshot cannot repopulate the table', async () => {
    snapshot = [entry(1)]
    const panel = await mountPanel()
    invokeMock.mockClear()
    const clear = deferred<void>()
    let pollCalls = 0
    invokeMock.mockImplementation((command: string) => {
      if (command === 'clear_communication_logs') return clear.promise
      if (command === 'get_communication_logs') {
        pollCalls++
        return Promise.resolve(snapshot.slice())
      }
      return Promise.resolve(undefined)
    })

    await panel.findAll('.log-header .log-btn')[1].trigger('click')
    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    expect(pollCalls).toBe(0)

    clear.resolve()
    await flushPromises()
    expect(panel.find('.log-table').exists()).toBe(false)
    expect(panel.text()).toContain('No logs')
  })

  it('captures connection and filtered rows before awaiting the save dialog', async () => {
    const clickedEntry = entry(4, 'rx', { i_frame: 'M_SP_NA_1' }, 'clicked snapshot')
    snapshot = [clickedEntry]
    const panel = await mountPanel()
    await panel.find('.dir-filter').setValue('rx')
    const path = deferred<string | null>()
    saveMock.mockReturnValue(path.promise)

    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    snapshot = [entry(5, 'rx', { i_frame: 'M_SP_NA_1' }, 'new target snapshot')]
    await panel.find('.conn-select').setValue('conn-2')
    path.resolve('/tmp/master-captured.csv')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      connectionId: 'conn-1',
      path: '/tmp/master-captured.csv',
      entries: [clickedEntry],
    })
  })

  it('serializes backend logging toggles across rapid connection changes', async () => {
    const panel = await mountPanel()
    invokeMock.mockClear()
    const disableFirst = deferred<void>()
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === 'get_communication_logs') return Promise.resolve([])
      if (command === 'set_logging_enabled'
        && args?.connectionId === 'conn-1' && args?.enabled === false) {
        return disableFirst.promise
      }
      return Promise.resolve(undefined)
    })

    await panel.find('.conn-select').setValue('conn-2')
    await nextTick()
    await panel.find('.conn-select').setValue('conn-1')
    disableFirst.resolve()
    await flushPromises()

    const toggles = invokeMock.mock.calls
      .filter(([command]) => command === 'set_logging_enabled')
      .map(([, args]) => args)
    expect(toggles).toEqual([
      { connectionId: 'conn-1', enabled: false },
      { connectionId: 'conn-1', enabled: true },
    ])
  })

  it('keeps a selected Type ID filter while the target reloads without that type', async () => {
    snapshot = [entry(1, 'rx', { i_frame: 'C_SC_NA_1' })]
    const panel = await mountPanel()
    await panel.find('.frame-filter').setValue('type:C_SC_NA_1')

    snapshot = [entry(2, 'rx', { i_frame: 'M_SP_NA_1' })]
    await panel.find('.conn-select').setValue('conn-2')
    await flushPromises()

    expect((panel.find('.frame-filter').element as HTMLSelectElement).value).toBe('type:C_SC_NA_1')
    expect(panel.find('.frame-filter').text()).toContain('C_SC_NA_1')
    expect(panel.text()).toContain('No logs match')
  })

  it('reselects a live connection when the current connection disappears', async () => {
    selectedConnectionId.value = null
    snapshot = [entry(1)]
    const panel = await mountPanel()
    await panel.find('.conn-select').setValue('conn-2')
    await flushPromises()
    expect((panel.find('.conn-select').element as HTMLSelectElement).value).toBe('conn-2')

    connections = [connections[0]]
    await panel.setProps({ expanded: false })
    await panel.setProps({ expanded: true })
    await flushPromises()

    expect((panel.find('.conn-select').element as HTMLSelectElement).value).toBe('conn-1')
    expect(panel.findAll('.conn-select option')).toHaveLength(1)
  })

  it('renders and colors the wire APCI kind even when the serialized label is misleading', async () => {
    const iFrame = entry(1, 'rx', { i_frame: 'misleading' }, 'wire I')
    iFrame.raw_bytes = [0x68, 0x0a, 0x00, 0x00, 0x00, 0x00, 13, 0, 0, 0, 0, 0]
    const sFrame = entry(2, 'rx', { i_frame: 'misleading' }, 'wire S')
    sFrame.raw_bytes = [0x68, 0x04, 0x01, 0x00, 0x00, 0x00]
    const uFrame = entry(3, 'rx', { i_frame: 'misleading' }, 'wire U')
    uFrame.raw_bytes = [0x68, 0x04, 0x43, 0x00, 0x00, 0x00]
    snapshot = [iFrame, sFrame, uFrame]
    const panel = await mountPanel()

    const frameCells = panel.findAll('td.col-frame')
    expect(frameCells.map(cell => cell.text())).toEqual(['U TESTFR ACT', 'S frame', 'I M_ME_NC_1'])
    expect(frameCells.map(cell => cell.classes().find(name => name.startsWith('frame-')))).toEqual([
      'frame-u', 'frame-s', 'frame-i',
    ])
  })

  it('localizes all semantic frame variants that can be emitted by the backend', async () => {
    snapshot = [
      entry(1, 'tx', 'counter_interrogation'),
      entry(2, 'tx', 'step_command'),
      entry(3, 'tx', 'bitstring'),
      entry(4, 'tx', 'raw_apdu'),
    ]
    const panel = await mountPanel()

    expect(panel.findAll('td.col-frame').map(cell => cell.text())).toEqual([
      'RAW APDU', 'C_BO', 'C_RC', 'C_CI',
    ])
  })

  it('preserves the reviewed row as new entries arrive and enforces resize minimums', async () => {
    snapshot = Array.from({ length: 40 }, (_, index) => entry(index))
    const panel = await mountPanel()
    const scroller = panel.find('.log-body').element as HTMLDivElement
    Object.defineProperty(scroller, 'clientHeight', { configurable: true, value: 100 })
    scroller.scrollTop = 55
    await panel.find('.log-body').trigger('scroll')
    await flushScrollFrame()
    expect(panel.find('.auto-follow-btn').attributes('aria-pressed')).toBe('false')

    snapshot = [...snapshot, entry(40)]
    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    await flushPromises()
    expect(scroller.scrollTop).toBe(80)

    snapshot = [...snapshot.slice(1), entry(41)]
    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    await flushPromises()
    expect(scroller.scrollTop).toBe(105)

    const firstCol = panel.find('colgroup col')
    panel.find('.column-resizer').element.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, clientX: 120 }))
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: -500 }))
    await nextTick()
    expect(firstCol.attributes('style')).toContain('105px')
    expect(panel.findAll('.column-resizer')).toHaveLength(6)

    await panel.find('.auto-follow-btn').trigger('click')
    await flushPromises()
    expect(scroller.scrollTop).toBe(0)
  })

  it('applies pointer and keyboard resize deltas exactly on a wide viewport', async () => {
    snapshot = [entry(1)]
    const panel = await mountPanel()
    const firstCol = panel.find('colgroup col')
    const handle = panel.find('.column-resizer')
    vi.spyOn(handle.element.parentElement as HTMLElement, 'getBoundingClientRect').mockReturnValue({
      width: 120,
      height: 25,
      top: 0,
      right: 120,
      bottom: 25,
      left: 0,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    })

    handle.element.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, clientX: 100 }))
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: 110 }))
    await nextTick()
    expect(firstCol.attributes('style')).toContain('130px')

    await handle.trigger('keydown', { key: 'ArrowRight' })
    expect(firstCol.attributes('style')).toContain('140px')
    expect(panel.find('.log-table').attributes('style')).not.toContain('min-width')
  })

  it('keeps a 10k-entry stream virtualized', async () => {
    snapshot = Array.from({ length: 10_000 }, (_, index) => entry(index))
    const panel = await mountPanel()

    expect(panel.findAll('tbody tr:not(.log-spacer)').length).toBeLessThanOrEqual(24)
    expect(panel.find('.filter-count').text()).toBe('10000 / 10000')
  })

  it('clamps a deep virtual-scroll window immediately when filters shrink the result', async () => {
    snapshot = Array.from({ length: 200 }, (_, index) => entry(index))
    snapshot[17] = entry(17, 'rx', { i_frame: 'M_SP_NA_1' }, 'ONLY_RESULT')
    const panel = await mountPanel()
    const scroller = panel.find('.log-body').element as HTMLDivElement
    Object.defineProperty(scroller, 'clientHeight', { configurable: true, value: 100 })
    scroller.scrollTop = 3_000
    await panel.find('.log-body').trigger('scroll')
    await flushScrollFrame()

    await panel.find('.log-search').setValue('ONLY_RESULT')

    expect(panel.findAll('tbody tr:not(.log-spacer)')).toHaveLength(1)
    expect(panel.find('td.col-detail').text()).toContain('ONLY_RESULT')
    expect(panel.find('.log-spacer').exists()).toBe(false)
  })
})
