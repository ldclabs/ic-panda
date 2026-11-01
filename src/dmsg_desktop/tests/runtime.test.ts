import assert from 'node:assert/strict'
import { test } from 'node:test'
import { requireRuntimeVersion } from '../src/main/runtime.ts'

test('a desktop beta cannot attach to an older stable daemon or an earlier beta', () => {
  assert.throws(() => requireRuntimeVersion('1.2.4', '1.3.0-beta.1'), /needs daemon/)
  assert.throws(() => requireRuntimeVersion('1.3.0-beta.2', '1.3.0-beta.10'), /needs daemon/)
  assert.throws(() => requireRuntimeVersion(undefined, '1.3.0'), /unknown/)
  requireRuntimeVersion('1.3.0-beta.10', '1.3.0-beta.2')
  requireRuntimeVersion('1.3.0', '1.3.0-beta.10')
  requireRuntimeVersion('1.3.0', '1.3.0')
})
