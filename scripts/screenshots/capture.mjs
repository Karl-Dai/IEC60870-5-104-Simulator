/**
 * Regenerate the README tutorial screenshots (docs/screenshots/tut-*.png).
 *
 * These are HEADLESS captures of the exact same Vue frontends the desktop apps
 * ship, with the Tauri IPC layer (window.__TAURI_INTERNALS__) stubbed so the UI
 * renders populated without a real backend. No native GUI is launched — the
 * pages run in the dev servers and are screenshotted in headless Chromium.
 *
 * Prerequisites
 *   1. Dev servers running (in two terminals, from the repo root):
 *        npm --prefix frontend run dev          # slave  -> http://localhost:5176
 *        npm --prefix master-frontend run dev   # master -> http://localhost:5177
 *   2. Playwright + Chromium (installed under scripts/):
 *        npm --prefix scripts i -D playwright
 *        npx --prefix scripts playwright install chromium
 *
 * Run
 *        node scripts/screenshots/capture.mjs
 */
import { chromium } from 'playwright'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const HERE = dirname(fileURLToPath(import.meta.url))
const OUT_DIR = resolve(HERE, '../../docs/screenshots')
const SLAVE = 'http://localhost:5176/'
const MASTER = 'http://localhost:5177/'
const DARK_BG = 'rgb(17, 17, 27)' // body background once shared tokens.css applies

// ----------------------------------------------------------------------------
// Mock data — representative of a small substation talking to one master on CA 1
// ----------------------------------------------------------------------------
const q = { quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false }

const slavePoints = [
  { ioa: 1, asdu_type: 'M_SP_NA_1', category: '单点 (SP)', name: '断路器合位', comment: '断路器 QF1', value: '1', ...q, timestamp: null },
  { ioa: 2, asdu_type: 'M_DP_NA_3', category: '双点 (DP)', name: '隔离开关', comment: '刀闸 QS1', value: '2', ...q, timestamp: null },
  { ioa: 3, asdu_type: 'M_ST_NA_5', category: '步位置 (ST)', name: '有载调压档位', comment: '', value: '8', ...q, timestamp: null },
  { ioa: 4, asdu_type: 'M_BO_NA_7', category: '位串 (BO)', name: '保护动作字', comment: '', value: '16711680', ...q, timestamp: null },
  { ioa: 5, asdu_type: 'M_ME_NA_9', category: '归一化 (ME_NA)', name: '母线电压', comment: '10kV I 母', value: '0.95', ...q, timestamp: null },
  { ioa: 6, asdu_type: 'M_ME_NB_11', category: '标度化 (ME_NB)', name: '有功功率', comment: '', value: '1250', ...q, timestamp: null },
  { ioa: 7, asdu_type: 'M_ME_NC_13', category: '浮点 (ME_NC)', name: '系统频率', comment: '', value: '50.02', ...q, timestamp: null },
  { ioa: 8, asdu_type: 'M_IT_NA_15', category: '累计量 (IT)', name: '正向有功电度', comment: '', value: '123456', ...q, timestamp: null },
]

const masterPoints = slavePoints.map((p, i) => ({
  ioa: 101 + i, common_address: 1, asdu_type: p.asdu_type, category: p.category, value: p.value, ...q, timestamp: null, update_seq: 200,
}))

const conn = {
  id: 'c1', target_address: '127.0.0.1', port: 2404, common_addresses: [1], state: 'Connected', use_tls: false,
  t0: 30, t1: 15, t2: 10, t3: 20, k: 12, w: 8, default_qoi: 20, default_qcc: 6,
  interrogate_period_s: 60, counter_interrogate_period_s: 60, broadcast_address: 65535,
}

// COT is taken from detail_event.payload.cot; frame label from the snake_case key.
const masterLogs = [
  { timestamp: '2026-05-29T12:00:00.100Z', direction: '↑', frame_label: 'u_start_act', detail: '', detail_event: null, raw_bytes: [0x68, 0x04, 0x07, 0x00, 0x00, 0x00] },
  { timestamp: '2026-05-29T12:00:00.150Z', direction: '↓', frame_label: 'u_start_con', detail: '', detail_event: null, raw_bytes: [0x68, 0x04, 0x0b, 0x00, 0x00, 0x00] },
  { timestamp: '2026-05-29T12:00:01.200Z', direction: '↑', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤命令 (QOI=20)', detail_event: { payload: { cot: 6 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x64, 0x01, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-05-29T12:00:01.260Z', direction: '↓', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤确认', detail_event: { payload: { cot: 7 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x02, 0x00, 0x64, 0x01, 0x07, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-05-29T12:00:01.300Z', direction: '↓', frame_label: { i_frame: 'M_SP_NA_1' }, detail: '单点信息 IOA=101', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x12, 0x00, 0x00, 0x02, 0x00, 0x01, 0x01, 0x14, 0x00, 0x01, 0x00, 0x65, 0x00, 0x00, 0x01] },
  { timestamp: '2026-05-29T12:00:01.420Z', direction: '↓', frame_label: { i_frame: 'M_ME_NC_1' }, detail: '短浮点测量值 IOA=107', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x14, 0x02, 0x00, 0x02, 0x00, 0x0d, 0x01, 0x14, 0x00, 0x01, 0x00, 0x6b, 0x00, 0xa4, 0x70, 0x49, 0x42, 0x00] },
  { timestamp: '2026-05-29T12:00:01.560Z', direction: '↓', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤结束', detail_event: { payload: { cot: 10 } }, raw_bytes: [0x68, 0x0e, 0x04, 0x00, 0x04, 0x00, 0x64, 0x01, 0x0a, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-05-29T12:00:05.880Z', direction: '↓', frame_label: { i_frame: 'M_ME_NC_1' }, detail: '短浮点测量值 IOA=107 (变位)', detail_event: { payload: { cot: 3 } }, raw_bytes: [0x68, 0x14, 0x06, 0x00, 0x04, 0x00, 0x0d, 0x01, 0x03, 0x00, 0x01, 0x00, 0x6b, 0x00, 0xc3, 0xf5, 0x48, 0x42, 0x00] },
]

// ----------------------------------------------------------------------------
// IPC stub — runs in the browser BEFORE the app boots (no closure over Node).
// ----------------------------------------------------------------------------
function installTauriMock(cfg) {
  try { localStorage.setItem('iec104.language', cfg.locale) } catch (e) { /* ignore */ }
  const DATA = cfg.commands || {}
  // these two return an incremental {seq,total_count,points} envelope; once the
  // poller has caught up (sinceSeq >= seq) return an empty delta so it stays put.
  const INCREMENTAL = ['list_data_points_since', 'get_received_data_since']
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      if (cmd.indexOf('plugin:event|') === 0) return 1 // listen()/unlisten() event id
      if (!(cmd in DATA)) return null
      const v = DATA[cmd]
      if (INCREMENTAL.indexOf(cmd) !== -1 && v && Array.isArray(v.points)) {
        if (args && typeof args.sinceSeq === 'number' && args.sinceSeq >= v.seq) {
          return { seq: v.seq, total_count: v.total_count, points: [] }
        }
      }
      return v
    },
    transformCallback: (cb) => { const id = Math.floor(Math.random() * 1e9); window['_cb' + id] = cb; return id },
    unregisterCallback: () => {},
    convertFileSrc: (p) => p,
  }
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
}

// ----------------------------------------------------------------------------
// Shot list
// ----------------------------------------------------------------------------
const masterConnected = {
  list_connections: [conn],
  get_received_data_since: { seq: 200, total_count: 8, points: masterPoints },
  get_communication_logs: masterLogs,
  check_for_update: null,
  set_logging_enabled: null,
}

const shots = [
  {
    name: 'tut-1-slave',
    url: SLAVE,
    cfg: { locale: 'zh-CN', commands: {
      list_servers: [{ id: 's1', bind_address: '0.0.0.0', port: 2404, state: 'Running', station_count: 1 }],
      list_stations: [{ common_address: 1, name: '变电站 1', point_count: 8 }],
      list_data_points_since: { seq: 100, total_count: 8, points: slavePoints },
      get_communication_logs: [],
      check_for_update: null,
    } },
    async act(page) {
      await page.getByText('变电站 1', { exact: false }).first().click().catch(() => {})
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 8, { timeout: 6000 }).catch(() => {})
      await page.locator('table tbody tr').first().click().catch(() => {})
      await page.waitForTimeout(600)
    },
  },
  {
    name: 'tut-2-master-newconn',
    url: MASTER,
    cfg: { locale: 'zh-CN', commands: {
      list_connections: [],
      get_received_data_since: { seq: 0, total_count: 0, points: [] },
      get_communication_logs: [],
      check_for_update: null,
      set_logging_enabled: null,
    } },
    async act(page) {
      await page.getByText('新建连接', { exact: false }).first().click().catch(() => {})
      await page.waitForTimeout(700)
    },
  },
  {
    name: 'tut-3-master-data',
    url: MASTER,
    cfg: { locale: 'zh-CN', commands: masterConnected },
    async act(page) {
      await page.getByText('127.0.0.1', { exact: false }).first().click().catch(() => {})
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 8, { timeout: 6000 }).catch(() => {})
      await page.waitForTimeout(500)
    },
  },
  {
    name: 'tut-4-master-log',
    url: MASTER,
    cfg: { locale: 'zh-CN', commands: masterConnected },
    async act(page) {
      await page.getByText('127.0.0.1', { exact: false }).first().click().catch(() => {})
      await page.waitForTimeout(800)
      await page.getByText('通信日志', { exact: false }).first().click().catch(() => {})
      await page.waitForTimeout(1500)
    },
  },
]

// ----------------------------------------------------------------------------
const browser = await chromium.launch()
try {
  for (const shot of shots) {
    // locale drives navigator.language, which the i18n detector falls back to;
    // addInitScript's localStorage.setItem can't run on the pre-navigation origin.
    const ctx = await browser.newContext({ viewport: { width: 1200, height: 800 }, deviceScaleFactor: 1, locale: shot.cfg.locale })
    const page = await ctx.newPage()
    await page.addInitScript(installTauriMock, shot.cfg)
    await page.goto(shot.url, { waitUntil: 'domcontentloaded' })
    await page.waitForFunction((bg) => getComputedStyle(document.body).backgroundColor === bg, DARK_BG, { timeout: 8000 }).catch(() => {})
    await shot.act(page)
    await page.screenshot({ path: resolve(OUT_DIR, shot.name + '.png') })
    await ctx.close()
    console.log('✓', shot.name)
  }
} finally {
  await browser.close()
}
