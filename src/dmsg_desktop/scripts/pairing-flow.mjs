import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/** Real Electron UI/IPC and QR pixels, with an isolated daemon fixture. */
export async function pairingFlow({ app, main, shots, methods, hold, waitFor, calls, created, emit, setSession }) {
  const vector = JSON.parse(readFileSync(new URL('../../../tests/vectors/client/device-invite-v2.json', import.meta.url)))
  let sequence = 0, next = null, failInvite = false, failInspect = false, approveError, scanReady = false
  const preview = token => ({ token, kind: 'device_pair', route: 'approve', device_id: '34'.repeat(32), role: 'Administrator', capabilities: ['ContentSign', 'VaultUnlock', 'RootManage'], expires_at: Date.now() + 240000 })
  const count = method => calls.filter(c => c.method === method).length
  const last = method => calls.filter(c => c.method === method).at(-1)
  methods['device/pair_invite'] = () => {
    if (failInvite) throw { code: 'UNAVAILABLE', message: 'Fixture relay unavailable' }
    return { ...vector, id: `invitation-${++sequence}`, expires_at: Date.now() + 300000, candidate: null }
  }
  methods['device/pair_invite_status'] = p => ({ id: p.id, candidate: next })
  methods['device/pair_invite_cancel'] = () => null
  methods['device/pair_inspect'] = p => {
    if (failInspect) { failInspect = false; throw { code: 'POLICY_STALE', message: 'Fixture account changed during preview' } }
    if (p.token === 'invalid') throw { code: 'INTEGRITY_FAILED', message: 'Fixture bad pairing proof' }
    if (p.token === 'other-device') return { ...preview(p.token), route: 'handoff', can_enroll: true, approvers: ['56'.repeat(32)], qr: vector.qr }
    if (p.token === 'expired-request') return { ...preview(p.token), expires_at: Date.now() - 1 }
    return preview(p.token)
  }
  methods['device/pair_scan'] = p => {
    assert.equal(typeof p.image, 'string')
    assert.equal(p.path, undefined, 'renderer never names a file path')
    assert.ok(Buffer.from(p.image, 'base64').length > 100)
    if (!scanReady) throw { code: 'NOT_FOUND', message: 'No QR in this frame' }
    return preview(vector.plaintext)
  }
  methods['device/key_request'] = () => ({ token: 'signed-key-request', device_id: '78'.repeat(32), key_id: '9a'.repeat(32), qr: vector.qr })
  methods['device/approve'] = p => {
    if (approveError) throw { code: approveError, message: 'Fixture approval failure' }
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
  await main.getByTestId('device-pair-preview').getByText('Content signing', { exact: true }).waitFor()
  await main.getByTestId('device-pair-preview').getByText('Unlock workspace', { exact: true }).waitFor()
  await main.getByTestId('device-pair-preview').getByText('Manage account security', { exact: true }).waitFor()
  approveError = 'UNAVAILABLE'
  await main.getByTestId('device-pair-review').click()
  await main.getByTestId('device-authorization-error').waitFor()
  assert.equal(await main.getByTestId('device-pair-preview').isVisible(), true)
  approveError = undefined
  const finishApproval = hold('device/approve')
  await main.getByTestId('device-pair-review').click()
  await waitFor(() => count('device/approve') === 2)
  assert.equal(await main.getByTestId('device-pair-review').isDisabled(), true)
  assert.equal(await main.getByTestId('device-authorization').getByTestId('dialog-close').isDisabled(), true)
  await finishApproval()
  await main.getByTestId('device-authorization').waitFor({ state: 'detached' })
  assert.deepEqual(last('device/approve').params, { token: 'scanned-request', device: '34'.repeat(32), source: 'gui' })
  assert.equal(count('approval/respond'), 0, 'transport and review never authorize automatically')

  // A concurrent account change can interrupt the preview or invalidate it before review.
  failInspect = true
  next = preview('joining-request')
  await open()
  await main.getByTestId('device-authorization-error').waitFor()
  assert.equal(await main.getByTestId('device-invitation-qr').isVisible(), true)
  await main.getByTestId('device-pair-preview').waitFor()
  next = null
  approveError = 'VERSION_CONFLICT'
  await main.getByTestId('device-pair-review').click()
  await main.getByText('Refresh the request on the joining device and scan it again.', { exact: true }).waitFor()
  assert.equal(await main.getByTestId('device-pair-review').count(), 0)
  const invitations = count('device/pair_invite')
  await main.getByTestId('device-authorization').getByRole('button', { name: 'Retry' }).click()
  await main.getByTestId('device-invitation-qr').waitFor()
  assert.equal(count('device/pair_invite'), invitations + 1)
  await close()
  approveError = undefined

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
    await openScan()
    await main.getByTestId('device-pair-request').fill('other-device')
    await main.getByTestId('device-pair-inspect').click()
    await main.getByTestId('device-pair-preview').waitFor()
    assert.equal(await main.getByTestId('device-pair-review').count(), 0)
    await main.getByTestId('device-pair-preview').getByRole('img').waitFor()
    assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path: join(shots, `pairing-handoff-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    await main.getByTestId('device-pair-enroll').click()
    await main.getByTestId('device-pair-refresh-after-enrollment').waitFor()
    assert.equal(await dialog.getByRole('img').count(), 1)
    assert.equal(await main.getByTestId('device-pair-review').count(), 0)
    assert.equal(count('device/key_register'), 0, 'registration needs an eligible administrator')
    await main.screenshot({ path: join(shots, `pairing-enrollment-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
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

  // A login proof and the later device proof share one invitation and comparison code.
  await main.evaluate(async () => { await window.dmsg.setLocale('en') })
  const loginCandidate = { kind: 'login_binding', token: 'invited-login', principal: 'new-principal', device_id: '34'.repeat(32), code: '1234 ABCD', expires_at: Date.now() + 240000 }
  const originalShow = methods['approval/show']
  const originalBind = methods['account/bind_approve']
  const receiptId = '91'.repeat(32)
  let approvedLogin = false
  methods['account/bind_approve'] = p => {
    assert.equal(p.token, loginCandidate.token); assert.equal(p.principal, loginCandidate.principal)
    return created(receiptId, 'account', { change:'bind_login', principal:p.principal })
  }
  methods['approval/show'] = p => {
    const result = originalShow(p)
    return p.id === receiptId && approvedLogin ? {...result,state:'done'} : result
  }
  next = loginCandidate
  await open()
  await main.getByTestId('device-pair-preview').getByText(loginCandidate.code).waitFor()
  await main.getByTestId('device-pair-review').click()
  await waitFor(() => last('account/bind_approve')?.params.token === loginCandidate.token)
  await main.getByTestId('device-pair-open-login-approval').waitFor()
  const inviteCount = count('device/pair_invite')
  approvedLogin = true
  next = {...preview('device-after-binding'),code:loginCandidate.code}
  await main.getByTestId('device-pair-preview').getByText(loginCandidate.code).waitFor()
  assert.equal(count('device/pair_invite'), inviteCount)
  await main.getByTestId('device-pair-review').click()
  await waitFor(() => last('device/approve')?.params.token === 'device-after-binding')
  next = null
  methods['approval/show'] = originalShow
  methods['account/bind_approve'] = originalBind

  // On the joining desktop, typing or previewing never submits a binding.
  methods['device/moved_list'] = () => []
  methods['device/saved_invitation'] = () => null
  const target = '040g2081040g2081040g'
  const originalStatus = methods['daemon/status']
  let principal = 'first-principal'
  methods['daemon/status'] = () => ({ ...originalStatus(), principal })
  methods['device/join_invitation'] = p => ({
    account: target, principal, code:loginCandidate.code, expires_at:Date.now()+240000,
    stage: p.invitation === 'other-login' && principal === 'first-principal' ? 'other_account' : p.principal ? 'waiting_login' : 'bind_login',
  })
  setSession('no_account'); emit('session/changed')
  const openJoin = main.getByTestId('open-account-invite')
  await openJoin.click()
  const wizard = main.getByTestId('join-account-invitation')
  const input = wizard.getByTestId('join-invitation-token')
  await input.fill('unbound-login')
  await wizard.getByRole('button',{name:'Review',exact:true}).click()
  assert.equal(last('device/join_invitation').params.principal,undefined)
  await wizard.getByText(target).waitFor()
  for (const [locale, appearance, width, height] of [['zh','light',1180,760], ['zh','dark',760,540], ['fr','light',760,540], ['ar','dark',760,540]]) {
    await window.evaluate((window, [w,h]) => window.setContentSize(w,h), [width,height])
    await main.evaluate(async ({locale,appearance}) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, {locale,appearance})
    await main.waitForSelector(`html[lang|="${locale}"]`)
    assert.ok(await wizard.evaluate(el => el.scrollWidth <= el.clientWidth + 1))
    await main.screenshot({ path:join(shots,`account-invitation-${locale}-${appearance}-${width}.png`), animations:'disabled' })
  }
  await main.evaluate(async () => { await window.dmsg.setLocale('en') })

  await wizard.getByText('Use this login for this account and add this device').click()
  await wizard.getByText('Waiting for login approval').waitFor()
  assert.equal(last('device/join_invitation').params.bind_login,true)
  await wizard.getByTestId('dialog-close').click()
  const previousScan = methods['device/pair_scan']
  methods['device/pair_scan'] = params => {
    assert.equal(params.invitation, true)
    assert.equal(params.image, vector.qr.split(',')[1])
    return { token:vector.token, stage:'device', principal:'new-principal', account:target, expires_at:Date.now()+120000 }
  }
  writeFileSync(imagePath, Buffer.from(vector.qr.split(',')[1], 'base64'))
  await openJoin.click()
  await app.evaluate((_electron, path) => globalThis.__open.push(path), imagePath)
  await wizard.getByRole('button',{name:'QR code from an image'}).click()
  await wizard.getByText(target).waitFor()
  await wizard.getByTestId('dialog-close').click()
  methods['device/pair_scan'] = previousScan
  await openJoin.click(); await input.fill('other-login')
  await wizard.getByRole('button',{name:'Review',exact:true}).click()
  await wizard.getByText('This login belongs to another account. Choose a different login.').waitFor()
  await wizard.getByRole('button', { name: 'Log in', exact: true }).click()
  await main.locator('[data-testid="login-steps"][data-phase="browser"]').waitFor()
  const originalLoginResult = methods['session/login_result']
  principal = 'new-principal'
  methods['session/login_result'] = () => ({ done: true, status: methods['daemon/status']() })
  emit('session/changed')
  emit('login/changed')
  await wizard.getByText('new-principal').waitFor()
  assert.equal(last('device/join_invitation').params.invitation, 'other-login')
  assert.equal(last('device/join_invitation').params.principal, undefined)
  methods['session/login_result'] = originalLoginResult
  await wizard.getByTestId('dialog-close').click()

  // Preview and confirmed submission retry the step that failed.
  let failPreview = true, failSubmission = true
  methods['device/join_invitation'] = p => {
    if (p.invitation === 'expired-invite') throw { code: 'EXPIRED', message: 'Expired' }
    if (!p.principal && failPreview) { failPreview = false; throw { code: 'UNAVAILABLE', message: 'Temporary' } }
    if (p.principal && failSubmission) { failSubmission = false; throw { code: 'UNAVAILABLE', message: 'Temporary' } }
    return { account: target, principal, code: loginCandidate.code, expires_at: Date.now() + 240000, stage: p.principal ? 'waiting_login' : 'bind_login' }
  }
  await openJoin.click(); await input.fill('retry-preview')
  await wizard.getByRole('button', { name: 'Review', exact: true }).click()
  await wizard.getByRole('button', { name: 'Retry', exact: true }).click()
  await wizard.getByText(target).waitFor()
  assert.equal(last('device/join_invitation').params.principal, undefined)
  const confirm = wizard.getByRole('button', { name: 'Use this login for this account and add this device', exact: true })
  await confirm.click()
  await wizard.getByRole('button', { name: 'Retry', exact: true }).click()
  await wizard.getByText('Waiting for login approval').waitFor()
  assert.equal(last('device/join_invitation').params.principal, principal)
  assert.equal(last('device/join_invitation').params.bind_login, true)
  await wizard.getByTestId('dialog-close').click()
  await openJoin.click(); await input.fill('expired-invite')
  await wizard.getByRole('button', { name: 'Review', exact: true }).click()
  await input.waitFor()
  await wizard.getByTestId('dialog-close').click()

  // Without a desktop camera, the authorized phone can scan the desktop display.
  methods['account/bind_request'] = () => ({ token:'binding-for-phone', qr:vector.qr })
  await openJoin.click()
  await wizard.getByTestId('join-show-request').click()
  await main.getByTestId('bind-account').fill(target)
  await main.getByTestId('join-account').getByRole('button', {name:'Create a binding request'}).click()
  await main.getByTestId('binding-qr').waitFor()
  await main.getByTestId('join-account').getByRole('button', {name:'Request token', exact:true}).click()
  assert.equal(await main.getByTestId('bind-token').innerText(),'binding-for-phone')
  // The joining desktop can replace a still-valid request and retry after losing its connection.
  let made = 0, checks = 0
  methods['device/pair_request'] = () => ({ ...preview(`joining-request-${++made}`), qr: vector.qr })
  methods['device/moved_list'] = () => []
  methods['recovery/status'] = () => ({ enabled: false })
  methods['account/bind_accept'] = p => {
    assert.equal(p.account, target)
    setSession('pairing_required')
    emit('session/changed')
    return { state: 'pairing_required' }
  }
  methods['session/unlock'] = p => {
    assert.equal(p.pairing_poll, true)
    if (++checks === 1) throw { code: 'NETWORK_UNAVAILABLE', message: 'Fixture connection interrupted' }
    if (checks > 2) {
      setSession(checks === 3 ? 'root_required' : 'unlocked')
      emit('session/changed')
    }
    return null
  }
  await main.evaluate(async () => { await window.dmsg.setLocale('en') })
  await main.getByTestId('join-account').getByRole('button', { name: 'Finish after an administrator approved', exact: true }).click()
  const generate = main.getByTestId('onboarding').getByTestId('pair-request')
  await main.getByTestId('pair-token').filter({ hasText: 'joining-request-1' }).waitFor()
  await generate.focus()
  await main.keyboard.press('Enter')
  await main.getByTestId('pair-token').filter({ hasText: 'joining-request-2' }).waitFor()
  await main.screenshot({ path: join(shots, 'pairing-refresh-joining.png'), animations: 'disabled' })
  await waitFor(() => checks === 2)
  assert.equal(await main.getByTestId('pair-token').isVisible(), true)
  await main.getByText('Approved. Preparing your vault…', { exact: true }).waitFor()
  await waitFor(() => checks === 4)
  await main.getByTestId('welcome-finish').waitFor()
  methods['daemon/status'] = originalStatus
}
