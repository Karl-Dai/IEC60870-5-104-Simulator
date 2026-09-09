import { beforeEach, describe, expect, it } from 'vitest'
import { theme, setTheme, toggleTheme, __resetThemeForTests } from '@shared/composables/useTheme'

describe('useTheme', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('defaults to dark when no system preference or stored choice exists', () => {
    __resetThemeForTests()
    expect(theme.value).toBe('dark')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })

  it('toggles between dark and light and persists the choice', () => {
    setTheme('dark')
    toggleTheme()
    expect(theme.value).toBe('light')
    expect(document.documentElement.dataset.theme).toBe('light')
    expect(localStorage.getItem('iec104-theme')).toBe('light')
    toggleTheme()
    expect(theme.value).toBe('dark')
    expect(localStorage.getItem('iec104-theme')).toBe('dark')
  })

  it('restores a stored choice on reset', () => {
    localStorage.setItem('iec104-theme', 'light')
    __resetThemeForTests()
    expect(theme.value).toBe('light')
  })
})
