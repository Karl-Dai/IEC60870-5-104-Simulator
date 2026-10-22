import { describe, it, expect } from 'vitest'
import { verifyPortableExe } from './verify-portable-exe.mjs'
import { mkdtempSync, writeFileSync, symlinkSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

describe('portable executable release guard', () => {
  it('accepts the original unpatched Windows executable', () => {
    expect(() => verifyPortableExe(Buffer.from('MZ...__TAURI_BUNDLE_TYPE_VAR_UNK'))).not.toThrow()
  })
  it.each(['NSS', 'MSI'])('rejects a binary patched for %s even when renamed portable.exe', (bundle) => {
    expect(() => verifyPortableExe(Buffer.from(`MZ...__TAURI_BUNDLE_TYPE_VAR_${bundle}`))).toThrow()
  })
  it('rejects non-executable or truncated artifacts', () => {
    expect(() => verifyPortableExe(Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK'))).toThrow()
    expect(() => verifyPortableExe(Buffer.from('MZ'))).toThrow()
  })
  it.skipIf(process.platform === 'win32')('enforces the guard when its CLI is invoked through a symlink', () => {
    const dir = mkdtempSync(join(tmpdir(), 'portable-release-guard-'))
    try {
      const cli = join(dir, 'verify.mjs'), artifact = join(dir, 'portable.exe')
      symlinkSync(fileURLToPath(new URL('./verify-portable-exe.mjs', import.meta.url)), cli)
      writeFileSync(artifact, 'MZ...__TAURI_BUNDLE_TYPE_VAR_UNK')
      expect(spawnSync(process.execPath, [cli, artifact]).status).toBe(0)
      writeFileSync(artifact, 'MZ...__TAURI_BUNDLE_TYPE_VAR_NSS')
      expect(spawnSync(process.execPath, [cli, artifact]).status).not.toBe(0)
    } finally {
      rmSync(dir, { recursive: true, force: true })
    }
  })
})
