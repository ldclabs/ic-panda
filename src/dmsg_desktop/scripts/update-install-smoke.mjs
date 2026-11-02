// Exercise the production updater in an isolated Electron process. No release
// is downloaded or installed and no real daemon or account is used.
// DMSG_UPDATE_DIALOG_PREVIEW=1 also opens the native dialogs for visual and
// keyboard inspection; either choice advances without installing anything.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { build } from 'vite'
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const profile = await mkdtemp(join(tmpdir(), 'dmsg-update-install-'))
const output = resolve('test-results/update-install')
await build({
  configFile: false,
  logLevel: 'warn',
  build: {
    ssr: true, outDir: output, emptyOutDir: true, minify: false,
    rollupOptions: {
      input: { updater: resolve('src/main/updater.ts'), session: resolve('src/main/update-session.ts'), text: resolve('src/main/text.ts') },
      external: ['electron', 'electron-updater'],
      output: { format: 'cjs', entryFileNames: '[name].cjs', chunkFileNames: '[name].cjs' }
    }
  }
})
await writeFile(join(output, 'fixture.cjs'), `
const { app, BrowserWindow, dialog, nativeTheme } = require('electron')
const { autoUpdater } = require('electron-updater')
const { Updates } = require('./updater.cjs')
const { UpdateSession } = require('./session.cjs')
const { textFor } = require('./text.cjs')
app.setPath('userData', process.env.DMSG_UPDATE_TEST_PROFILE)
const nativeDialog = dialog.showMessageBox.bind(dialog)
globalThis.scenario = async ({ locale = 'zh', response = 0, mode = 'busy', native = false, theme = 'light' } = {}) => {
  const steps = [], calls = [], prompts = []
  const updates = new Updates(() => {})
  updates.session = new UpdateSession({
    enabled: true, channel: 'stable', changed() {},
    find: async () => ({ version: '1.3.1', channel: 'stable', installer: 'fixture.zip', assets: ['fixture.zip'] }),
    download: async () => {}
  })
  await updates.check()
  nativeTheme.themeSource = theme
  const originalNow = Date.now
  let clock = 0, duplicateBlocked = false, channelBlocked = false
  Date.now = () => clock
  const call = async (method, params) => {
    calls.push([method, params]); clock += 61_000
    if (mode === 'rpc-error' || (mode === 'interrupt-error' && !params.if_idle)) throw new Error('EXECUTION_UNKNOWN')
    if (mode === 'idle' || !params.if_idle) { steps.push('stopped'); return { stopped: true } }
    return { stopped: false, in_flight: { root_rotation: true } }
  }
  const quit = async () => { steps.push('quit'); if (mode === 'quit-error') throw new Error('QUIT_FAILED') }
  const resume = async () => { steps.push('resume') }
  autoUpdater.quitAndInstall = () => { steps.push('install'); if (mode === 'install-error') throw new Error('INSTALL_FAILED') }
  dialog.showMessageBox = async options => {
    prompts.push(options)
    if (options.type === 'error') return { response: 0 }
    const before = calls.length
    await updates.apply(call, quit, resume, textFor(locale))
    duplicateBlocked = before === calls.length
    try { updates.setChannel('beta') } catch { channelBlocked = true }
    return native ? nativeDialog(options) : { response }
  }
  let error
  try { await updates.apply(call, quit, resume, textFor(locale)) }
  catch (failure) { error = failure.message }
  finally { Date.now = originalNow; dialog.showMessageBox = nativeDialog }
  return { steps, calls, prompts, error, duplicateBlocked, channelBlocked, info: updates.info() }
}
app.whenReady().then(async () => {
  const window = new BrowserWindow({ width: 760, height: 540, title: 'dMsg update test — isolated profile' })
  await window.loadURL('about:blank')
})
`)
const app = await electron.launch({ executablePath: electronPath, colorScheme: null,
  args: [join(output, 'fixture.cjs')], env: { ...process.env, DMSG_UPDATE_TEST_PROFILE: profile } })
try {
  await app.firstWindow()
  const run = options => app.evaluate((_electron, options) => globalThis.scenario(options), options)
  const deferred = await run({})
  assert.deepEqual(deferred.steps, [])
  assert.equal(deferred.info.applying, false)
  assert.equal(deferred.info.state.kind, 'ready')
  assert.equal(deferred.duplicateBlocked, true)
  assert.equal(deferred.channelBlocked, true)
  assert.equal(deferred.prompts[0].defaultId, 0)
  assert.equal(deferred.prompts[0].cancelId, 0)
  assert.equal(deferred.prompts[0].buttons[0], '稍后')
  assert.match(deferred.prompts[0].detail, /任务已保存/)
  const interrupted = await run({ response: 1 })
  assert.deepEqual(interrupted.calls, [['daemon/stop', { if_idle: true }], ['daemon/stop', { if_idle: false }]])
  assert.deepEqual(interrupted.steps, ['stopped', 'quit', 'install'])
  assert.equal(interrupted.info.applying, true)
  const idle = await run({ mode: 'idle' })
  assert.deepEqual(idle.steps, ['stopped', 'quit', 'install'])
  assert.equal(idle.prompts.length, 0)
  for (const mode of ['rpc-error', 'interrupt-error', 'quit-error', 'install-error']) {
    const result = await run({ mode, response: 1 })
    assert.equal(result.error, undefined, mode)
    const failure = result.prompts.find(prompt => prompt.type === 'error')
    assert.ok(failure, mode)
    assert.match(failure.detail, /EXECUTION_UNKNOWN|QUIT_FAILED|INSTALL_FAILED/, mode)
    assert.equal(result.info.applying, false, mode)
    assert.equal(result.info.state.kind, 'ready', mode)
    assert.deepEqual(result.steps, mode === 'quit-error' ? ['stopped', 'quit', 'resume']
      : mode === 'install-error' ? ['stopped', 'quit', 'install', 'resume'] : [], mode)
  }
  console.log('Updater Electron checks passed: idle, defer, explicit interrupt, duplicate/channel guard, stop/quit/install failures and retry')
  if (process.env.DMSG_UPDATE_DIALOG_PREVIEW === '1') {
    for (const locale of ['zh', 'fr', 'ar']) for (const theme of ['light', 'dark']) {
      console.log('Native dialog:', locale, theme, '(Later/Escape advances)')
      const result = await run({ locale, theme, native: true })
      assert.deepEqual(result.steps, result.info.applying ? ['stopped', 'quit', 'install'] : [])
      console.log('Native choice:', result.info.applying ? 'restart (simulated)' : 'later')
    }
    console.log('Native dialog matrix completed')
  }
} finally {
  await app.close()
}
