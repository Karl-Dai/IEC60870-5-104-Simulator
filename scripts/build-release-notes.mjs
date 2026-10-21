#!/usr/bin/env node
// Generate a richly formatted GitHub Release body for a given tag.
// Usage: node scripts/build-release-notes.mjs <tag>     # writes RELEASE_BODY.md
//
// Produces:
//   - Header (version + date)
//   - Per-OS download table (slave / master)
//   - Verbatim CHANGELOG section for the version
//   - Footer with links to full changelog + previous releases
//
// Designed to be called from CI after build jobs upload assets, so the
// rendered table always lines up with what's actually in the release.

import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { extractChangelogSection } from './gen-update-manifest.mjs'

const REPO = 'Karl-Dai/IEC60870-5-104-Simulator'
const REPO_URL = `https://github.com/${REPO}`

const MACOS_FIRST_LAUNCH_NOTE = [
  '## macOS 首次启动 / First launch on macOS',
  '',
  '首次双击 `.app` 会被 Gatekeeper 拦截("Apple 无法验证…")。放行:打开',
  '*系统设置 → 隐私与安全性*,滚到底点 *仍要打开*;或终端执行',
  '`xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"`。',
  `详细步骤见 [README](${REPO_URL}#first-launch-on-macos)。`,
  '',
  'First launch is blocked by Gatekeeper ("Apple could not verify…"). To allow:',
  '*System Settings → Privacy & Security*, scroll to bottom, click *Open Anyway*; or',
  'run `xattr -dr com.apple.quarantine "/Applications/IEC104Slave.app"` in Terminal.',
  `Full steps in the [README](${REPO_URL}#first-launch-on-macos).`,
].join('\n')

// 国内用户首次从旧版升级时,updater 仍指向 github.com,大概率拉不到;
// 引导他们直接走镜像下载安装包。新版本起 updater 已带 proxy fallback。
const CN_MIRROR_BANNER = [
  '> 🇨🇳 **中国大陆用户**:首次从旧版升级若失败,请直接从镜像下载安装包(新版本起更新检查会自动走 proxy):',
  '>',
  `> <https://ghfast.top/${REPO_URL}/releases/latest>`,
  '>',
  '> 🌍 **Users in mainland China**: if the in-app updater fails on first upgrade from a previous version, download installers from the mirror above (later versions will auto-fallback through proxies).',
].join('\n')

const APPS = [
  { role: 'Slave (从站)', prefix: 'IEC104Slave' },
  { role: 'Master (主站)', prefix: 'IEC104Master' },
]

const PLATFORMS = [
  { label: 'macOS Apple Silicon', file: (p, v) => `${p}_${v}_aarch64.dmg` },
  { label: 'macOS Intel',         file: (p, v) => `${p}_${v}_x64.dmg` },
  { label: 'Windows x64 (NSIS)',  file: (p, v) => `${p}_${v}_x64-setup.exe` },
  { label: 'Windows x64 (MSI)',   file: (p, v) => `${p}_${v}_x64_en-US.msi` },
  { label: 'Windows x64 (Portable)', file: (p, v) => `${p}_${v}_x64-portable.exe` },
  { label: 'Windows ARM64 (NSIS)',  file: (p, v) => `${p}_${v}_arm64-setup.exe` },
  { label: 'Windows ARM64 (MSI)',   file: (p, v) => `${p}_${v}_arm64_en-US.msi` },
  { label: 'Windows ARM64 (Portable)', file: (p, v) => `${p}_${v}_arm64-portable.exe` },
  { label: 'Linux AppImage',      file: (p, v) => `${p}_${v}_amd64.AppImage` },
  { label: 'Linux deb',           file: (p, v) => `${p}_${v}_amd64.deb` },
  { label: 'Linux rpm',           file: (p, v) => `${p === 'IEC104Slave' ? 'IEC104Slave' : 'IEC104Master'}-${v}-1.x86_64.rpm` },
]

export function buildBody(tag, changelog) {
  const version = tag.replace(/^v/, '')
  const section = extractChangelogSection(changelog, version)

  const lines = []
  lines.push(`# IEC60870-5-104 Simulator ${tag}`)
  lines.push('')
  lines.push(CN_MIRROR_BANNER)
  lines.push('')
  lines.push('## 下载 / Downloads')
  lines.push('')
  lines.push('按平台点击 ⬇️ 即可下载安装包 / Click ⬇️ for your platform to download the installer:')
  lines.push('')
  lines.push(`| 平台 / Platform | ${APPS[0].role} | ${APPS[1].role} |`)
  lines.push('|---|---|---|')
  for (const p of PLATFORMS) {
    const cells = APPS.map((a) => {
      const filename = p.file(a.prefix, version)
      const url = `${REPO_URL}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(filename)}`
      return `[⬇️ 下载 / Download](${url} "${filename}")`
    }).join(' | ')
    lines.push(`| ${p.label} | ${cells} |`)
  }
  lines.push('')
  if (section) {
    lines.push(section)
    lines.push('')
  } else {
    lines.push(`> ⚠️  CHANGELOG.md 缺少 \`${version}\` 的 section,请补上。`)
    lines.push('')
  }
  lines.push(MACOS_FIRST_LAUNCH_NOTE)
  lines.push('')
  lines.push('---')
  lines.push('')
  lines.push(`完整变更历史 / Full changelog: [CHANGELOG.md](${REPO_URL}/blob/main/CHANGELOG.md)`)
  lines.push('')
  lines.push(`之前版本 / Previous releases: <${REPO_URL}/releases>`)
  return lines.join('\n')
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const tag = process.argv[2]
  if (!tag) { console.error('usage: build-release-notes.mjs <tag>'); process.exit(1) }
  const changelog = readFileSync(resolve(process.cwd(), 'CHANGELOG.md'), 'utf8')
  const body = buildBody(tag, changelog)
  const out = resolve(process.cwd(), 'RELEASE_BODY.md')
  writeFileSync(out, body + '\n')
  console.log(`wrote ${out} (${body.length} bytes)`)
}
