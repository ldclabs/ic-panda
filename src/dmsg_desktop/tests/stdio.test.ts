import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import test from 'node:test'

const moduleUrl = new URL('../src/main/stdio.ts', import.meta.url).href

test('closing a launchers output pipes does not crash the running app', { timeout: 10_000 }, async () => {
  const child = spawn(process.execPath, ['--input-type=module', '-e', `
    import { handleBrokenOutput } from ${JSON.stringify(moduleUrl)}
    const broken = []
    for (const [name, stream] of [['stdout', process.stdout], ['stderr', process.stderr]]) {
      handleBrokenOutput(stream)
      stream.on('error', error => {
        broken.push([name, error.code])
        if (broken.length === 2) process.send(broken, () => process.exit(0))
      })
    }
    process.on('message', () => {
      console.log('output after launcher closed'.repeat(4096))
      console.error('IPC error after launcher closed'.repeat(4096))
    })
    process.send('ready')
  `], { stdio: ['ignore', 'pipe', 'pipe', 'ipc'] })
  const exited = once(child, 'exit')
  try {
    assert.deepEqual(await once(child, 'message'), ['ready', undefined])
    assert.ok(child.stdout && child.stderr)
    child.stdout.destroy()
    child.stderr.destroy()
    const result = once(child, 'message')
    child.send('log')
    const [broken] = await result
    assert.deepEqual(broken.sort(), [['stderr', 'EPIPE'], ['stdout', 'EPIPE']])
    assert.deepEqual(await exited, [0, null])
  } finally {
    if (child.exitCode === null) child.kill()
  }
})

test('unrelated output errors are not suppressed', async () => {
  const child = spawn(process.execPath, ['--input-type=module', '-e', `
    import { handleBrokenOutput } from ${JSON.stringify(moduleUrl)}
    handleBrokenOutput(process.stderr)
    process.stderr.emit('error', Object.assign(new Error('output failed'), { code: 'EIO' }))
  `], { stdio: 'ignore' })
  assert.deepEqual(await once(child, 'exit'), [1, null])
})
