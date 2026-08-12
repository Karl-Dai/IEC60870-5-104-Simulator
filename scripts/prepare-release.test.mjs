import { describe, expect, it } from 'vitest'
import {
  buildAboutNotes,
  buildChangelogSection,
  nextPatchVersion,
  parseCommit,
  prependChangelog,
  updateCargoToml,
  updateJsonVersion,
  updateReleaseNotes,
} from './prepare-release.mjs'

describe('prepare-release', () => {
  it('bumps a stable tag by one patch version', () => {
    expect(nextPatchVersion('v1.15.7')).toBe('1.15.8')
    expect(() => nextPatchVersion('v1.15.7-rc.1')).toThrow(/stable SemVer/)
  })

  it('classifies conventional commits and preserves scopes', () => {
    expect(parseCommit('fix(config): replace workspace on load (#65)')).toEqual({
      category: 'Fixed',
      summary: 'config: replace workspace on load (#65)',
    })
    expect(parseCommit('feat!: change config format')).toEqual({
      category: 'Added',
      summary: 'change config format (breaking)',
    })
    expect(parseCommit('plain subject')).toEqual({ category: 'Changed', summary: 'plain subject' })
  })

  it('generates a categorized changelog and localized About summaries', () => {
    const subjects = [
      'fix(config): replace workspace on load (#65)',
      'feat(log): export filtered rows (#66)',
      'docs: explain releases',
      'fix: 修复中文提交信息',
    ]
    const changelog = buildChangelogSection('1.15.8', '2026-08-12', subjects)
    expect(changelog).toContain('## [1.15.8] - 2026-08-12')
    expect(changelog).toContain('### Added 新增\n\n- log: export filtered rows (#66)')
    expect(changelog).toContain('### Fixed 修复\n\n- config: replace workspace on load (#65)')

    const notes = buildAboutNotes('1.15.8', subjects)
    expect(notes.zh[0]).toContain('新增 / Added')
    expect(notes.en.find((note) => note.startsWith('v1.15.8 Fixed:')))
      .toContain('config: replace workspace on load (#65)')
    expect(notes.en.join(' ')).not.toMatch(/[\u3400-\u9fff]/u)
    expect(notes.en.join(' ')).toContain('See CHANGELOG.md for details.')
  })

  it('updates all supported version file formats', () => {
    expect(updateCargoToml('[package]\nname = "app"\nversion = "1.2.3"\n', '1.2.4'))
      .toContain('version = "1.2.4"')
    expect(updateJsonVersion('{\n  "version": "1.2.3",\n  "x": true\n}\n', '1.2.4'))
      .toContain('"version": "1.2.4"')

  })

  it('inserts the new changelog and release notes without deleting history', () => {
    const oldChangelog = '# Changelog\n\nIntro.\n\n## [1.2.3] - 2026-01-01\n\n- old\n'
    const section = buildChangelogSection('1.2.4', '2026-01-02', ['fix: new fix'])
    const changelog = prependChangelog(oldChangelog, section)
    expect(changelog.indexOf('[1.2.4]')).toBeLessThan(changelog.indexOf('[1.2.3]'))

    const source = `export const RELEASE_NOTES: string[] = [
  'v1.2.3 old note',
]
export const ABOUT_RELEASE_NOTES = {
  'zh-CN': RELEASE_NOTES.slice(0, 1),
  'en-US': [
    'v1.2.3 Old note.',
  ],
} as const
`
    const updated = updateReleaseNotes(
      source,
      '1.2.4',
      { zh: ["v1.2.4 修复 / Fixed: don't regress"], en: ["v1.2.4 Fixed: don't regress"] },
    )
    expect(updated).toContain("'v1.2.4 修复 / Fixed: don\\'t regress'")
    expect(updated).toContain("'zh-CN': RELEASE_NOTES.slice(0, 1)")
    expect(updated).toContain("'v1.2.3 old note'")
    expect(updated).not.toContain('v1.2.3 Old note.')
  })
})
