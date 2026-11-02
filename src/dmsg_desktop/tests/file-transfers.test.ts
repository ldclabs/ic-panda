import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync } from 'node:fs'
import { createContext, runInContext } from 'node:vm'
import ts from 'typescript'

function cards() {
  const source = readFileSync(new URL('../src/renderer/views/files/adding.svelte.ts', import.meta.url), 'utf8')
  const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
  const context = createContext({ exports: {}, $state: (value: unknown) => value, setTimeout: () => 0 })
  runInContext(code, context)
  return context.exports as any
}

test('last bytes wait for cloud commit or integrity verification', () => {
  const { jobs, progress, backups, downloads } = cards()
  progress({ job: 'backup', path: 'file', upload_id: 'upload', done: 100, total: 100, stage: 'committing' })
  backups([{ file: 'file', version: 'version', upload_id: 'upload', sent: 100, chunks: 100, stage: 'committing' }])
  assert.equal(jobs.length, 1)
  assert.equal(jobs[0].stage, 'committing')
  backups([])
  assert.equal(jobs[0].stage, 'done')
  progress({ job: 'fetch', path: 'download', done: 100, total: 100, stage: 'verifying' })
  assert.equal(jobs[1].stage, 'verifying')
  progress({ job: 'fetch', path: 'download', done: 100, total: 100, stage: 'failed', error: 'INTEGRITY_FAILED' })
  assert.equal(jobs[1].error, 'INTEGRITY_FAILED')
  downloads([{ upload_id: 'download', verify: false, done: 100, total: 100, bytes: 100, total_bytes: 100, stage: 'failed', error: 'INTEGRITY_FAILED' }])
  assert.equal(jobs[1].stage, 'error')
})

test('retry updates the same task and different versions remain separate', () => {
  const { jobs, progress, backups } = cards()
  backups([{ file: 'file', version: 'v1', upload_id: 'first', sent: 4, chunks: 100, stage: 'retrying', error: 'UNAVAILABLE' }])
  progress({ job: 'backup', path: 'file', upload_id: 'first', done: 8, total: 100, stage: 'uploading' })
  assert.equal(jobs.length, 1)
  assert.equal(jobs[0].stage, 'running')
  assert.equal(jobs[0].error, undefined)
  backups([{ file: 'file', version: 'v1', upload_id: 'first', sent: 8, chunks: 100 }, { file: 'file', version: 'v2', upload_id: 'second', sent: 0, chunks: 50 }])
  assert.equal(jobs.length, 2)
})


test('navigation retains cards while account changes forget them', () => {
  const { jobs, progress, scopeJobs } = cards()
  scopeJobs('first-account')
  progress({ job: 'fetch', path: 'download', done: 1, total: 100 })
  scopeJobs('first-account')
  assert.equal(jobs.length, 1)
  scopeJobs('second-account')
  assert.equal(jobs.length, 0)
})
