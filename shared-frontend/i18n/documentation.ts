import type { Locale } from './types'

export function documentationUrlForLocale(repoUrl: string, locale: Locale): string {
  const readme = locale === 'zh-CN' ? 'README_CN.md' : 'README.md'
  return `${repoUrl}/blob/main/${readme}`
}
