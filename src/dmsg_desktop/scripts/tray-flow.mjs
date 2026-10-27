import assert from 'node:assert/strict'
import { join, resolve } from 'node:path'
import { writeFile } from 'node:fs/promises'

/** Observe the actual images sent to Electron's native tray as daemon state changes. */
export async function trayFlow({ app, methods, emit, waitFor, setSession, shots }) {
  await app.evaluate(({ Tray }) => {
    const setImage = Tray.prototype.setImage
    Tray.prototype.setImage = function (image) {
      globalThis.__trayFrame = { png: image.toPNG().toString('base64'), retina: image.toPNG({ scaleFactor: 2 }).toString('base64'), template: image.isTemplateImage(), scales: image.getScaleFactors() }
      return setImage.call(this, image)
    }
  })
  const status = methods['daemon/status']
  let waiting = 0
  methods['daemon/status'] = () => ({ ...status(), approvals_waiting: waiting })
  const template = process.platform === 'darwin' ? 'Template' : ''
  for (const [state, count, badge] of [
    ['unlocked', 0, ''], ['locked', 0, 'Locked'], ['locked', 2, 'LockedWaiting'], ['unlocked', 2, 'Waiting'], ['unlocked', 0, '']
  ]) {
    const expected = await app.evaluate(({ nativeImage }, path) => nativeImage.createFromPath(path).toPNG().toString('base64'), resolve(`resources/tray${badge}${template}.png`))
    waiting = count; setSession(state); emit('session/changed', { state })
    await waitFor(async () => (await app.evaluate(() => globalThis.__trayFrame?.png)) === expected)
    const frame = await app.evaluate(() => globalThis.__trayFrame)
    assert.equal(frame.template, process.platform === 'darwin')
    assert.ok(frame.scales.includes(1) && frame.scales.includes(2))
    await writeFile(join(shots, `tray-${badge || 'idle'}@2x.png`), Buffer.from(frame.retina, 'base64'))
  }
  methods['daemon/status'] = status
  emit('session/changed', { state: 'unlocked' })
}
