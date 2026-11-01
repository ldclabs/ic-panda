import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/** Real Electron UI/IPC and QR pixels, with an isolated daemon fixture. */
export async function pairingFlow({ app, main, shots, methods, hold, waitFor, calls, created }) {
  const vector = JSON.parse(readFileSync(new URL('../../../tests/vectors/client/device-invite-v2.json', import.meta.url)))
  let sequence = 0, next = null, failInvite = false, failApprove = false, scanReady = false
  const preview = token => ({ token, device_id: '34'.repeat(32), role: 'Administrator', capabilities: ['ContentSign', 'VaultUnlock', 'RootManage'], expires_at: Date.now() + 240000 })
  const count = method => calls.filter(c => c.method === method).length
  const last = method => calls.filter(c => c.method === method).at(-1)
  methods['device/pair_invite'] = () => {
    if (failInvite) throw { code: 'UNAVAILABLE', message: 'Fixture relay unavailable' }
    return { ...vector, id: `invitation-${++sequence}`, expires_at: Date.now() + 300000, candidate: null }
  }
  methods['device/pair_invite_status'] = p => ({ id: p.id, candidate: next })
  methods['device/pair_invite_cancel'] = () => null
  methods['device/pair_inspect'] = p => {
    if (p.token === 'invalid') throw { code: 'INTEGRITY_FAILED', message: 'Fixture bad pairing proof' }
    return preview(p.token)
  }
  methods['device/pair_scan'] = p => {
    assert.equal(typeof p.image, 'string')
    assert.equal(p.path, undefined, 'renderer never names a file path')
    assert.ok(Buffer.from(p.image, 'base64').length > 100)
    if (!scanReady) throw { code: 'NOT_FOUND', message: 'No QR in this frame' }
    return preview(vector.plaintext)
  }
  methods['device/approve'] = p => {
    if (failApprove) throw { code: 'UNAVAILABLE', message: 'Fixture approval failure' }
    return created('e9'.repeat(32), 'device_pair', preview(p.token))
  }
  const go = async part => { await main.evaluate(part => location.hash = `/settings/${part}`, part) }
  const close = async () => {
    await main.mouse.move(0, 0)
    if (await main.getByRole('tooltip').count()) {
      await main.keyboard.press('Escape')
      await main.getByRole('tooltip').waitFor({ state: 'hidden' })
    }
    await main.keyboard.press('Escape')
    await main.getByTestId('device-authorization').waitFor({ state: 'detached' })
  }
  const open = async () => { await main.getByTestId('device-authorize').click(); await main.getByTestId('device-authorization').waitFor() }
  const openScan = async () => { await main.getByTestId('device-pair-open-scan').click(); await main.getByTestId('device-authorization').waitFor() }

  await go('security')
  await main.getByTestId('security-approve-login').waitFor()
  assert.equal(await main.getByTestId('device-authorize').count(), 0)
  assert.equal(await main.getByTestId('security-approve-others').count(), 0)
  await go('devices')
  await main.getByTestId('device-authorize').focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('device-invitation-qr').waitFor()
  assert.equal(await main.getByTestId('device-invitation-qr').getAttribute('src'), vector.qr)
  assert.equal(await main.getByTestId('device-pair-review').count(), 0)
  await main.evaluate(() => navigator.clipboard.writeText = async text => { globalThis.__copiedInvitation = text })
  await main.getByTestId('device-invitation-copy').click()
  await waitFor(() => main.evaluate(token => globalThis.__copiedInvitation === token, vector.token))
  await close()
  await waitFor(() => count('device/pair_invite_cancel') === 1)

  // Closing while invitation creation waits cancels that exact late invitation.
  const finishInvite = hold('device/pair_invite')
  await open()
  await close()
  await go('security')
  await finishInvite()
  await waitFor(() => count('device/pair_invite_cancel') === 2)
  assert.equal(await main.getByTestId('device-authorization').count(), 0)

  // Receiving a proof only previews it. The explicit review creates the request.
  await go('devices')
  await open()
  next = preview('scanned-request')
  await main.getByTestId('device-pair-preview').waitFor()
  next = null
  assert.equal(count('device/approve'), 0)
  assert.ok((await main.getByTestId('device-pair-preview').innerText()).includes('34'.repeat(32)))
  failApprove = true
  await main.getByTestId('device-pair-review').click()
  await main.getByTestId('device-authorization-error').waitFor()
  assert.equal(await main.getByTestId('device-pair-preview').isVisible(), true)
  failApprove = false
  const finishApproval = hold('device/approve')
  await main.getByTestId('device-pair-review').click()
  await waitFor(() => count('device/approve') === 2)
  assert.equal(await main.getByTestId('device-pair-review').isDisabled(), true)
  assert.equal(await main.getByTestId('device-authorization').getByTestId('dialog-close').isDisabled(), true)
  await finishApproval()
  await main.getByTestId('device-authorization').waitFor({ state: 'detached' })
  assert.deepEqual(last('device/approve').params, { token: 'scanned-request', device: '34'.repeat(32), source: 'gui' })
  assert.equal(count('approval/respond'), 0, 'transport and review never authorize automatically')

  // An unavailable relay still leaves the direct request flow usable. Edits invalidate a pending preview.
  failInvite = true
  await open()
  await main.getByTestId('device-authorization-error').waitFor()
  await main.getByTestId('device-pair-mode').click()
  await main.getByTestId('device-pair-request').fill('invalid')
  await main.getByTestId('device-pair-inspect').click()
  await main.getByTestId('device-authorization-error').waitFor()
  assert.equal(await main.getByTestId('device-pair-request').inputValue(), 'invalid')
  await main.getByTestId('device-pair-request').fill('request-A')
  const finishPreview = hold('device/pair_inspect')
  await main.getByTestId('device-pair-inspect').click()
  await main.getByTestId('device-pair-request').fill('request-B')
  await finishPreview()
  assert.equal(await main.getByTestId('device-pair-preview').count(), 0)
  await main.getByTestId('device-pair-inspect').click()
  await main.getByTestId('device-pair-preview').waitFor()
  assert.equal(last('device/pair_inspect').params.token, 'request-B')
  await main.getByTestId('device-pair-request').fill('request-C')
  assert.equal(await main.getByTestId('device-pair-preview').count(), 0)
  await close()
  failInvite = false

  // Late status replies cannot cross navigation or a switch to manual entry.
  const finishStatus = hold('device/pair_invite_status')
  await open()
  await main.getByTestId('device-invitation-qr').waitFor()
  await main.getByTestId('device-pair-mode').click()
  await finishStatus()
  assert.equal(await main.getByTestId('device-pair-request').isVisible(), true)
  assert.equal(await main.getByTestId('device-invitation-qr').count(), 0)
  await close()

  // The reverse flow scans a new device's request locally, without a relay round trip.
  const imagePath = join(await app.evaluate(({ app }) => app.getPath('userData')), 'device-request.png')
  const image = vector.request_qr.split(',')[1]
  writeFileSync(imagePath, Buffer.from(image, 'base64'))
  scanReady = true
  const approvalsBeforeScan = count('device/approve')
  const invitationsBeforeScan = count('device/pair_invite')
  failInvite = true
  await openScan()
  await main.getByTestId('device-pair-request').fill('draft survives picker cancellation')
  await app.evaluate(() => globalThis.__open.push(undefined))
  await main.getByTestId('device-pair-image').click()
  await waitFor(() => main.getByTestId('device-pair-image').isEnabled())
  assert.equal(await main.getByTestId('device-pair-request').inputValue(), 'draft survives picker cancellation')
  await app.evaluate((_electron, path) => globalThis.__open.push(path), imagePath)
  await main.getByTestId('device-pair-image').click()
  await main.getByTestId('device-pair-preview').waitFor()
  assert.equal(await main.getByTestId('device-pair-request').inputValue(), vector.plaintext)
  await close()
  await app.evaluate(({ clipboard, systemPreferences }, image) => {
    clipboard.read = async () => [{ types: ['image/png'], getType: async () => new Blob([Buffer.from(image, 'base64')], { type: 'image/png' }) }]
    systemPreferences.askForMediaAccess = async () => true
  }, image)
  await openScan()
  await main.getByTestId('device-pair-clipboard').click()
  await main.getByTestId('device-pair-preview').waitFor()
  assert.equal(await main.getByTestId('device-pair-request').inputValue(), vector.plaintext)
  await close()
  assert.equal(count('device/approve'), approvalsBeforeScan)

  // Real video/canvas capture with a synthetic camera: tracks end after success
  // and even when getUserMedia resolves after its dialog has been closed.
  await main.evaluate(async data => {
    const img = new Image()
    img.src = data
    await img.decode()
    const canvas = document.createElement('canvas')
    canvas.width = 640; canvas.height = 640
    const paint = () => canvas.getContext('2d').drawImage(img, 0, 0, 640, 640)
    paint()
    globalThis.__cameraTracks = []
    const make = () => {
      const stream = canvas.captureStream(5)
      globalThis.__cameraTracks.push(...stream.getTracks())
      requestAnimationFrame(paint)
      return stream
    }
    navigator.mediaDevices.getUserMedia = async constraints => {
      if (constraints.audio !== false) throw new Error('microphone requested')
      if (globalThis.__holdCamera) return new Promise(resolve => globalThis.__finishCamera = () => resolve(make()))
      return make()
    }
  }, vector.request_qr)
  scanReady = false
  const scans = count('device/pair_scan')
  await openScan()
  await main.getByTestId('device-pair-camera').click()
  await main.getByTestId('qr-camera').waitFor()
  await waitFor(() => count('device/pair_scan') > scans)
  scanReady = true
  await main.getByTestId('device-pair-preview').waitFor()
  assert.equal(await main.getByTestId('qr-camera').count(), 0)
  await waitFor(() => main.evaluate(() => globalThis.__cameraTracks.every(track => track.readyState === 'ended')))
  assert.equal(count('device/approve'), approvalsBeforeScan)
  await close()
  await main.evaluate(() => globalThis.__holdCamera = true)
  await openScan()
  await main.getByTestId('device-pair-camera').click()
  await waitFor(() => main.evaluate(() => typeof globalThis.__finishCamera === 'function'))
  await close()
  await main.evaluate(() => globalThis.__finishCamera())
  await waitFor(() => main.evaluate(() => globalThis.__cameraTracks.every(track => track.readyState === 'ended')))
  await app.evaluate(({ systemPreferences }) => systemPreferences.askForMediaAccess = async () => false)
  await openScan()
  await main.getByTestId('device-pair-camera').click()
  await main.getByTestId('qr-camera-error').waitFor()
  await close()
  for (const page of app.windows()) {
    if (await page.evaluate(() => window.dmsg.role) === 'approval') {
      await assert.rejects(page.evaluate(() => window.dmsg.cameraAccess()), /FORBIDDEN/)
    }
  }

  assert.equal(count('device/pair_invite'), invitationsBeforeScan, 'direct scanning does not create a relay invitation')
  failInvite = false

  const window = await app.browserWindow(main)
  for (const [locale, appearance, width, height] of [['zh', 'light', 1180, 760], ['zh', 'dark', 760, 540], ['fr', 'light', 760, 540], ['ar', 'dark', 760, 540]]) {
    await window.evaluate((window, [w, h]) => window.setContentSize(w, h), [width, height])
    await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
    await main.waitForSelector(`html[lang|="${locale}"]`)
    await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
    await main.getByTestId('device-authorize').scrollIntoViewIfNeeded()
    await main.screenshot({ path: join(shots, `pairing-devices-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    await open()
    await main.getByTestId('device-invitation-qr').waitFor()
    const dialog = main.getByTestId('device-authorization')
    await main.keyboard.press('Tab')
    assert.ok(await dialog.evaluate(el => el.contains(document.activeElement)))
    assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    assert.ok(await main.getByTestId('device-invitation-qr').evaluate(el => {
      const qr = el.getBoundingClientRect()
      const body = el.closest('[role="dialog"]').querySelector('header').nextElementSibling.getBoundingClientRect()
      return qr.top >= body.top && qr.bottom <= body.bottom
    }), `the complete QR code is visible in ${locale}`)
    await main.screenshot({ path: join(shots, `pairing-qr-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    next = preview('scanned-request')
    await main.getByTestId('device-pair-preview').waitFor()
    next = null
    await main.mouse.move(0, 0)
    await dialog.getByTestId('dialog-close').focus()
    assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path: join(shots, `pairing-preview-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    await close()
    await open()
    await main.getByTestId('device-pair-mode').click()
    await main.getByTestId('device-pair-request').fill('long-pairing-request-'.repeat(120))
    assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path: join(shots, `pairing-paste-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    await close()
  }

  // Expired proofs cannot be submitted from a still-open dialog.
  next = { ...preview('expired-request'), expires_at: Date.now() - 1 }
  await open()
  await main.getByTestId('device-pair-preview').waitFor()
  assert.equal(await main.getByTestId('device-pair-review').isDisabled(), true)
  await close()
  next = null
}
