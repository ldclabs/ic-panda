import assert from 'node:assert/strict'
import { join } from 'node:path'

/**
 * Replacing a device, with synthetic data in the smoke runner: the request
 * a new device makes, the review and handover steps on both sides,
 * transfers between devices, what only this device holds, moving native
 * addresses and this device's native key root.
 */
export async function successionFlow({ app, main, shots, methods, wallets, account, created, emit, waitFor, calls, setSession }) {
  const go = route => main.evaluate(route => location.hash = route, route)
  const last = method => calls.filter(c => c.method === method).at(-1)
  const count = method => calls.filter(c => c.method === method).length
  const browser = await app.browserWindow(main)
  const me = '11'.repeat(32), fresh = '34'.repeat(32), old = '56'.repeat(32)
  let sequence = 0xc0
  const request = (type, detail) => created((++sequence).toString(16).padStart(2, '0').repeat(32), type, detail)

  /** Each shown case at the default and the narrowest window, light and dark, in Chinese, French and Arabic. */
  async function matrix(name, selector) {
    for (const [locale, appearance, width, height] of [['zh', 'light', 1180, 760], ['zh', 'dark', 760, 540], ['fr', 'light', 760, 540], ['ar', 'dark', 760, 540]]) {
      await browser.evaluate((w, [width, height]) => w.setContentSize(width, height), [width, height])
      await main.evaluate(async ({ locale, appearance }) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(appearance) }, { locale, appearance })
      await main.waitForSelector(`html[lang|="${locale}"]`)
      await main.waitForFunction(dark => matchMedia('(prefers-color-scheme: dark)').matches === dark, appearance === 'dark')
      const target = main.locator(selector).first()
      await target.scrollIntoViewIfNeeded()
      assert.ok(await target.evaluate(el => el.scrollWidth <= el.clientWidth + 1), `${name} fits ${locale} ${width}`)
      assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${name} has no window overflow in ${locale}`)
      await main.screenshot({ path: join(shots, `succession-${name}-${locale}-${appearance}-${width}.png`), animations: 'disabled' })
    }
    await browser.evaluate(w => w.setContentSize(1180, 760))
    await main.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
    await main.waitForSelector('html[lang="en"]')
    await main.waitForFunction(() => !matchMedia('(prefers-color-scheme: dark)').matches)
  }

  // What only this device holds: unbacked versions, unsynced changes and a sync older than a week.
  const local = { versions: 12, bytes: 5_330_000, unbacked_versions: 3, unbacked_bytes: 1_250_000, pending_sync: 2, last_sync_ok_at: Date.now() - 9 * 86_400_000 }
  methods['files/local_only'] = () => structuredClone(local)

  // Tasks as the daemon shows them: every step, with what is available on this side.
  const ORDER = ['authorize', 'approval_key', 'cosigners', 'content', 'wallets', 'native_addresses', 'revoke']
  const AFTER = { authorize: [], approval_key: ['authorize'], cosigners: ['authorize'], content: ['authorize'], wallets: ['cosigners'], native_addresses: ['wallets'], revoke: ['approval_key', 'content', 'wallets', 'native_addresses'] }
  const RUNS = { predecessor: ['authorize', 'approval_key', 'cosigners', 'content'], successor: ['approval_key', 'cosigners', 'content', 'wallets', 'native_addresses', 'revoke'] }
  const done = evidence => ({ at: Date.now(), evidence, skipped: [] })
  const view = task => ({
    id: task.id,
    side: task.side,
    account,
    predecessor: task.predecessor,
    successor: task.successor,
    steps: ORDER.map(step => ({ step, settled: !!task.steps[step], at: task.steps[step]?.at ?? null, evidence: task.steps[step]?.evidence ?? null, skipped: task.steps[step]?.skipped ?? [] })),
    available: task.cancelled_at ? [] : ORDER.filter(s => !task.steps[s] && RUNS[task.side].includes(s) && AFTER[s].every(p => task.steps[p])),
    cancelled_at: task.cancelled_at ?? null,
    created_at: task.created_at
  })
  let tasks = []
  methods['device/successions'] = () => tasks.map(view)
  const task = id => tasks.find(t => t.id === id)
  const changed = async (type = 'approval/changed') => {
    const before = count('device/successions')
    emit(type)
    await waitFor(() => count('device/successions') > before)
  }

  const token = `ZG1zZy1zdWNjZXNzaW9uLzE${'x'.repeat(900)}`
  const capabilities = ['ContentSign', 'VaultUnlock', 'RootManage', 'FormalApprove', 'WalletApprove']
  methods['device/succession_review'] = p => {
    if (p.token !== token) throw { code: 'INVALID_INPUT', message: 'Fixture token is not a succession request' }
    return { id: 'b1'.repeat(32), device_id: fresh, role: 'Administrator', capabilities, approval_key: true, cosigner: true, expires_at: Date.now() + 600_000, wallets: [{ wallet: wallets.a, name: 'Engineering' }, { wallet: wallets.b, name: 'Operations' }] }
  }
  methods['device/succession_approve'] = p => {
    assert.deepEqual([p.token, p.device], [token, fresh])
    if (!task('b1'.repeat(32))) tasks.push({ id: 'b1'.repeat(32), side: 'predecessor', predecessor: me, successor: fresh, steps: {}, created_at: Date.now() })
    return { ...request('device_pair', { device_id: fresh, role: 'Administrator', capabilities }), succession: 'b1'.repeat(32) }
  }
  let rotated = false
  methods['device/succession_run'] = p => {
    const t = task(p.id)
    assert.ok(view(t).available.includes(p.step), `${p.step} is available on the ${t.side}`)
    switch (`${t.side}:${p.step}`) {
      case 'predecessor:authorize':
        return methods['device/succession_approve']({ token, device: fresh })
      case 'predecessor:approval_key':
        return request('approval_key', { device_id: t.successor, key_id: '5b'.repeat(32) })
      case 'predecessor:cosigners':
        // A wallet that has not rotated yet leaves the step open; the retry finishes it.
        if (!rotated) {
          rotated = true
          return { rotations: [{ wallet: wallets.a, rotated: true }, { wallet: wallets.b, rotated: false }] }
        }
        t.steps.cosigners = done({ rotations: [{ wallet: wallets.a, rotated: true }, { wallet: wallets.b, rotated: true }] })
        return view(t)
      case 'successor:wallets':
        return { requests: [wallets.a, wallets.b].map(wallet => request('wallet', { action: 'join', wallet, name: wallet === wallets.a ? 'Engineering' : 'Operations', replaces: [t.predecessor], exclusive: false })) }
      case 'successor:native_addresses':
        return { addresses: [{ wallet: wallets.a, from: 5 }] }
      case 'successor:revoke':
        return request('device_revoke', { device_id: t.predecessor })
    }
    throw { code: 'FORBIDDEN', message: 'Fixture step does not run on this side' }
  }
  methods['device/succession_skip'] = p => {
    const t = task(p.id)
    t.steps[p.step] = { at: Date.now(), evidence: null, skipped: p.skipped }
    return view(t)
  }
  methods['device/succession_cancel'] = p => {
    task(p.id).cancelled_at = Date.now()
    return view(task(p.id))
  }

  // One transfer at a time, as the daemon keeps it.
  let transfer = null
  methods['transfer/status'] = () => structuredClone(transfer)
  const report = (bytes, peer) => ({ peer, entries: 40, records: 0, kept: 0, skipped: 0, blobs: 0, bytes, total_bytes: 5_330_000, last_error: null })
  methods['transfer/receive_start'] = p => {
    transfer = {
      role: 'receive', session: p.session ?? 'e3'.repeat(16), advert: 'dmsg-e3e3', to: me, succession: p.succession ?? null, port: 52000,
      addresses: ['192.168.1.20:52000', '[fe80::1]:52000'], bonjour: 'Local network access was declined', state: 'waiting',
      error: null, report: report(0, null), started_at: Date.now(), ended_at: null
    }
    return structuredClone(transfer)
  }
  methods['transfer/send_start'] = p => {
    transfer = {
      role: 'send', session: p.session, advert: 'dmsg-e3e3', to: p.to ?? fresh, succession: p.succession ?? null, port: null,
      addresses: p.address ? [p.address] : [], bonjour: null, state: 'running', error: null, report: report(2_000_000, fresh), started_at: Date.now(), ended_at: null
    }
    return structuredClone(transfer)
  }
  methods['transfer/cancel'] = () => {
    transfer = { ...transfer, state: 'cancelled', ended_at: Date.now() }
    return structuredClone(transfer)
  }
  methods['device/moved_import'] = p => {
    assert.equal(p.id, `${account}-56565656`)
    return { records: 12, bytes: 1_250_000 }
  }

  await go('/settings/devices')
  await main.getByTestId('devices').waitFor()
  const risk = main.getByTestId('devices-local-only')
  await risk.waitFor()
  const riskText = await risk.innerText()
  for (const part of ['3 file versions', '2 changes', 'more than 7 days']) assert.ok(riskText.includes(part), `${part} in ${riskText}`)
  await main.getByTestId('succession').getByText('No handovers on this device.').waitFor()
  await main.getByTestId('device-transfer').getByText('No transfer yet.').waitFor()
  assert.equal(await main.locator('input:visible, textarea:visible').count(), 0, 'the page has no open forms')

  // The old device reviews the new device's request with the keyboard; editing it drops the shown review.
  await main.getByTestId('succession-review').focus()
  await main.keyboard.press('Enter')
  const review = main.getByTestId('succession-review-dialog')
  await review.waitFor()
  assert.equal(await main.getByTestId('succession-check').isDisabled(), true)
  await main.getByTestId('succession-token').fill('not a request')
  await main.getByTestId('succession-check').click()
  // The interface names the failure, never the daemon's raw message.
  await review.getByText('INVALID_INPUT', { exact: false }).waitFor()
  await main.getByTestId('succession-token').fill(token)
  await main.getByTestId('succession-check').click()
  await main.getByTestId('succession-reviewed').waitFor()
  assert.ok((await main.getByTestId('succession-reviewed').innerText()).includes('Engineering · Operations'))
  await main.getByTestId('succession-token').fill(`${token} `)
  await main.getByTestId('succession-reviewed').waitFor({ state: 'detached' })
  await main.getByTestId('succession-check').click()
  await main.getByTestId('succession-reviewed').waitFor()
  assert.equal(await main.getByTestId('succession-approve').isDisabled(), true)
  await main.getByTestId('succession-device').fill('ab'.repeat(32))
  await review.getByText('This is not the device in the request.').waitFor()
  assert.equal(await main.getByTestId('succession-approve').isDisabled(), true)
  await main.keyboard.press('Tab')
  assert.ok(await review.evaluate(el => el.contains(document.activeElement)), 'focus stays within the review')
  await matrix('review', '[data-testid="succession-review-dialog"]')
  await main.getByTestId('succession-device').fill(` ${fresh.toUpperCase()} `)
  await waitFor(() => main.getByTestId('succession-approve').isEnabled())
  await main.getByTestId('succession-approve').click()
  await review.waitFor({ state: 'detached' })
  // The main process marks requests the person starts here as the window's.
  assert.deepEqual(last('device/succession_approve').params, { token, device: fresh, source: 'gui' })
  await main.getByTestId('succession-task').waitFor()
  assert.equal(await main.getByTestId('succession-run-authorize').count(), 1, 'the authorization waits until the approval is done')

  // Once the new device is enrolled, the old device runs its own steps.
  task('b1'.repeat(32)).steps.authorize = done({ account_version: 8, added_at: Date.now() })
  await changed()
  await main.getByTestId('succession-run-approval_key').waitFor()
  await main.getByTestId('succession-run-approval_key').click()
  await waitFor(() => last('device/succession_run')?.params.step === 'approval_key')
  task('b1'.repeat(32)).steps.approval_key = done({ key_id: '5b'.repeat(32) })
  await changed()
  await main.getByTestId('succession-run-cosigners').click()
  await main.getByTestId('succession-notice').getByText('Some wallets are not done yet.', { exact: false }).waitFor()
  await main.getByTestId('succession-run-cosigners').click()
  await main.getByTestId('succession-step-cosigners').getByText('Done', { exact: true }).waitFor()

  // The old device sends its file library to the new one's session; a handover needs no device ID.
  await main.getByTestId('succession-send').click()
  const send = main.getByTestId('transfer-send-dialog')
  await send.waitFor()
  assert.equal(await main.getByTestId('transfer-to').count(), 0)
  await main.getByTestId('transfer-session').fill('e3e3')
  assert.equal(await main.getByTestId('transfer-start').isDisabled(), true)
  assert.equal(await main.getByTestId('transfer-session').getAttribute('aria-invalid'), 'true')
  await main.getByTestId('transfer-session').fill('E3'.repeat(16))
  await main.getByTestId('transfer-start').click()
  await send.waitFor({ state: 'detached' })
  assert.deepEqual(last('transfer/send_start').params, { session: 'e3'.repeat(16), succession: 'b1'.repeat(32) })
  await main.getByTestId('transfer-progress').waitFor()
  transfer = { ...transfer, state: 'done', ended_at: Date.now(), report: { ...transfer.report, records: 40, kept: 2, skipped: 1, bytes: 5_330_000 } }
  task('b1'.repeat(32)).steps.content = done(transfer.report)
  emit('transfer/changed')
  await main.getByTestId('transfer-counts').waitFor()
  assert.ok((await main.getByTestId('transfer-counts').innerText()).includes('40 records added, 2 already here, 1 left out'))
  await main.getByTestId('succession-step-content').getByText('Done', { exact: true }).waitFor()

  // Stopping asks first; Escape keeps the handover.
  await main.getByTestId('succession-task').getByRole('button', { name: 'Stop handover', exact: true }).click()
  await main.getByTestId('succession-stop').waitFor()
  await main.keyboard.press('Escape')
  await main.getByTestId('succession-stop').waitFor({ state: 'detached' })
  assert.equal(count('device/succession_cancel'), 0)

  // On the new device: wallets, native addresses, content and the old device's revocation.
  const successor = { id: 'b2'.repeat(32), side: 'successor', predecessor: old, successor: me, created_at: Date.now(), steps: {
    authorize: done({ account_version: 8, added_at: Date.now() }), approval_key: done({ key_id: '5c'.repeat(32) }), cosigners: done({ confirmed: true })
  } }
  tasks = [successor]
  transfer = null
  await changed('session/changed')
  await main.getByTestId('succession-run-wallets').waitFor()
  await main.getByTestId('succession-run-wallets').click()
  await main.getByTestId('succession-notice').getByText('2 wallet approvals are waiting', { exact: false }).waitFor()
  successor.steps.wallets = done({ wallets: [wallets.a, wallets.b] })
  await changed('wallet/changed')
  await main.getByTestId('succession-native').click()
  await main.getByTestId('succession-addresses').waitFor()
  await main.getByTestId('succession-receive').click()
  await main.getByTestId('transfer-session-code').waitFor()
  assert.deepEqual(last('transfer/receive_start').params, { succession: successor.id })
  assert.equal(await main.getByTestId('transfer-session-code').innerText(), 'e3'.repeat(16))
  await main.getByTestId('transfer-addresses').getByText('192.168.1.20:52000', { exact: false }).waitFor()
  await matrix('devices', '[data-testid="succession"]')
  // A stopped transfer resumes in the same session.
  await main.getByTestId('transfer-cancel').click()
  await main.getByTestId('transfer-resume').waitFor()
  await main.getByTestId('transfer-resume').click()
  await waitFor(() => last('transfer/receive_start').params.session === 'e3'.repeat(16))
  assert.deepEqual(last('transfer/receive_start').params, { session: 'e3'.repeat(16), succession: successor.id })
  await main.getByTestId('transfer-cancel').click()
  await main.getByTestId('transfer-resume').waitFor()

  // The handover's address opens the wallet with the move to this device.
  wallets.state.enabled = true
  wallets.state.localKeys = [{ label: 'This Mac', public_key: `02${'21'.repeat(32)}`, hardware: true, backend: 'secure-enclave' }]
  const show = methods['wallet/show']
  methods['wallet/show'] = p => {
    const shown = show(p)
    if (p.wallet !== wallets.a) return shown
    return {
      ...shown,
      addresses: [...shown.addresses, { id: 5, chain: 'Bitcoin', control: 'taproot_2of2', text: 'bc1pnativeoldmacfixture', label: 'Old Mac vault', verified: true, cosign: null, archived: false, control_details: { cosigner: 7, chain_key_after: 144, recovery: null } }],
      cosigners: [
        ...shown.cosigners,
        { id: 7, label: 'Old Mac native', member: account, public_key: '7a'.repeat(32), curve: 'Bip340', envelope: 'Raw', local: false },
        { id: 8, label: 'Lost Mac', member: account, public_key: `03${'8b'.repeat(32)}`, curve: 'P256', envelope: 'Raw', local: false }
      ]
    }
  }
  const utxos = [
    { txid: 'c4'.repeat(32), vout: 0, value: '150000', height: 869_700, spendable_at: 869_844 },
    { txid: 'c5'.repeat(32), vout: 1, value: '72000', height: 869_990, spendable_at: 870_134 },
    { txid: 'c6'.repeat(32), vout: 0, value: '9000', height: null, spendable_at: null }
  ]
  let proposed = null
  const plan = path => ({
    wallet: wallets.a,
    from: { id: 5, text: 'bc1pnativeoldmacfixture', label: 'Old Mac vault', cosigner: 7, mine: false, chain_key_after: 144, recovery_after: 52_560 },
    key: '9f'.repeat(32),
    cosigner: 9,
    to: { id: 6, text: 'bc1pnativethismacfixture', label: 'Old Mac vault (this Mac)' },
    next: proposed ? 'wait' : 'sweep',
    pending: proposed ? { proposal: proposed, status: 'Open' } : null,
    cooldown_until: null,
    archived: false,
    sweep: path === 'l1'
      ? { path, tip: 870_000, fee_rate: 4, utxos, inputs: 3, total: '231000', fee: '1240', amount: '229760', dust: false, empty: false }
      : { path, tip: 870_000, fee_rate: 4, utxos, inputs: 1, total: '150000', fee: '620', amount: '149380', dust: false, empty: false }
  })
  methods['wallet/nc_migrate_plan'] = p => {
    assert.deepEqual([p.wallet, p.from], [wallets.a, 5])
    return plan(p.path)
  }
  methods['wallet/nc_migrate_start'] = p => {
    proposed = 61
    return plan(p.path)
  }
  await main.getByTestId('succession-addresses').getByRole('button', { name: 'Move to this device', exact: true }).click()
  const move = main.getByTestId('wallet-nc-migrate')
  await move.waitFor()
  await main.getByTestId('wallet-nc-sweep').waitFor()
  assert.equal(await move.getByText('Spendable now', { exact: true }).count(), 3, 'the old device cosigns every output')
  await main.click('[data-testid="wallet-nc-path"] [data-value="l2"]')
  await move.getByText('From block 870134', { exact: true }).waitFor()
  assert.deepEqual(last('wallet/nc_migrate_plan').params, { wallet: wallets.a, from: 5, path: 'l2' })
  assert.equal(await move.getByText('Unconfirmed', { exact: true }).count(), 1)
  await matrix('nc', '[data-testid="wallet-nc-migrate"]')
  await main.getByTestId('wallet-nc-next-step').click()
  await move.getByText('Proposal #61: Open').waitFor()
  assert.deepEqual(last('wallet/nc_migrate_start').params, { wallet: wallets.a, from: 5, path: 'l2' })
  assert.equal(await main.getByTestId('wallet-nc-next-step').isDisabled(), true, 'nothing more to propose while the sweep waits')
  await main.keyboard.press('Escape')
  await move.waitFor({ state: 'detached' })
  await main.getByTestId('wallet-nc-move').waitFor()

  // This device's native key root: without a Secure Enclave, a root in software is the person's explicit choice.
  let root = null
  methods['wallet/native_status'] = () => ({
    created: !!root, protection: root, protectors: [], created_at: root && Date.now(), hardware: null,
    key: root && { wallet: wallets.a, public_key: '9f'.repeat(32), created_at: Date.now() }
  })
  methods['wallet/native_create'] = p => {
    if (!p.software) throw { code: 'UNAVAILABLE', message: 'No Secure Enclave on this device' }
    root = 'software'
    return null
  }
  methods['wallet/bindings'] = p => ({ wallet: p.wallet, bindings: [], unbind: ['12'.repeat(32)] })
  await main.getByTestId('wallet-tab-team').click()
  await main.getByTestId('wallet-tab-settings').click()
  await main.getByTestId('wallet-native-create').click()
  await main.getByTestId('wallet-native-software').waitFor()
  await main.getByTestId('wallet-native-software-confirm').click()
  await main.getByTestId('wallet-native').getByText('Software', { exact: true }).waitFor()
  assert.deepEqual(last('wallet/native_create').params, { software: true, biometry_only: false })
  // A revoked device's binding is offered for removal; a key this device does not hold is replaced.
  await main.getByTestId('wallet-unbind').getByRole('button', { name: 'Unbind', exact: true }).click()
  await waitFor(() => last('wallet/unbind'))
  assert.deepEqual(last('wallet/unbind').params, { wallet: wallets.a, devices: ['12'.repeat(32)], source: 'gui' })
  await main.getByTestId('wallet-native').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'succession-wallet-native.png'), animations: 'disabled' })
  await main.getByRole('button', { name: /Wallet cosigners.*Technical details/ }).click()
  assert.equal(await main.getByTestId('wallet-replace-key').count(), 1, 'only the P-256 key of a lost device is replaceable')
  await main.getByTestId('wallet-replace-key').click()
  const replace = main.getByTestId('wallet-replace-key-dialog')
  await replace.waitFor()
  await main.screenshot({ path: join(shots, 'succession-wallet-replace-key.png'), animations: 'disabled' })
  await replace.getByRole('button', { name: 'Ask for approval', exact: true }).click()
  await replace.waitFor({ state: 'detached' })
  assert.deepEqual(last('wallet/change').params.change, { kind: 'replace_cosigner', id: 8, label: 'This Mac' })

  // Leaving content and addresses behind is recorded, and the revocation lists what stays.
  await go('/settings/devices')
  await main.getByTestId('succession-native').click()
  await main.getByTestId('succession-addresses').waitFor()
  await main.getByTestId('succession-step-native_addresses').getByRole('button', { name: 'Leave behind', exact: true }).click()
  await main.getByTestId('succession-leave').getByText('keep needing the old device', { exact: false }).waitFor()
  await main.getByTestId('succession-leave-confirm').click()
  await waitFor(() => last('device/succession_skip'))
  assert.deepEqual(last('device/succession_skip').params, { id: successor.id, step: 'native_addresses', skipped: [`${wallets.a} #5`] })
  await main.getByTestId('succession-step-content').getByRole('button', { name: 'Leave behind', exact: true }).click()
  await main.getByTestId('succession-leave').getByText('stays only on the old device', { exact: false }).waitFor()
  await main.getByTestId('succession-leave-confirm').click()
  await waitFor(() => last('device/succession_skip').params.step === 'content')
  assert.deepEqual(last('device/succession_skip').params.skipped, ['library'])
  await main.getByTestId('succession-revoke').click()
  const revoke = main.getByTestId('succession-revoke-dialog')
  await revoke.waitFor()
  const left = await revoke.innerText()
  assert.ok(left.includes('Files') && left.includes(`${wallets.a} #5`), left)
  await main.getByTestId('succession-revoke-dialog-confirm').click()
  await waitFor(() => last('device/succession_run')?.params.step === 'revoke')
  successor.steps.revoke = done({ account_version: 9 })
  await changed()
  await main.getByTestId('succession').getByText('Handover complete.', { exact: false }).waitFor()
  assert.equal(await main.getByTestId('succession-task').getByRole('button', { name: 'Stop handover', exact: true }).count(), 0)

  // Files and the strict device binding say what only this device holds.
  await go('/files')
  await main.getByTestId('files-local-only').waitFor()
  await go('/settings/security')
  await main.getByTestId('device-bound-toggle').click()
  await main.getByTestId('device-bound-local-only').waitFor()
  await main.keyboard.press('Escape')
  await main.getByTestId('security-editor').waitFor({ state: 'detached' })

  // A new Mac that is not yet a device asks to replace an administrator device, or has a copy approve it.
  methods['device/moved_list'] = () => [{ id: `${account}-56565656`, account, device_id: old, moved_at: Date.now(), reason: 'anchor' }]
  methods['device/moved_approve_successor'] = p => request('device_pair', { device_id: me, role: 'Administrator', capabilities, moved: p.id })
  methods['device/succession_request'] = p => {
    assert.equal(p.predecessor, old)
    return { id: 'b3'.repeat(32), token, device_id: me, approval_key: true, cosigner: false, qr: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==' }
  }
  setSession('pairing_required')
  emit('session/changed', { state: 'pairing_required' })
  await go('/settings/session')
  await main.getByTestId('moved-replace').click()
  await waitFor(() => last('device/moved_approve_successor'))
  assert.deepEqual(last('device/moved_approve_successor').params, { id: `${account}-56565656`, source: 'gui' })
  await main.getByTestId('succession-request').getByRole('button').first().click()
  await main.getByTestId('succession-predecessor').fill(old.slice(2))
  assert.equal(await main.getByTestId('succession-request-create').isDisabled(), true)
  await main.getByTestId('succession-predecessor').fill(old)
  await main.getByTestId('succession-request-create').click()
  await main.getByTestId('succession-request-made').waitFor()
  // The old device scans the request or pastes it.
  await main.getByTestId('succession-qr').waitFor()
  assert.equal(await main.getByTestId('succession-token-made').inputValue(), token)
  assert.equal(await main.getByTestId('succession-token-made').evaluate(el => el.readOnly && el.labels.length === 1), true)
  await matrix('request', '[data-testid="succession-request"]')

  // Once this Mac is a device, the copy's files import into the handover's content step.
  tasks = [{ id: 'b3'.repeat(32), side: 'successor', predecessor: old, successor: me, created_at: Date.now(), steps: { authorize: done({ account_version: 10, added_at: Date.now() }) } }]
  setSession('unlocked')
  emit('session/changed', { state: 'unlocked' })
  await main.getByTestId('moved-import').click()
  await main.getByTestId('moved-notice').getByText('Imported 12 records', { exact: false }).waitFor()
  assert.deepEqual(last('device/moved_import').params, { id: `${account}-56565656`, succession: 'b3'.repeat(32) })
  console.log('Succession: review and approval, both sides\' steps, transfers, leaving behind, revocation, native addresses and key root, local-only notices, the new Mac\'s request and 16 matrix screenshots passed')
}
