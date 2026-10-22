import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'

const workflow = readFileSync(new URL('../.github/workflows/release-on-merge.yml', import.meta.url), 'utf8').replaceAll('\r\n', '\n')
const names = ['Identify the merged release PR', 'Validate release metadata', 'Create release tag', 'Start multi-platform release build']
const steps = names.map((name) => {
  const section = workflow.split(`      - name: ${name}\n`)[1]?.split('\n      - name: ')[0]
  assert.ok(section, `Missing workflow step: ${name}`)
  return {
    section,
    script: section.split('        run: |\n')[1].split('\n')
      .filter((line) => line.startsWith('          ')).map((line) => line.slice(10)).join('\n'),
  }
})

// Execute the real workflow shell bodies with read/write commands stubbed.
// No GitHub request, tag push or workflow dispatch leaves this test process.
const mocks = `
gh() {
  echo "gh $*" >> "$CALLS"
  case "$1 $2" in
    'pr list') [ "$HAS_PR" = false ] || echo automation/release-v1.15.21 ;;
    'api --paginate')
      [ "$3" = "repos/test/repo/releases?per_page=100" ] || return 9
      [ "$4" = --jq ] || return 9
      [ "$5" = '.[] | select(.tag_name == "v1.15.21" and .draft == false) | .tag_name' ] || return 9
      [ "$API_ERROR" = false ] || return 1
      # Model the API's published-only filter, including older candidates.
      case "$STATE" in published|older-published) echo v1.15.21 ;; esac ;;
    'release view') echo "$LATEST" ;;
    'run list')
      [ "$RUN_LOOKUP_ERROR" = false ] || return 1
      if [ "$7" = --commit ]; then
        printf '%s' "$PUBLISHED_RUN"
        return 0
      fi
      if [ "\${@: -1}" = '.[] | select(.headSha == "'"$EXISTING_RUN_SHA"'") | [.status, (.conclusion // "")] | @tsv' ]; then
        printf '%s' "$EXISTING_RUN"
      fi ;;
    'run rerun') [ "$RERUN_ERROR" = false ] ;;
    'workflow run') : ;;
    *) return 9 ;;
  esac
}
node() {
  echo "node $*" >> "$CALLS"
  case "$2" in
    current) echo "$CURRENT" ;;
    verify) [ "$METADATA_VALID" = true ] ;;
    next) echo "$EXPECTED" ;;
    *) return 9 ;;
  esac
}
git() {
  echo "git $*" >> "$CALLS"
  case "$1" in
    rev-parse) [ "$TAG_EXISTS" = true ] ;;
    rev-list) [ "$TAG_EXISTS" = true ] && echo tag-sha ;;
    merge-base) [ "$TAG_ANCESTOR" = true ] && { [ -z "$TAG_ANCESTOR_SHA" ] || [ "$4" = "$TAG_ANCESTOR_SHA" ]; } ;;
    fetch|config|tag|push) : ;;
    *) return 9 ;;
  esac
}
`

function run(overrides = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'release-guard-'))
  const calls = join(dir, 'calls').replaceAll('\\', '/')
  const output = join(dir, 'output').replaceAll('\\', '/')
  writeFileSync(calls, '')
  writeFileSync(output, '')
  const env = {
    ...process.env, CALLS: calls, GITHUB_OUTPUT: output,
    GITHUB_REPOSITORY: 'test/repo', GITHUB_SHA: 'tested-sha', TESTED_SHA: 'tested-sha',
    HAS_PR: 'true', STATE: 'absent', API_ERROR: 'false',
    CURRENT: '1.15.21', LATEST: 'v1.15.20', EXPECTED: '1.15.21',
    METADATA_VALID: 'true', TAG_EXISTS: 'false', TAG_ANCESTOR: 'true',
    TAG_ANCESTOR_SHA: '', EXISTING_RUN_SHA: 'tested-sha',
    PUBLISHED_RUN: '123\tcompleted\tsuccess', RUN_LOOKUP_ERROR: 'false', RERUN_ERROR: 'false',
    EXISTING_RUN: '', ...overrides,
  }
  let status = 0
  let logs = ''
  try {
    for (const [index, step] of steps.entries()) {
      if (index > 0) {
        assert.match(step.section, /if: steps\.pull_request\.outputs\.release == 'true'/)
        const outputs = Object.fromEntries(readFileSync(output, 'utf8').trim().split('\n').map((line) => line.split('=')))
        if (outputs.release !== 'true') continue
        env.RELEASE_BRANCH = outputs.branch
        env.TAG = outputs.tag
      }
      const result = spawnSync('bash', ['-c', mocks.replaceAll('\r\n', '\n') + '\n' + step.script], { env, encoding: 'utf8' })
      assert.ifError(result.error)
      status = result.status
      logs += result.stdout + result.stderr
      if (status !== 0) break
    }
    return { status, logs, calls: readFileSync(calls, 'utf8'), output: readFileSync(output, 'utf8') }
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

for (const state of ['published', 'older-published']) {
  test(`${state} release skips validation, tags and dispatch`, () => {
    const result = run({ STATE: state, TAG_EXISTS: 'true', LATEST: state === 'older-published' ? 'v1.15.22' : 'v1.15.21', EXPECTED: '1.15.22' })
    assert.equal(result.status, 0, result.logs)
    assert.match(result.output, /release=false/)
    assert.doesNotMatch(result.calls, /node .* (verify|next)|gh release view|git (tag|push)|gh workflow run|gh run rerun/)
  })
}

for (const status of ['queued', 'in_progress', 'waiting', 'pending', 'requested']) {
  test(`published release with ${status} run is not retried`, () => {
    const result = run({ STATE: 'published', TAG_EXISTS: 'true', PUBLISHED_RUN: `123\t${status}\t` })
    assert.equal(result.status, 0, result.logs)
    assert.match(result.output, /release=false/)
    assert.doesNotMatch(result.calls, /gh run rerun|gh workflow run|git (tag|push)/)
  })
}

for (const conclusion of ['cancelled', 'timed_out']) {
  test(`published release with ${conclusion} run retries unsuccessful jobs`, () => {
    const result = run({ STATE: 'published', TAG_EXISTS: 'true', PUBLISHED_RUN: `123\tcompleted\t${conclusion}` })
    assert.equal(result.status, 0, result.logs)
    assert.match(result.calls, /gh run rerun 123 --failed/)
    assert.doesNotMatch(result.calls, /gh workflow run|git (tag|push)/)
  })
}

for (const [label, overrides] of [
  ['missing run', { PUBLISHED_RUN: '' }],
  ['run lookup failure', { RUN_LOOKUP_ERROR: 'true' }],
  ['rerun failure', { PUBLISHED_RUN: '123\tcompleted\tfailure', RERUN_ERROR: 'true' }],
  ['unrecognized conclusion', { PUBLISHED_RUN: '123\tcompleted\tskipped' }],
  ['missing tag', { TAG_EXISTS: 'false' }],
  ['tag outside tested history', { TAG_ANCESTOR: 'false' }],
]) {
  test(`published release ${label} fails visibly`, () => {
    const result = run({ STATE: 'published', TAG_EXISTS: 'true', ...overrides })
    assert.notEqual(result.status, 0, result.logs)
    assert.doesNotMatch(result.output, /release=false/)
    assert.doesNotMatch(result.calls, /gh workflow run|git (tag|push)/)
    if (label !== 'rerun failure') assert.doesNotMatch(result.calls, /gh run rerun/)
  })
}

test('no matching merged release PR is a no-op', () => {
  const result = run({ HAS_PR: 'false' })
  assert.equal(result.status, 0, result.logs)
  assert.doesNotMatch(result.calls, /gh api|git |gh workflow run/)
})

for (const state of ['absent', 'draft']) {
  test(`${state} release validates, tags and dispatches`, () => {
    const result = run({ STATE: state })
    assert.equal(result.status, 0, result.logs)
    assert.match(result.calls, /node .* verify v1.15.21/)
    assert.match(result.calls, /git push origin v1.15.21/)
    assert.match(result.calls, /gh workflow run release.yml --ref v1.15.21 -f tag=v1.15.21/)
  })
}

test('existing ancestor tag still permits recovery of a failed draft build', () => {
  const result = run({ STATE: 'draft', TAG_EXISTS: 'true', EXISTING_RUN: 'completed\tfailure' })
  assert.equal(result.status, 0, result.logs)
  assert.match(result.calls, /git merge-base --is-ancestor v1.15.21 tested-sha/)
  assert.doesNotMatch(result.calls, /git tag |git push/)
  assert.match(result.calls, /gh workflow run/)
})

for (const [label, overrides] of [
  ['API failure', { API_ERROR: 'true' }],
  ['branch metadata mismatch', { CURRENT: '1.15.20' }],
  ['invalid release metadata', { METADATA_VALID: 'false' }],
  ['wrong patch version', { EXPECTED: '1.15.22' }],
  ['tag outside tested history', { TAG_EXISTS: 'true', TAG_ANCESTOR: 'false' }],
]) {
  test(`${label} fails closed`, () => {
    const result = run(overrides)
    assert.notEqual(result.status, 0, result.logs)
    assert.doesNotMatch(result.calls, /git push|gh workflow run/)
  })
}

for (const existing of ['queued\t', 'in_progress\t', 'completed\tsuccess']) {
  test(`${existing.trim()} build is not dispatched twice`, () => {
    const result = run({ TAG_EXISTS: 'true', EXISTING_RUN: existing })
    assert.equal(result.status, 0, result.logs)
    assert.doesNotMatch(result.calls, /gh workflow run/)
  })
}

test('release job binds TESTED_SHA to the triggering Test commit', () => {
  assert.match(workflow, /    env:\n      TESTED_SHA: \$\{\{ github\.event\.workflow_run\.head_sha \}\}/)
})

test('tag ancestry uses the triggering Test commit after main advances', () => {
  const result = run({ GITHUB_SHA: 'newer-main-sha', TAG_EXISTS: 'true' })
  assert.equal(result.status, 0, result.logs)
  assert.match(result.calls, /git merge-base --is-ancestor v1.15.21 tested-sha/)
})

test('tag only in newer untested main history is rejected', () => {
  const result = run({
    GITHUB_SHA: 'newer-main-sha', TAG_EXISTS: 'true', TAG_ANCESTOR_SHA: 'newer-main-sha',
  })
  assert.notEqual(result.status, 0, result.logs)
  assert.doesNotMatch(result.calls, /git push|gh workflow run/)
})

for (const existing of ['queued\t', 'in_progress\t', 'completed\tsuccess']) {
  test(`advancing main does not redispatch ${existing.trim()} tested build`, () => {
    const result = run({ GITHUB_SHA: 'newer-main-sha', TAG_EXISTS: 'false', EXISTING_RUN: existing })
    assert.equal(result.status, 0, result.logs)
    assert.match(result.calls, /select\(\.headSha == "tested-sha"\)/)
    assert.doesNotMatch(result.calls, /gh workflow run/)
  })
}

for (const state of ['published', 'older-published']) {
  test(`${state} release retries failed jobs on its original tag commit`, () => {
    const result = run({
      STATE: state, TAG_EXISTS: 'true', PUBLISHED_RUN: '123\tcompleted\tfailure',
      GITHUB_SHA: 'newer-main-sha', TESTED_SHA: 'newer-tested-sha',
      LATEST: state === 'older-published' ? 'v1.15.22' : 'v1.15.21', EXPECTED: '1.15.22',
    })
    assert.equal(result.status, 0, result.logs)
    assert.match(result.calls, /gh run rerun 123 --failed/)
    assert.match(result.calls, /gh run list --workflow release.yml --branch v1.15.21 --commit tag-sha --limit 1/)
    assert.match(result.calls, /git merge-base --is-ancestor v1.15.21 newer-tested-sha/)
    assert.doesNotMatch(result.calls, /node .* (verify|next)|gh release view|git (tag|push)|gh workflow run/)
    assert.match(result.output, /release=false/)
  })
}
