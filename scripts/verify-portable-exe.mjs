import { readFileSync, realpathSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

// tauri-bundler restores the unpatched executable after creating NSIS/MSI
// bundles. A failed bundling operation must never publish a patched installer
// binary as a portable update, because that would select the installer channel.
export function verifyPortableExe(bytes) {
  if (bytes[0] !== 0x4d || bytes[1] !== 0x5a
      || !bytes.includes(Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK'))) {
    throw new Error('Portable EXE must be an unpatched Windows executable')
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  verifyPortableExe(readFileSync(process.argv[2]))
}
