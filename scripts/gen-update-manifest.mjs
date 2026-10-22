#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const REPO = 'Karl-Dai/IEC60870-5-104-Simulator'

// Tauri 2 with bundle.createUpdaterArtifacts: true publishes:
//   - macOS: <name>_<arch>.app.tar.gz (no version in name)
//   - Linux: <name>_<ver>_amd64.AppImage  (the AppImage itself, NOT a tar.gz)
//   - Windows: <name>_<ver>_x64-setup.exe (the NSIS installer itself, NOT a .nsis.zip)
const PLATFORM_PATTERNS = [
  { key: 'darwin-aarch64', re: /_aarch64\.app\.tar\.gz$/ },
  { key: 'darwin-x86_64',  re: /_x64\.app\.tar\.gz$/ },
  { key: 'windows-x86_64', re: /_x64-setup\.exe$/ },
  { key: 'windows-aarch64', re: /_arm64-setup\.exe$/ },
  { key: 'windows-x86_64-portable', re: /_x64-portable\.exe$/ },
  { key: 'windows-aarch64-portable', re: /_arm64-portable\.exe$/ },
  { key: 'linux-x86_64',   re: /_amd64\.AppImage$/ },
]

// 与 `crates/*/tauri.conf.json` 的 `updater.endpoints` 顺序保持一致(proxy 在前,
// github 兜底)。修改顺序请同步两个 tauri.conf.json。
// cn0 = 自建加速源 gh.carldai.cloud(大陆腾讯云 nginx 前端,链式回源 gh.daichangyu.com
// 新加坡反代 → github),302 改写回本源让安装包下载也走加速;末位 null = GitHub 原始兜底。
export const MANIFEST_VARIANTS = [
  { suffix: '-cn0', prefix: 'https://gh.carldai.cloud/' },
  { suffix: '',     prefix: null },
]

export function buildManifest(manifest, urlPrefix) {
  if (!urlPrefix) return manifest
  const platforms = {}
  for (const [k, v] of Object.entries(manifest.platforms)) {
    platforms[k] = { signature: v.signature, url: `${urlPrefix}${v.url}` }
  }
  return { ...manifest, platforms }
}

export function groupAssetsByRole(assets) {
  const groups = { slave: {}, master: {} }
  const sigByUrl = new Map()
  for (const a of assets) {
    if (a.name.endsWith('.sig')) sigByUrl.set(a.name.slice(0, -4), a.browser_download_url)
  }
  for (const a of assets) {
    if (a.name.endsWith('.sig')) continue
    const role = a.name.startsWith('IEC104Slave_') ? 'slave'
              : a.name.startsWith('IEC104Master_') ? 'master' : null
    if (!role) continue
    const plat = PLATFORM_PATTERNS.find((p) => p.re.test(a.name))
    if (!plat) continue
    groups[role][plat.key] = {
      url: a.browser_download_url,
      sigUrl: sigByUrl.get(a.name),
    }
  }
  return groups
}

export function requirePortableAssets(platforms) {
  for (const arch of ['x86_64', 'aarch64']) {
    if (platforms[`windows-${arch}`] && !platforms[`windows-${arch}-portable`]) {
      throw new Error(`missing signed portable updater for windows-${arch}`)
    }
  }
}

export function extractChangelogSection(md, version) {
  const lines = md.split('\n')
  // Match both `## 1.2.3` and `## [1.2.3]` (Keep a Changelog style).
  const startRe = new RegExp(`^##\\s+\\[?${version.replace(/\./g, '\\.')}\\]?\\b`)
  let inSection = false
  const out = []
  for (const line of lines) {
    if (startRe.test(line)) { inSection = true; continue }
    if (inSection && /^##\s+/.test(line)) break
    if (inSection) out.push(line)
  }
  return out.join('\n').trim()
}

async function fetchSigContent(url) {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`fetch sig failed: ${url} ${res.status}`)
  return (await res.text()).trim()
}

async function fetchReleaseWithRetry(tag, attempts = 6, delayMs = 5000) {
  // tauri-action's per-job upload races with publish-manifest's start: even
  // when every build job has reported success, GitHub's REST API can take
  // a few seconds to surface the release for the freshly-pushed tag. Retry
  // a handful of times before bailing out so a transient 404 doesn't kill
  // the whole release pipeline.
  for (let i = 1; i <= attempts; i++) {
    try {
      return execFileSync('gh', ['api', `repos/${REPO}/releases/tags/${tag}`], { encoding: 'utf8' })
    } catch (e) {
      const stderr = String(e.stderr ?? '')
      const isNotFound = stderr.includes('Not Found') || stderr.includes('HTTP 404')
      if (!isNotFound || i === attempts) throw e
      console.error(`release ${tag} not visible yet (attempt ${i}/${attempts}), retrying in ${delayMs}ms…`)
      await new Promise((r) => setTimeout(r, delayMs))
    }
  }
}

async function main() {
  const tag = process.argv[2]
  if (!tag) { console.error('usage: gen-update-manifest.mjs <tag>'); process.exit(1) }
  const version = tag.replace(/^v/, '')

  const json = await fetchReleaseWithRetry(tag)
  const release = JSON.parse(json)
  const grouped = groupAssetsByRole(release.assets)

  const changelogPath = resolve(process.cwd(), 'CHANGELOG.md')
  const notes = extractChangelogSection(readFileSync(changelogPath, 'utf8'), version)
  const pubDate = release.published_at

  for (const role of ['slave', 'master']) {
    requirePortableAssets(grouped[role])
    const platforms = {}
    for (const [key, val] of Object.entries(grouped[role])) {
      if (!val.sigUrl) {
        throw new Error(
          `missing .sig for ${role}/${key} (asset ${val.url}). ` +
          `Did the TAURI_SIGNING_PRIVATE_KEY secret get configured on the runner?`
        )
      }
      const sig = await fetchSigContent(val.sigUrl)
      platforms[key] = { signature: sig, url: val.url }
    }
    if (Object.keys(platforms).length === 0) {
      throw new Error(`no platforms found for role ${role}`)
    }
    const manifest = { version, notes, pub_date: pubDate, platforms }
    for (const { suffix, prefix } of MANIFEST_VARIANTS) {
      const variant = buildManifest(manifest, prefix)
      const out = resolve(process.cwd(), `latest-${role}${suffix}.json`)
      writeFileSync(out, JSON.stringify(variant, null, 2))
      console.log(`wrote ${out}`)
    }
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((e) => { console.error(e); process.exit(1) })
}
