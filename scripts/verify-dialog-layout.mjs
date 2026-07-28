/**
 * 小视口对话框布局回归校验(issue #28 —— "标题/底部按钮被内容顶走或裁掉")。
 *
 * 纯 CSS 的 flex + overflow 布局 jsdom 测不出来(不计算布局),所以这里用真实无头
 * Chromium 逐个挂载对话框组件,在【小视口高度】下断言:
 *   1. 对话框外框完整落在视口内(没有被裁掉的部分);
 *   2. 标题可见且在视口内;
 *   3. 底部主按钮可见、在视口内、且真的可点击(hit-test 通过);
 *   4. 正文 scrollHeight > clientHeight 时正文可滚动,且滚到底后标题/按钮位置不动。
 *
 * 组件是从 dev server 的模块图里动态 import 后独立挂载的(全局 CSS 变量/tokens 由
 * 宿主页面提供),所以连当前没有被任何页面引用的对话框(RawSendDialog)也能覆盖。
 *
 * 前置条件
 *   1. 依赖已装: npm --prefix frontend ci && npm --prefix master-frontend ci && npm --prefix scripts ci
 *   2. 两个 dev server 在跑(仓库根目录,两个终端):
 *        npm --prefix frontend run dev          # 子站 -> http://localhost:5176
 *        npm --prefix master-frontend run dev   # 主站 -> http://localhost:5177
 *   3. Chromium: npx --prefix scripts playwright install chromium
 *
 * 运行
 *        node scripts/verify-dialog-layout.mjs
 *        node scripts/verify-dialog-layout.mjs --only=NewServerModal,ControlDialog
 *        node scripts/verify-dialog-layout.mjs --headed --shots=/tmp/dlg   # 存截图便于肉眼复核
 *        SLAVE_URL=http://localhost:5186/ node scripts/verify-dialog-layout.mjs  # dev server 换了端口
 *
 * 退出码 0 = 全部通过;非 0 = 有断言失败(失败明细打印在 stdout)。
 */
import { chromium } from 'playwright'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mkdirSync } from 'node:fs'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO = resolve(HERE, '..')
const SHARED_FS = '/@fs' + resolve(REPO, 'shared-frontend')
const USE_DIALOG_URL = SHARED_FS + '/composables/useDialog.ts'

// 端口可覆盖:同一台机器上可能有别的 worktree 占着 5176/5177,那时它的 dev server
// 不会 serve 本 worktree 的 /@fs 路径,用例会整片 "Failed to fetch dynamically imported module"。
// 换端口跑: npm --prefix frontend run dev -- --port 5186
//           SLAVE_URL=http://localhost:5186/ node scripts/verify-dialog-layout.mjs
const APPS = {
  slave: process.env.SLAVE_URL || 'http://localhost:5176/',
  master: process.env.MASTER_URL || 'http://localhost:5177/',
}

// 宿主页面自己也会拉一轮数据;不给桩它会在控制台抛错,污染"页面 JS 报错"这条断言。
const APP_DEFAULT_COMMANDS = {
  slave: {
    list_servers: [], list_stations: [],
    list_data_points_since: { seq: 0, total_count: 0, points: [] },
    get_communication_logs: [], check_for_update: null,
  },
  master: {
    list_connections: [],
    get_received_data_since: { seq: 0, total_count: 0, points: [] },
    get_communication_logs: [], check_for_update: null, set_logging_enabled: null,
  },
}

// 小视口:1280x520 ≈ 1366x768 在 125% 缩放下的可用高度;1366x600 为略宽松的一档。
const VIEWPORTS = [
  { width: 1280, height: 520 },
  { width: 1366, height: 600 },
]

const argv = process.argv.slice(2)
const only = (argv.find((a) => a.startsWith('--only=')) || '').slice(7).split(',').filter(Boolean)
const shotsDir = (argv.find((a) => a.startsWith('--shots=')) || '').slice(8)
const headed = argv.includes('--headed')
if (shotsDir) mkdirSync(shotsDir, { recursive: true })

// ---------------------------------------------------------------------------
// 假数据 / IPC 桩
// ---------------------------------------------------------------------------
const q = { ov: false, bl: false, sb: false, nt: false, iv: false }
const ts = (s) => ({ year: 2026, month: 7, day: 26, day_of_week: 0, hour: 12, minute: 0, millisecond: s * 1000, invalid: false, summer_time: false })

const parsedFrame = {
  raw_hex: '68 2E 00 00 02 00 0D 06 14 00 01 00',
  length: 48,
  start_byte: 0x68,
  apdu_length: 46,
  control_field: [0x00, 0x00, 0x02, 0x00],
  apci: { frame_type: 'i', send_seq: 0, recv_seq: 1 },
  asdu: {
    type_id: 13, type_name: 'M_ME_NC_1', sq: false, num_objects: 6, cot: 20,
    cot_name: 'interrogated_by_station', negative: false, test: false, originator: 0, common_address: 1,
    objects: Array.from({ length: 6 }, (_, i) => ({
      ioa: 101 + i,
      value: { type: 'short_float', value: 50.02 + i },
      quality: { ...q },
      timestamp: ts(i),
      raw_hex: '65 00 A4 70 49 42 00',
    })),
  },
  warnings: ['示例告警:声明长度与实际字节数不一致', '示例告警:COT 与类型标识组合非常规'],
}

const slavePoints = [
  { ioa: 1, asdu_type: 'M_SP_NA_1', category: 'single_point' },
  { ioa: 2, asdu_type: 'M_DP_NA_1', category: 'double_point' },
  { ioa: 3, asdu_type: 'M_ME_NC_1', category: 'float_measured' },
  { ioa: 4, asdu_type: 'C_SC_NA_1', category: 'single_command' },
]

const controlPoints = slavePoints.map((p) => ({
  ...p, name: '点位 ' + p.ioa, comment: '', value: '0', timestamp: null,
  quality_ov: false, quality_bl: false, quality_sb: false, quality_nt: false, quality_iv: false,
}))

// 多 CA 的连接:让 ControlDialog 走 <select> 分支(比 number input 稍高)
const masterConn = {
  id: 'c1', target_address: '127.0.0.1', port: 2404, common_addresses: [1, 2, 3], state: 'Connected', use_tls: false,
  t0: 30, t1: 15, t2: 10, t3: 20, k: 12, w: 8, default_qoi: 20, default_qcc: 6,
  interrogate_period_s: 60, counter_interrogate_period_s: 60, broadcast_address: 65535,
}

const LONG_ERR ='发送失败: 连接 c1 当前不可用 —— 底层 TCP 写入返回 os error 32 (Broken pipe),' +
  '请检查子站是否仍在监听、以及 k/w 窗口是否已被未确认的 I 帧填满导致发送阻塞。'

// ---------------------------------------------------------------------------
// 用例
// ---------------------------------------------------------------------------
/**
 * act 里一律用 dispatchEvent / 直接改 DOM 触发交互,而不是 Playwright 的 click():
 * 未修复状态下按钮/复选框可能落在视口外,click() 会先滚动或直接超时,那样断言还没跑
 * 就挂了 —— 我们要的是"能构造出长内容,再检查布局",所以交互必须绕过可见性检查。
 */
const CASES = [
  {
    name: 'NewServerModal',
    app: 'slave',
    module: '/src/components/NewServerModal.vue',
    props: {},
    provideDialog: true,
    box: '.modal-box',
    title: '.modal-title',
    primary: '.modal-btn.confirm',
    body: '.modal-body',
    expectOverflow: true,
    commands: { list_bind_address_suggestions: ['0.0.0.0', '127.0.0.1', '192.168.1.20'] },
    // 勾上 Enable TLS,展开到 9 个字段(约 650 CSS px)
    async act(page) {
      await clickAll(page, '.modal-field input[type="checkbox"]')
    },
  },
  {
    name: 'DataPointModal',
    app: 'slave',
    module: '/src/components/DataPointModal.vue',
    props: { serverId: 's1', commonAddress: 1, point: null, category: null },
    provideDialog: true,
    box: '.modal',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    commands: {},
  },
  {
    name: 'BatchAddModal',
    app: 'slave',
    module: '/src/components/BatchAddModal.vue',
    props: { serverId: 's1', commonAddress: 1, category: null, existingPoints: slavePoints },
    provideDialog: true,
    box: '.modal',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    expectOverflow: true,
    commands: {},
  },
  {
    name: 'BatchControlOptionsModal',
    app: 'slave',
    module: '/src/components/BatchControlOptionsModal.vue',
    props: { serverId: 's1', commonAddress: 1, points: controlPoints },
    provideDialog: true,
    box: '.modal',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    commands: {},
    // 两个开关都打开 -> 两组单选全部展开
    async act(page) {
      await clickAll(page, '.modal-body .check-item input[type="checkbox"]')
      await clickAll(page, '.modal-body .radio-item input[value="custom"]')
    },
  },
  {
    name: 'BatchWriteModal',
    app: 'slave',
    module: '/src/components/BatchWriteModal.vue',
    props: { serverId: 's1', commonAddress: 1, existingPoints: slavePoints, defaultType: 'M_ME_NC_1' },
    provideDialog: true,
    box: '.modal',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    commands: {},
    async act(page) {
      await setFieldValue(page, '.modal-body input[type="text"], .modal-body textarea', '1-4,10-20,100-200')
    },
  },
  {
    name: 'ParseFrameDialog',
    app: 'slave',
    module: SHARED_FS + '/components/ParseFrameDialog.vue',
    props: { prefill: '68 2E 00 00 02 00 0D 06 14 00 01 00' },
    box: '.modal-box',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    expectOverflow: true,
    commands: { parse_frame_full: parsedFrame },
  },
  {
    name: 'ControlDialog',
    app: 'master',
    module: '/src/components/ControlDialog.vue',
    props: { connectionId: 'c1', commonAddress: 1, prefillIoa: 101, prefillCommandType: 'setpoint_float' },
    box: '.modal-box',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    expectOverflow: true,
    commands: { list_connections: [masterConn], send_control_command: { __throw: LONG_ERR } },
    // 展开高级区 + 触发错误提示 -> setpoint 形态下最高的一版
    async act(page) {
      await clickAll(page, '.advanced-summary')
      await clickAll(page, '.modal-footer .btn-primary')
      await page.waitForSelector('.error-msg', { state: 'attached', timeout: 3000 })
    },
  },
  {
    name: 'RawSendDialog',
    app: 'master',
    module: '/src/components/RawSendDialog.vue',
    props: { connectionId: 'c1' },
    box: '.modal-box',
    title: '.modal-title',
    primary: '.modal-footer .btn-primary',
    body: '.modal-body',
    expectOverflow: true,
    commands: {
      send_raw_apdu: {
        byte_len: 240, timestamp: '2026-07-26T12:00:00.123Z',
        // 故意很长:break-all 折行后把结果块撑到十几二十行,模拟"发了个长 ASDU"的真实高度
        sent_hex: Array.from({ length: 40 }, () => '68 0E 00 00 02 00 64 01 06 00 01 00 00 00 00 14').join(' '),
      },
    },
    // 填报文 -> 解析预览 -> 发送(渲染出结果块),把模板按钮下面的内容全撑开
    async act(page) {
      await setFieldValue(page, '.modal-body .hex-area', '68 0E 00 00 00 00 64 01 06 00 01 00 00 00 00 14')
      await clickAll(page, '.preview-row .btn-secondary')
      await clickAll(page, '.modal-footer .btn-primary')
      await page.waitForSelector('.result-ok', { state: 'attached', timeout: 3000 })
    },
  },
]

// ---------------------------------------------------------------------------
// 浏览器内执行的两段代码(不能闭包 Node 变量)
// ---------------------------------------------------------------------------
function installTauriMock(cfg) {
  try { localStorage.setItem('iec104.locale', 'zh-CN') } catch (e) { /* ignore */ }
  const DATA = cfg.commands || {}
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd) => {
      if (cmd.indexOf('plugin:event|') === 0) return 1
      if (!(cmd in DATA)) return null
      const v = DATA[cmd]
      if (v && typeof v === 'object' && '__throw' in v) throw v.__throw
      return v
    },
    transformCallback: (cb) => { const id = Math.floor(Math.random() * 1e9); window['_cb' + id] = cb; return id },
    unregisterCallback: () => {},
    convertFileSrc: (p) => p,
  }
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
}

/** 从 dev server 拿组件模块并独立挂载;visible 由 window.__dlgOpen() 翻成 true,走真实"打开"路径。 */
async function mountDialog(cfg) {
  const src = await (await fetch(cfg.module)).text()
  const m = src.match(/"(\/node_modules\/\.vite\/deps\/vue\.js[^"]*)"/)
  const vueMod = await import(m ? m[1] : '/node_modules/.vite/deps/vue.js')
  const vue = vueMod.createApp ? vueMod : vueMod.default
  const mod = await import(cfg.module)

  const host = document.createElement('div')
  host.id = 'dlg-harness'
  document.body.appendChild(host)

  const visible = vue.ref(false)
  const app = vue.createApp({
    render: () => vue.h(mod.default, { ...cfg.props, visible: visible.value, 'onUpdate:visible': () => {} }),
  })
  if (cfg.provideDialog) {
    const d = await import(cfg.useDialogUrl)
    app.provide(d.dialogKey, { showAlert: d.showAlert, showConfirm: d.showConfirm, showPrompt: d.showPrompt })
  }
  app.provide('refreshTree', () => {})
  app.mount(host)
  window.__dlgOpen = async () => { visible.value = true; await vue.nextTick() }
  await window.__dlgOpen()
}

// ---------------------------------------------------------------------------
// 交互工具:绕过可见性检查(未修复状态下元素可能在视口外)
// ---------------------------------------------------------------------------
async function clickAll(page, selector) {
  await page.evaluate((sel) => {
    document.querySelectorAll(sel).forEach((el) => el.dispatchEvent(new MouseEvent('click', { bubbles: true })))
  }, selector)
  await page.waitForTimeout(120)
}

async function setFieldValue(page, selector, value) {
  await page.evaluate(([sel, val]) => {
    const el = document.querySelector(sel)
    if (!el) return
    el.value = val
    el.dispatchEvent(new Event('input', { bubbles: true }))
    el.dispatchEvent(new Event('change', { bubbles: true }))
  }, [selector, value])
  await page.waitForTimeout(120)
}

// ---------------------------------------------------------------------------
// 断言
// ---------------------------------------------------------------------------
const TOL = 1 // px 容差

function inViewport(rect, vh) {
  return rect.top >= -TOL && rect.bottom <= vh + TOL && rect.height > 0
}

async function measure(page, c) {
  return page.evaluate((sel) => {
    const pick = (s) => document.querySelector(s)
    const r = (el) => {
      if (!el) return null
      const b = el.getBoundingClientRect()
      const cs = getComputedStyle(el)
      return {
        top: b.top, bottom: b.bottom, height: b.height, width: b.width,
        visible: cs.display !== 'none' && cs.visibility !== 'hidden' && b.height > 0,
        scrollHeight: el.scrollHeight, clientHeight: el.clientHeight, overflowY: cs.overflowY,
      }
    }
    const bodyEl = pick(sel.body)
    return {
      count: document.querySelectorAll(sel.box).length,
      box: r(pick(sel.box)),
      title: r(pick(sel.title)),
      primary: r(pick(sel.primary)),
      body: r(bodyEl),
      vh: window.innerHeight,
    }
  }, { box: c.box, title: c.title, primary: c.primary, body: c.body })
}

async function runCase(browser, c, vp) {
  const failures = []
  const notes = []
  const result = { failures, notes, overflow: false }
  const ctx = await browser.newContext({ viewport: vp, locale: 'zh-CN', deviceScaleFactor: 1 })
  const page = await ctx.newPage()
  const pageErrors = []
  page.on('pageerror', (e) => pageErrors.push(e.message))
  try {
    await page.addInitScript(installTauriMock, { commands: { ...APP_DEFAULT_COMMANDS[c.app], ...(c.commands || {}) } })
    await page.goto(APPS[c.app], { waitUntil: 'domcontentloaded' })
    await page.waitForFunction(() => getComputedStyle(document.body).backgroundColor !== 'rgba(0, 0, 0, 0)', null, { timeout: 10000 }).catch(() => {})
    await page.evaluate(mountDialog, { module: c.module, props: c.props || {}, provideDialog: !!c.provideDialog, useDialogUrl: USE_DIALOG_URL })
    await page.waitForSelector(c.box, { state: 'attached', timeout: 5000 })
    await page.waitForTimeout(350) // dialog-pop 过渡
    if (c.act) await c.act(page)
    await page.waitForTimeout(200)

    const m = await measure(page, c)
    if (m.count !== 1) failures.push(`期望页面上恰好 1 个 ${c.box},实际 ${m.count}`)
    if (!m.box) failures.push(`找不到外框 ${c.box}`)
    if (!m.title) failures.push(`找不到标题 ${c.title}`)
    if (!m.primary) failures.push(`找不到主按钮 ${c.primary}`)
    // 正文容器缺失本身就是缺陷(未修复的对话框根本没有滚动容器),但不阻断其余几何断言
    if (!m.body) failures.push(`找不到正文滚动容器 ${c.body} —— 内容超高只会被裁掉,没有滚动条`)
    if (!m.box || !m.title || !m.primary) throw new Error(failures.join('; '))

    // 1) 外框整体在视口内
    if (!inViewport(m.box, m.vh)) {
      failures.push(`外框超出视口: top=${m.box.top.toFixed(1)} bottom=${m.box.bottom.toFixed(1)} 高=${m.box.height.toFixed(1)} 视口高=${m.vh}`)
    }
    // 2) 标题可见且在视口内
    if (!m.title.visible) failures.push('标题不可见')
    if (!inViewport(m.title, m.vh)) {
      failures.push(`标题不在视口内: top=${m.title.top.toFixed(1)} bottom=${m.title.bottom.toFixed(1)} 视口高=${m.vh}`)
    }
    // 3) 主按钮可见、在视口内、可点击
    if (!m.primary.visible) failures.push('底部主按钮不可见')
    if (!inViewport(m.primary, m.vh)) {
      failures.push(`底部主按钮不在视口内: top=${m.primary.top.toFixed(1)} bottom=${m.primary.bottom.toFixed(1)} 视口高=${m.vh}`)
    } else {
      const hit = await page.evaluate((sel) => {
        const el = document.querySelector(sel)
        const b = el.getBoundingClientRect()
        const at = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2)
        return !!at && (at === el || el.contains(at))
      }, c.primary)
      if (!hit) failures.push('底部主按钮中心点被其他元素遮挡,点不到')
    }

    // 4) 正文滚动:内容溢出时必须由正文自己滚,且滚动后标题/按钮不动
    const overflow = !!m.body && m.body.scrollHeight > m.body.clientHeight + TOL
    result.overflow = overflow
    if (overflow) {
      if (m.body.overflowY !== 'auto' && m.body.overflowY !== 'scroll') {
        failures.push(`正文溢出但 overflow-y=${m.body.overflowY},不会出现滚动条`)
      }
      const scrolled = await page.evaluate((sel) => {
        const el = document.querySelector(sel)
        el.scrollTop = el.scrollHeight
        return el.scrollTop
      }, c.body)
      if (scrolled <= 0) failures.push('正文溢出但滚不动 (scrollTop 归零)')
      const after = await measure(page, c)
      if (Math.abs(after.title.top - m.title.top) > TOL) {
        failures.push(`正文滚到底后标题跟着动了: ${m.title.top.toFixed(1)} -> ${after.title.top.toFixed(1)}`)
      }
      if (Math.abs(after.primary.top - m.primary.top) > TOL) {
        failures.push(`正文滚到底后底部按钮跟着动了: ${m.primary.top.toFixed(1)} -> ${after.primary.top.toFixed(1)}`)
      }
      if (!inViewport(after.title, after.vh)) failures.push('正文滚到底后标题离开了视口')
      if (!inViewport(after.primary, after.vh)) failures.push('正文滚到底后底部按钮离开了视口')
      notes.push(`正文可滚 (${m.body.clientHeight}/${m.body.scrollHeight}px)`)
    } else if (m.body) {
      notes.push(`内容未溢出 (${m.body.scrollHeight}px)`)
    }
    notes.push(`外框高 ${m.box.height.toFixed(0)}px`)

    if (shotsDir) {
      await page.screenshot({ path: resolve(shotsDir, `${c.name}-${vp.width}x${vp.height}.png`) })
    }
  } catch (e) {
    failures.push('异常: ' + e.message)
  } finally {
    if (pageErrors.length) failures.push('页面 JS 报错: ' + pageErrors.join(' | '))
    await ctx.close()
  }
  return result
}

// ---------------------------------------------------------------------------
async function main() {
  for (const [name, url] of Object.entries(APPS)) {
    const ok = await fetch(url).then((r) => r.ok).catch(() => false)
    if (!ok) {
      console.error(`dev server 未就绪: ${name} ${url}\n先跑: npm --prefix ${name === 'slave' ? 'frontend' : 'master-frontend'} run dev`)
      process.exit(2)
    }
  }

  const cases = only.length ? CASES.filter((c) => only.includes(c.name)) : CASES
  if (!cases.length) { console.error('--only 没匹配到任何用例'); process.exit(2) }

  const browser = await chromium.launch({ headless: !headed })
  let failed = 0
  const sawOverflow = new Set()
  try {
    for (const vp of VIEWPORTS) {
      console.log(`\n=== 视口 ${vp.width}x${vp.height} ===`)
      for (const c of cases) {
        const { failures, notes, overflow } = await runCase(browser, c, vp)
        if (overflow) sawOverflow.add(c.name)
        const tag = `${c.name} [${c.app}]`
        if (failures.length) {
          failed += failures.length
          console.log(`✗ ${tag}`)
          for (const f of failures) console.log(`    - ${f}`)
        } else {
          console.log(`✓ ${tag}  ${notes.join(', ')}`)
        }
      }
    }
  } finally {
    await browser.close()
  }

  // 元断言:标记了 expectOverflow 的用例,至少要在一个视口里真的把正文撑到溢出,
  // 否则"正文可滚 + 标题/按钮不动"这组断言等于没跑,布局回归会被静默放过。
  console.log('\n=== 溢出覆盖检查 ===')
  for (const c of cases.filter((x) => x.expectOverflow)) {
    if (sawOverflow.has(c.name)) {
      console.log(`✓ ${c.name} 至少在一个视口触发了正文滚动`)
    } else {
      failed += 1
      console.log(`✗ ${c.name} 在所有视口都没触发正文滚动 —— 要么布局没把滚动收进正文,要么用例内容不够长`)
    }
  }
  console.log(failed ? `\n失败断言 ${failed} 条` : '\n全部通过')
  process.exit(failed ? 1 : 0)
}

await main()
