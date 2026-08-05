import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, ref } from 'vue'
import { dialogKey } from '@shared/composables/useDialog'
import { useI18n } from '@shared/i18n'
import type { LogEntry } from '../src/types'
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
  detail = `IOA=${index}`,
): LogEntry {
  return {
    timestamp: `2026-08-04T09:10:${String(index % 60).padStart(2, '0')}.123Z`,
    direction,
    frame_label: frameLabel,
    detail,
    raw_bytes: [0x68, index & 0xff],
  }
}

let snapshot: LogEntry[] = []
let wrapper: VueWrapper | null = null
let selectedServerId = ref<string | null>('server-1')

async function mountPanel(handler?: (command: string, args?: Record<string, unknown>) => unknown) {
  invokeMock.mockImplementation(handler ?? ((command: string) => {
    if (command === 'get_communication_logs') return Promise.resolve(snapshot.slice())
    return Promise.resolve(undefined)
  }))
  wrapper = mount(LogPanel, {
    props: { expanded: true },
    global: {
      provide: {
        selectedServerId,
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

describe('slave communication log enhancements', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    saveMock.mockReset()
    snapshot = []
    selectedServerId = ref('server-1')
    useI18n().setLocale('en-US')
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => window.setTimeout(callback, 0))
    vi.stubGlobal('cancelAnimationFrame', (id: number) => window.clearTimeout(id))
  })

  afterEach(() => {
    wrapper?.unmount()
    wrapper = null
    vi.unstubAllGlobals()
  })

  it('combines Dir, Type ID, localized detail, and raw-byte search and exports only matches', async () => {
    const localized = entry(1, 'rx', { i_frame: 'M_SP_NA_1' }, 'technical fallback')
    localized.detail_event = {
      kind: 'serverStarted',
      payload: { address: '192.0.2.10:2404', transport: 'TCP' },
    }
    const selected = entry(2, 'rx', { i_frame: 'C_SC_NA_1' }, 'COT=6 IOA=2')
    const wrongDirection = entry(3, 'tx', { i_frame: 'C_SC_NA_1' }, 'COT=6 IOA=3')
    snapshot = [localized, selected, wrongDirection]
    const panel = await mountPanel()

    expect(panel.find('td.col-time').text()).toMatch(/\.123/)

    await panel.find('.dir-filter').setValue('rx')
    await panel.find('.frame-filter').setValue('type:C_SC_NA_1')
    expect(panel.findAll('tbody tr:not(.log-spacer)')).toHaveLength(1)
    expect(panel.find('td.col-detail').text()).toContain('IOA=2')

    await panel.find('.frame-filter').setValue('all')
    await panel.find('.log-search').setValue('Server started')
    expect(panel.findAll('tbody tr:not(.log-spacer)')).toHaveLength(1)
    expect(panel.find('td.col-detail').text()).toContain('192.0.2.10')

    await panel.find('.log-search').setValue('68 02')
    expect(panel.findAll('tbody tr:not(.log-spacer)')).toHaveLength(1)
    expect(panel.find('td.col-detail').text()).toContain('IOA=2')

    saveMock.mockResolvedValue('/tmp/filtered.csv')
    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    await flushPromises()
    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      serverId: 'server-1',
      path: '/tmp/filtered.csv',
      entries: [selected],
    })
  })

  it('exports a header-only selection when active filters have no results', async () => {
    snapshot = [entry(1)]
    const panel = await mountPanel()
    await panel.find('.log-search').setValue('definitely absent')
    saveMock.mockResolvedValue('/tmp/empty.csv')

    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      serverId: 'server-1',
      path: '/tmp/empty.csv',
      entries: [],
    })
  })

  it('uses the authoritative backend full export when no filters are active, even with a populated cache', async () => {
    snapshot = [entry(1)]
    const panel = await mountPanel()
    saveMock.mockResolvedValue('/tmp/all.csv')

    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      serverId: 'server-1',
      path: '/tmp/all.csv',
    })
  })

  it('ignores a stale request that completes after the selected server changed', async () => {
    snapshot = [entry(1, 'rx', { i_frame: 'M_SP_NA_1' }, 'initial')]
    const panel = await mountPanel()
    const stale = deferred<LogEntry[]>()
    const fresh = entry(2, 'rx', { i_frame: 'M_SP_NA_1' }, 'fresh server')
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command !== 'get_communication_logs') return Promise.resolve(undefined)
      return args?.serverId === 'server-1' ? stale.promise : Promise.resolve([fresh])
    })

    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    selectedServerId.value = 'server-2'
    await nextTick()
    await flushPromises()
    expect(panel.find('td.col-detail').text()).toContain('fresh server')

    stale.resolve([entry(3, 'rx', { i_frame: 'M_SP_NA_1' }, 'stale server')])
    await flushPromises()
    expect(panel.find('td.col-detail').text()).toContain('fresh server')
    expect(panel.text()).not.toContain('stale server')
  })

  it('coalesces overlapping polls for one server so a slow response can still land', async () => {
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

  it('blocks polling while a clear is in flight and keeps the cleared view authoritative', async () => {
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

  it('captures the target and filtered snapshot before the save dialog resolves', async () => {
    const clickedEntry = entry(4, 'rx', { i_frame: 'M_SP_NA_1' }, 'clicked snapshot')
    snapshot = [clickedEntry]
    const panel = await mountPanel()
    await panel.find('.dir-filter').setValue('rx')
    const path = deferred<string | null>()
    saveMock.mockReturnValue(path.promise)

    await panel.findAll('.log-header .log-btn')[2].trigger('click')
    snapshot = [entry(5, 'rx', { i_frame: 'M_SP_NA_1' }, 'new target snapshot')]
    selectedServerId.value = 'server-2'
    await nextTick()
    path.resolve('/tmp/captured.csv')
    await flushPromises()

    expect(invokeMock).toHaveBeenCalledWith('save_logs_csv', {
      serverId: 'server-1',
      path: '/tmp/captured.csv',
      entries: [clickedEntry],
    })
  })

  it('keeps a selected Type ID filter while the target reloads without that type', async () => {
    snapshot = [entry(1, 'rx', { i_frame: 'C_SC_NA_1' })]
    const panel = await mountPanel()
    await panel.find('.frame-filter').setValue('type:C_SC_NA_1')

    snapshot = [entry(2, 'rx', { i_frame: 'M_SP_NA_1' })]
    selectedServerId.value = 'server-2'
    await flushPromises()

    expect((panel.find('.frame-filter').element as HTMLSelectElement).value).toBe('type:C_SC_NA_1')
    expect(panel.find('.frame-filter').text()).toContain('C_SC_NA_1')
    expect(panel.text()).toContain('No logs match')
  })

  it('turns auto-follow off on manual scroll, preserves the visible anchor, and jumps to latest when re-enabled', async () => {
    snapshot = Array.from({ length: 40 }, (_, index) => entry(index))
    const panel = await mountPanel()
    const scroller = panel.find('.log-body').element as HTMLDivElement
    Object.defineProperty(scroller, 'clientHeight', { configurable: true, value: 100 })
    scroller.scrollTop = 50
    await panel.find('.log-body').trigger('scroll')
    await flushScrollFrame()
    expect(panel.find('.auto-follow-btn').attributes('aria-pressed')).toBe('false')

    snapshot = [...snapshot, entry(40)]
    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    await flushPromises()
    expect(scroller.scrollTop).toBe(75)

    // Simulate the capped 10k buffer: one old entry is evicted while one new
    // entry is prepended in the latest-first view, so length stays constant.
    snapshot = [...snapshot.slice(1), entry(41)]
    await panel.findAll('.log-header .log-btn')[0].trigger('click')
    await flushPromises()
    expect(scroller.scrollTop).toBe(100)

    await panel.find('.auto-follow-btn').trigger('click')
    await flushPromises()
    expect(panel.find('.auto-follow-btn').attributes('aria-pressed')).toBe('true')
    expect(scroller.scrollTop).toBe(0)
  })

  it('resizes every column while enforcing the minimum width', async () => {
    snapshot = [entry(1)]
    const panel = await mountPanel()
    const firstCol = panel.find('colgroup col')
    const firstHandle = panel.find('.column-resizer')

    firstHandle.element.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, clientX: 120 }))
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: -500 }))
    await nextTick()

    expect(firstCol.attributes('style')).toContain('105px')
    expect(panel.findAll('.column-resizer')).toHaveLength(4)
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

  it('clamps a deep virtual-scroll window immediately when filtering to one row', async () => {
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

  it('keeps a 10k-entry stream virtualized', async () => {
    snapshot = Array.from({ length: 10_000 }, (_, index) => entry(index))
    const panel = await mountPanel()

    expect(panel.findAll('tbody tr:not(.log-spacer)').length).toBeLessThanOrEqual(24)
    expect(panel.find('.filter-count').text()).toBe('10000 / 10000')
  })
})
