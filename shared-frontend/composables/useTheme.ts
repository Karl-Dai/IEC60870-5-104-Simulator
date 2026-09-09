import { ref, computed } from 'vue'

export type Theme = 'dark' | 'light'

const STORAGE_KEY = 'iec104-theme'

function systemTheme(): Theme {
  try {
    return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
  } catch {
    return 'dark'
  }
}

function storedTheme(): Theme | null {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved === 'dark' || saved === 'light') return saved
  } catch { /* ignore */ }
  return null
}

function initialTheme(): Theme {
  return storedTheme() ?? systemTheme()
}

function applyTheme(next: Theme) {
  document.documentElement.dataset.theme = next
}

export const theme = ref<Theme>(initialTheme())
applyTheme(theme.value)

// Follow OS theme changes only while the user has not made an explicit choice.
try {
  window.matchMedia?.('(prefers-color-scheme: light)').addEventListener('change', () => {
    if (storedTheme() === null) {
      theme.value = systemTheme()
      applyTheme(theme.value)
    }
  })
} catch { /* ignore */ }

export function setTheme(next: Theme) {
  theme.value = next
  applyTheme(next)
  try { localStorage.setItem(STORAGE_KEY, next) } catch { /* ignore */ }
}

export function toggleTheme() {
  setTheme(theme.value === 'dark' ? 'light' : 'dark')
}

const themeRef = computed(() => theme.value)

export function useTheme() {
  return { theme: themeRef, setTheme, toggleTheme }
}

// Test-only: re-runs initial theme detection. Don't use in production code.
export function __resetThemeForTests() {
  theme.value = initialTheme()
  applyTheme(theme.value)
}
