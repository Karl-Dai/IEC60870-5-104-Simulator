import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'
import { buildSharedAliases } from '../shared-frontend/vite/aliases'

export default defineConfig(({ mode }) => {
  const alias = buildSharedAliases(import.meta.url)
  if (mode === 'mock') {
    alias['@tauri-apps/api/core'] = fileURLToPath(
      new URL('./src/mocks/tauriCore.ts', import.meta.url),
    )
  }

  return {
    plugins: [vue()],
    resolve: { alias },
    server: mode === 'mock'
      ? { allowedHosts: ['.trycloudflare.com'] }
      : undefined,
  }
})
