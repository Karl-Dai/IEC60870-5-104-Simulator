/**
 * Regenerate every screenshot referenced by README.md / README_CN.md.
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
 *        node scripts/screenshots/build-hero-animation.mjs
 */
import { chromium } from 'playwright'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'
import { readFileSync } from 'node:fs'

const HERE = dirname(fileURLToPath(import.meta.url))
const OUT_DIR = resolve(HERE, '../../docs/screenshots')
const SLAVE = 'http://localhost:5176/'
const MASTER = 'http://localhost:5177/'
const DARK_BG = 'rgb(17, 17, 27)' // body background once shared tokens.css applies
const APP_VERSION = JSON.parse(
  readFileSync(resolve(HERE, '../../crates/iec104sim-app/tauri.conf.json'), 'utf8'),
).version

// ----------------------------------------------------------------------------
// Mock data — representative of a small substation talking to one master on CA 1
// ----------------------------------------------------------------------------
const q = { quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false }

// asdu_type/category 须与后端 AsduTypeId::name() / DataCategory::key() 一致
const slaveMonitorPoints = [
  { ioa: 1, asdu_type: 'M_SP_NA_1', category: 'single_point', name: '断路器合位', comment: '断路器 QF1', value: '1', ...q, timestamp: null },
  { ioa: 2, asdu_type: 'M_DP_NA_1', category: 'double_point', name: '隔离开关', comment: '刀闸 QS1', value: '2', ...q, timestamp: null },
  { ioa: 3, asdu_type: 'M_ST_NA_1', category: 'step_position', name: '有载调压档位', comment: '', value: '8', ...q, timestamp: null },
  { ioa: 4, asdu_type: 'M_BO_NA_1', category: 'bitstring', name: '保护动作字', comment: '', value: '16711680', ...q, timestamp: null },
  { ioa: 5, asdu_type: 'M_ME_NA_1', category: 'normalized_measured', name: '母线电压', comment: '10kV I 母', value: '0.95', ...q, timestamp: null },
  { ioa: 6, asdu_type: 'M_ME_NB_1', category: 'scaled_measured', name: '有功功率', comment: '', value: '1250', ...q, timestamp: null },
  { ioa: 7, asdu_type: 'M_ME_NC_1', category: 'float_measured', name: '系统频率', comment: '', value: '50.02', ...q, timestamp: null },
  { ioa: 8, asdu_type: 'M_IT_NA_1', category: 'integrated_totals', name: '正向有功电度', comment: '', value: '123456', ...q, timestamp: null },
]

const slaveControlPoints = [
  {
    ioa: 1001, asdu_type: 'C_SC_NA_1', category: 'single_command', name: '断路器遥控', comment: '映射 QF1 合位', value: '0',
    mapping_common_address: 1, mapping_ioa: 1, mapping_asdu_type: 'M_SP_NA_1', command_qualifier: 0, select_before_operate: true,
    ...q, timestamp: null,
  },
  {
    ioa: 1007, asdu_type: 'C_SE_NC_1', category: 'float_setpoint', name: '频率设定值', comment: '映射系统频率', value: '50.00',
    mapping_common_address: 1, mapping_ioa: 7, mapping_asdu_type: 'M_ME_NC_1', command_qualifier: 0, select_before_operate: false,
    ...q, timestamp: null,
  },
]

const slavePoints = [...slaveMonitorPoints, ...slaveControlPoints]

const masterPoints = slaveMonitorPoints.map((p, i) => ({
  ioa: 101 + i, common_address: 1, asdu_type: p.asdu_type, category: p.category, value: p.value, ...q, timestamp: null, update_seq: 200,
}))

const conn = {
  id: 'c1', target_address: '127.0.0.1', port: 2404, common_addresses: [1], state: 'Connected', use_tls: false,
  t0: 30, t1: 15, t2: 10, t3: 20, k: 12, w: 8, default_qoi: 20, default_qcc: 6,
  interrogate_period_s: 60, counter_interrogate_period_s: 60, broadcast_address: 65535,
}

const multiCaConn = {
  ...conn,
  id: 'multi-ca',
  target_address: '10.15.48.12',
  common_addresses: [1, 2, 3],
  use_tls: true,
  channel_retry_s: 5,
  use_socks5: false,
  socks5_proxy_address: '127.0.0.1',
  socks5_proxy_port: 1080,
  socks5_username: '',
  socks5_password: '',
  socks5_remote_dns: true,
  ca_file: './ca.pem',
  cert_file: './client.pem',
  key_file: './client-key.pem',
  accept_invalid_certs: false,
  tls_version: 'auto',
}

const multiCaPoints = [1, 2, 3].flatMap((ca) => masterPoints.map((point, index) => ({
  ...point,
  common_address: ca,
  ioa: 101 + index,
  value: point.category === 'float_measured' ? (49.98 + ca * 0.01).toFixed(2) : point.value,
  update_seq: 300 + (ca - 1) * masterPoints.length + index,
})))

// COT is taken from detail_event.payload.cot; frame label from the snake_case key.
const masterLogs = [
  { timestamp: '2026-05-29T12:00:00.100Z', direction: 'tx', frame_label: 'u_start_act', detail: '', detail_event: null, raw_bytes: [0x68, 0x04, 0x07, 0x00, 0x00, 0x00] },
  { timestamp: '2026-05-29T12:00:00.150Z', direction: 'rx', frame_label: 'u_start_con', detail: '', detail_event: null, raw_bytes: [0x68, 0x04, 0x0b, 0x00, 0x00, 0x00] },
  { timestamp: '2026-05-29T12:00:01.200Z', direction: 'tx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤命令 (QOI=20)', detail_event: { payload: { cot: 6 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x64, 0x01, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-05-29T12:00:01.260Z', direction: 'rx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤确认', detail_event: { payload: { cot: 7 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x02, 0x00, 0x64, 0x01, 0x07, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-05-29T12:00:01.300Z', direction: 'rx', frame_label: { i_frame: 'M_SP_NA_1' }, detail: '单点信息 IOA=101', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x12, 0x00, 0x00, 0x02, 0x00, 0x01, 0x01, 0x14, 0x00, 0x01, 0x00, 0x65, 0x00, 0x00, 0x01] },
  { timestamp: '2026-05-29T12:00:01.420Z', direction: 'rx', frame_label: { i_frame: 'M_ME_NC_1' }, detail: '短浮点测量值 IOA=107', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x14, 0x02, 0x00, 0x02, 0x00, 0x0d, 0x01, 0x14, 0x00, 0x01, 0x00, 0x6b, 0x00, 0xa4, 0x70, 0x49, 0x42, 0x00] },
  { timestamp: '2026-05-29T12:00:01.560Z', direction: 'rx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤结束', detail_event: { payload: { cot: 10 } }, raw_bytes: [0x68, 0x0e, 0x04, 0x00, 0x04, 0x00, 0x64, 0x01, 0x0a, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-05-29T12:00:05.880Z', direction: 'rx', frame_label: { i_frame: 'M_ME_NC_1' }, detail: '短浮点测量值 IOA=107 (变位)', detail_event: { payload: { cot: 3 } }, raw_bytes: [0x68, 0x14, 0x06, 0x00, 0x04, 0x00, 0x0d, 0x01, 0x03, 0x00, 0x01, 0x00, 0x6b, 0x00, 0xc3, 0xf5, 0x48, 0x42, 0x00] },
]

const slaveLogs = [
  { timestamp: '2026-08-06T02:18:30.010Z', direction: 'rx', frame_label: 'connection_event', detail: '主站已连接:192.168.10.21:52144', detail_event: null, raw_bytes: null },
  { timestamp: '2026-08-06T02:18:30.080Z', direction: 'rx', frame_label: 'u_start_act', detail: 'STARTDT ACT', detail_event: null, raw_bytes: [0x68, 0x04, 0x07, 0x00, 0x00, 0x00] },
  { timestamp: '2026-08-06T02:18:30.090Z', direction: 'tx', frame_label: 'u_start_con', detail: 'STARTDT CON', detail_event: null, raw_bytes: [0x68, 0x04, 0x0b, 0x00, 0x00, 0x00] },
  { timestamp: '2026-08-06T02:18:31.120Z', direction: 'rx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤命令 CA=1 (QOI=20)', detail_event: { payload: { cot: 6 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x64, 0x01, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-08-06T02:18:31.150Z', direction: 'tx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤确认 CA=1', detail_event: { payload: { cot: 7 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x02, 0x00, 0x64, 0x01, 0x07, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-08-06T02:18:31.180Z', direction: 'tx', frame_label: { i_frame: 'M_SP_NA_1' }, detail: '断路器合位 CA=1 IOA=1', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x0e, 0x02, 0x00, 0x02, 0x00, 0x01, 0x01, 0x14, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01] },
  { timestamp: '2026-08-06T02:18:31.240Z', direction: 'tx', frame_label: { i_frame: 'M_ME_NC_1' }, detail: '系统频率 CA=1 IOA=7 值=50.02', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x12, 0x04, 0x00, 0x02, 0x00, 0x0d, 0x01, 0x14, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x7b, 0x14, 0x48, 0x42, 0x00] },
  { timestamp: '2026-08-06T02:18:31.300Z', direction: 'tx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤结束 CA=1', detail_event: { payload: { cot: 10 } }, raw_bytes: [0x68, 0x0e, 0x06, 0x00, 0x02, 0x00, 0x64, 0x01, 0x0a, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
]

const multiCaLogs = [
  { timestamp: '2026-08-05T09:03:24.110Z', direction: 'tx', frame_label: 'connection_event', detail: 'TLS 握手中… 10.15.48.12:2404', detail_event: null, raw_bytes: null },
  { timestamp: '2026-08-05T09:03:24.690Z', direction: 'rx', frame_label: 'connection_event', detail: 'TLS 握手成功', detail_event: null, raw_bytes: null },
  { timestamp: '2026-08-05T09:03:24.710Z', direction: 'tx', frame_label: 'u_start_act', detail: 'STARTDT ACT → 10.15.48.12:2404 (TLS)', detail_event: null, raw_bytes: [0x68, 0x04, 0x07, 0x00, 0x00, 0x00] },
  { timestamp: '2026-08-05T09:03:24.735Z', direction: 'rx', frame_label: 'u_start_con', detail: 'STARTDT CON', detail_event: null, raw_bytes: [0x68, 0x04, 0x0b, 0x00, 0x00, 0x00] },
  { timestamp: '2026-08-05T09:03:25.310Z', direction: 'tx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤命令 CA=1 (QOI=20)', detail_event: { payload: { cot: 6 } }, raw_bytes: [0x68, 0x0e, 0x00, 0x00, 0x00, 0x00, 0x64, 0x01, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-08-05T09:03:25.336Z', direction: 'tx', frame_label: { i_frame: 'C_IC_NA_1' }, detail: '总召唤命令 CA=2 (QOI=20)', detail_event: { payload: { cot: 6 } }, raw_bytes: [0x68, 0x0e, 0x02, 0x00, 0x00, 0x00, 0x64, 0x01, 0x06, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x14] },
  { timestamp: '2026-08-05T09:03:25.390Z', direction: 'rx', frame_label: { i_frame: 'M_SP_NA_1' }, detail: '单点信息 CA=1 IOA=101', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x12, 0x00, 0x00, 0x02, 0x00, 0x01, 0x01, 0x14, 0x00, 0x01, 0x00, 0x65, 0x00, 0x00, 0x01] },
  { timestamp: '2026-08-05T09:03:25.420Z', direction: 'rx', frame_label: { i_frame: 'M_ME_NC_1' }, detail: '短浮点测量值 CA=2 IOA=107', detail_event: { payload: { cot: 20 } }, raw_bytes: [0x68, 0x14, 0x02, 0x00, 0x02, 0x00, 0x0d, 0x01, 0x14, 0x00, 0x02, 0x00, 0x6b, 0x00, 0xa4, 0x70, 0x49, 0x42, 0x00] },
]

const slaveServers = [{
  id: 's1', bind_address: '0.0.0.0', port: 2404, state: 'Running', station_count: 1,
  client_count: 0, use_tls: false,
}]

const slaveServersWithMasters = [{ ...slaveServers[0], client_count: 2 }]

const clientConnections = [
  { peer_address: '192.168.10.21:52144', data_transfer_active: true },
  { peer_address: '10.20.0.15:49876', data_transfer_active: false },
]

const activeRandomMutations = [
  { ioa: 7, asdu_type: 'M_ME_NC_1', mode: 'random', period_ms: 1000, step: 1, min: 49.8, max: 50.2 },
]

const activePointValues = [
  { ioa: 7, asdu_type: 'M_ME_NC_1', value: '50.02', ...q, timestamp: null },
]

// ----------------------------------------------------------------------------
// IPC stub — runs in the browser BEFORE the app boots (no closure over Node).
// ----------------------------------------------------------------------------
function installTauriMock(cfg) {
  try { localStorage.setItem('iec104.locale', cfg.locale) } catch (e) { /* ignore */ }
  // Pin the dark theme so captures don't follow the host OS preference.
  try { localStorage.setItem('iec104-theme', 'dark') } catch (e) { /* ignore */ }
  try {
    for (const [key, value] of Object.entries(cfg.storage || {})) localStorage.setItem(key, value)
  } catch (e) { /* ignore */ }
  const DATA = cfg.commands || {}
  // these two return an incremental {seq,total_count,points} envelope; once the
  // poller has caught up (sinceSeq >= seq) return an empty delta so it stays put.
  const INCREMENTAL = ['list_data_points_since', 'get_received_data_since']
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      if (cmd.indexOf('plugin:event|') === 0) return 1 // listen()/unlisten() event id
      if (cmd === 'plugin:app|version') return cfg.version
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

const masterMultiConnected = {
  list_connections: [multiCaConn],
  get_received_data_since: { seq: 323, total_count: multiCaPoints.length, points: multiCaPoints },
  get_communication_logs: multiCaLogs,
  check_for_update: null,
  set_logging_enabled: null,
}

const multiCaForm = JSON.stringify({
  target_address: '10.15.48.12',
  port: 2404,
  common_addresses_text: '1, 2, 3',
  broadcast_address_hex: 'FFFF',
})

const socks5Form = JSON.stringify({
  target_address: 'rtu.example.com',
  port: 2404,
  common_addresses_text: '1',
  broadcast_address_hex: 'FFFF',
  use_socks5: true,
  socks5_proxy_address: 'proxy.example.com',
  socks5_proxy_port: 1080,
  socks5_username: 'alice',
  socks5_password: 'example-secret',
  socks5_remote_dns: true,
})

const shots = [
  {
    name: 'master-multi-ca-newconn',
    url: MASTER,
    cfg: {
      locale: 'zh-CN',
      storage: { 'iec104master.newConnForm.v2': multiCaForm },
      commands: masterMultiConnected,
    },
    async act(page) {
      await page.getByText('10.15.48.12', { exact: false }).first().click()
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 20, { timeout: 6000 })
      await page.waitForTimeout(3600) // wait for the initial category-change pulse to settle
      await page.getByText('新建连接', { exact: false }).first().click()
      await page.locator('.modal-box').waitFor({ timeout: 6000 })
      await page.waitForTimeout(700)
    },
  },
  {
    name: 'master-socks5-connection',
    url: MASTER,
    viewport: { width: 1280, height: 800 },
    cfg: {
      locale: 'zh-CN',
      storage: { 'iec104master.newConnForm.v2': socks5Form },
      commands: {
        list_connections: [],
        get_received_data_since: { seq: 0, total_count: 0, points: [] },
        get_communication_logs: [],
        check_for_update: null,
        set_logging_enabled: null,
      },
    },
    async act(page) {
      await page.getByText('新建连接', { exact: false }).first().click()
      await page.locator('.proxy-fields').waitFor({ timeout: 6000 })
      await page.getByText('由代理远程解析目标域名', { exact: false }).scrollIntoViewIfNeeded()
      await page.waitForTimeout(700)
    },
  },
  {
    name: 'master-multi-ca-comm-log',
    url: MASTER,
    viewport: { width: 1600, height: 900 },
    cfg: {
      locale: 'zh-CN',
      storage: { 'iec104.logPanel.height': '390' },
      commands: masterMultiConnected,
    },
    async act(page) {
      await page.getByText('10.15.48.12', { exact: false }).first().click()
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 20, { timeout: 6000 })
      await page.waitForTimeout(3600) // wait for the initial category-change pulse to settle
      await page.getByText('通信日志', { exact: false }).first().click()
      await page.locator('.log-panel table tbody tr').first().waitFor({ timeout: 6000 })
      await page.waitForTimeout(700)
    },
  },
  {
    name: 'slave-point-csv-log-analysis',
    url: SLAVE,
    viewport: { width: 1600, height: 900 },
    cfg: {
      locale: 'zh-CN',
      storage: { 'iec104.layout.logHeight': '390' },
      commands: {
        list_servers: slaveServersWithMasters,
        list_stations: [{ common_address: 1, name: '220kV 东站', point_count: slavePoints.length }],
        list_data_points_since: { seq: 100, total_count: slavePoints.length, points: slavePoints },
        list_client_connections: clientConnections,
        list_point_mutations: activeRandomMutations,
        get_data_point_values: activePointValues,
        get_communication_logs: slaveLogs,
        check_for_update: null,
      },
    },
    async act(page) {
      await page.getByText('220kV 东站', { exact: false }).first().click()
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 10, { timeout: 6000 })
      await page.waitForTimeout(700)
      await page.getByText('通信日志', { exact: false }).first().click()
      await page.locator('.log-panel table tbody tr').first().waitFor({ timeout: 6000 })
      await page.locator('.dir-filter').selectOption('tx')
      await page.locator('.frame-filter').selectOption('kind:i')
      await page.locator('.log-search').fill('CA=1')
      await page.getByText('4 / 8', { exact: true }).waitFor({ timeout: 6000 })
      await page.waitForTimeout(700)
    },
  },
  {
    name: 'slave-random-simulation',
    url: SLAVE,
    viewport: { width: 1600, height: 900 },
    cfg: { locale: 'zh-CN', commands: {
      list_servers: slaveServersWithMasters,
      list_stations: [{ common_address: 1, name: '220kV 东站', point_count: slavePoints.length }],
      list_data_points_since: { seq: 100, total_count: slavePoints.length, points: slavePoints },
      list_client_connections: clientConnections,
      list_point_mutations: activeRandomMutations,
      get_data_point_values: activePointValues,
      get_communication_logs: [],
      check_for_update: null,
    } },
    async act(page) {
      await page.getByText('220kV 东站', { exact: false }).first().click()
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 10, { timeout: 6000 })
      await page.getByText('系统频率', { exact: true }).click()
      await page.locator('.mut-mode').waitFor({ timeout: 6000 })
      await page.getByText('模拟设置', { exact: true }).click()
      await page.locator('.sim-drawer').waitFor({ timeout: 6000 })
      await page.getByRole('button', { name: '随机', exact: true }).waitFor({ timeout: 6000 })
      await page.waitForTimeout(700)
    },
  },
  {
    name: 'slave-master-connections',
    url: SLAVE,
    viewport: { width: 1600, height: 900 },
    cfg: { locale: 'zh-CN', commands: {
      list_servers: slaveServersWithMasters,
      list_stations: [{ common_address: 1, name: '220kV 东站', point_count: slavePoints.length }],
      list_data_points_since: { seq: 100, total_count: slavePoints.length, points: slavePoints },
      list_client_connections: clientConnections,
      list_point_mutations: activeRandomMutations,
      get_data_point_values: activePointValues,
      get_communication_logs: [],
      check_for_update: null,
    } },
    async act(page) {
      await page.getByText('220kV 东站', { exact: false }).first().click()
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 10, { timeout: 6000 })
      await page.locator('.client-count-badge').click()
      await page.locator('[role="dialog"] table tbody tr').first().waitFor({ timeout: 6000 })
      await page.waitForTimeout(600)
    },
  },
  {
    name: 'tut-1-slave-current-main',
    url: SLAVE,
    viewport: { width: 1600, height: 900 },
    cfg: { locale: 'zh-CN', commands: {
      list_servers: slaveServers,
      list_stations: [{ common_address: 1, name: '220kV 东站', point_count: slavePoints.length }],
      list_data_points_since: { seq: 100, total_count: slavePoints.length, points: slavePoints },
      list_point_mutations: activeRandomMutations,
      get_data_point_values: activePointValues,
      get_communication_logs: [],
      check_for_update: null,
    } },
    async act(page) {
      await page.getByText('220kV 东站', { exact: false }).first().click()
      await page.waitForFunction(() => document.querySelectorAll('table tbody tr').length >= 10, { timeout: 6000 })
      await page.getByText('系统频率', { exact: true }).click()
      await page.locator('.mut-mode').waitFor({ timeout: 6000 })
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
      await page.waitForTimeout(3600) // 等首轮数据触发的分类变位闪烁(3s)退去
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
      await page.waitForTimeout(3600) // 等首轮数据触发的分类变位闪烁(3s)退去
    },
  },
]

// ----------------------------------------------------------------------------
const browser = await chromium.launch()
try {
  for (const shot of shots) {
    // locale drives navigator.language, which the i18n detector falls back to;
    // addInitScript's localStorage.setItem can't run on the pre-navigation origin.
    const ctx = await browser.newContext({ viewport: shot.viewport ?? { width: 1200, height: 800 }, deviceScaleFactor: 1, locale: shot.cfg.locale })
    const page = await ctx.newPage()
    await page.addInitScript(installTauriMock, { ...shot.cfg, version: APP_VERSION })
    await page.goto(shot.url, { waitUntil: 'domcontentloaded' })
    await page.addStyleTag({ content: '*, *::before, *::after { animation: none !important; transition: none !important; caret-color: transparent !important; }' })
    await page.waitForFunction((bg) => getComputedStyle(document.body).backgroundColor === bg, DARK_BG, { timeout: 8000 }).catch(() => {})
    await page.getByText(`v${APP_VERSION}`, { exact: true }).waitFor({ timeout: 6000 })
    await shot.act(page)
    await page.screenshot({ path: resolve(OUT_DIR, shot.name + '.png') })
    await ctx.close()
    console.log('✓', shot.name)
  }
} finally {
  await browser.close()
}
