import { chromium, expect, test } from '@playwright/test'
import { build } from 'vite'
import { spawn, type ChildProcess } from 'node:child_process'
import { cp, mkdtemp, readFile, writeFile, rm, mkdir } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

test('MV3 → real user/COSE → workerd account and root initialization', async ({}, testInfo) => {
  test.skip(
    !process.env.DMSG_CLOUD_DIR,
    'Set DMSG_CLOUD_DIR for the private local relay harness'
  )
  test.setTimeout(300000)
  const app = resolve('.'),
    root = resolve('../..'),
    cloud = resolve(process.env.DMSG_CLOUD_DIR!)
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-account-probe-'))
  const extension = join(dir, 'extension'),
    children: ChildProcess[] = []
  let browser: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  const logs: string[] = []
  const extraBrowsers: Awaited<ReturnType<typeof chromium.launchPersistentContext>>[] = []
  function start(command: string, args: string[], cwd: string, fixtureDir = dir) {
    const child = spawn(command, args, {
      cwd,
      env: {
        ...process.env,
        DMSG_CLOUD_FIXTURE_DIR: fixtureDir,
        DMSG_SHARED_FIXTURE: process.env.DMSG_MIGRATION_PROBE === '1' ? '1' : '',
        WRANGLER_SEND_METRICS: 'false'
      },
      stdio: ['ignore', 'pipe', 'pipe']
    })
    child.stdout!.on('data', (data) => logs.push(String(data)))
    child.stderr!.on('data', (data) => logs.push(String(data)))
    child.on('error', (error) => logs.push(error.message))
    children.push(child)
    return child
  }
  async function readWhenReady(file: string, child: ChildProcess, fixtureDir = dir) {
    const end = Date.now() + 120000
    while (Date.now() < end) {
      try {
        return JSON.parse(await readFile(join(fixtureDir, file), 'utf8'))
      } catch {
        /* atomic file publication */
      }
      if (child.exitCode !== null) throw new Error(logs.join(''))
      await new Promise((done) => setTimeout(done, 100))
    }
    throw new Error(`Timeout waiting for ${file}\n${logs.join('')}`)
  }
  try {
    const gateway = start(
      'cargo',
      [
        'test',
        '--locked',
        '-p',
        'dmsg_integration',
        '--features',
        'pocketic-tests',
        '--test',
        'account_fixture',
        '--',
        '--ignored',
        '--nocapture'
      ],
      root
    )
    const fixture = await readWhenReady('fixture.json', gateway)
    if (process.env.DMSG_MIGRATION_PROBE === '1') {
      await mkdir(join(dir, 'legacy'))
      const sourceGateway = start(
        'cargo',
        [
          'test',
          '--locked',
          '-p',
          'dmsg_integration',
          '--features',
          'pocketic-tests',
          '--test',
          'legacy_fixture',
          '--',
          '--ignored',
          '--nocapture'
        ],
        root,
        join(dir, 'legacy')
      )
      fixture.legacy = await readWhenReady('fixture.json', sourceGateway, join(dir, 'legacy'))
    }
    await cp(resolve('dist'), extension, { recursive: true })
    await build({
      configFile: false,
      root: app,
      publicDir: false,
      logLevel: 'warn',
      build: {
        target: 'chrome120',
        outDir: extension,
        emptyOutDir: false,
        rollupOptions: { input: resolve('e2e/account-probe.html') }
      }
    })
    // Only this disposable test build can contact arbitrary loopback test ports.
    const manifestPath = join(extension, 'manifest.json')
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
    manifest.host_permissions.push('http://127.0.0.1/*', 'http://localhost/*')
    manifest.content_security_policy.extension_pages =
      manifest.content_security_policy.extension_pages.replace(
        "connect-src 'self'",
        "connect-src 'self' http://127.0.0.1:* http://localhost:*"
      )
    await writeFile(manifestPath, JSON.stringify(manifest))
    browser = await chromium.launchPersistentContext(join(dir, 'chrome'), {
      headless: true,
      ignoreDefaultArgs: ['--disable-extensions'],
      ...(process.env.DMSG_TEST_CHROME
        ? { executablePath: process.env.DMSG_TEST_CHROME }
        : { channel: 'chromium' }),
      args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
    })
    const worker = browser.serviceWorkers()[0] || (await browser.waitForEvent('serviceworker'))
    const extensionOrigin = `chrome-extension://${new URL(worker.url()).host}`
    const port = await new Promise<number>((done, reject) => {
      const socket = createServer()
      socket.once('error', reject)
      socket.listen(0, '127.0.0.1', () => {
        const address = socket.address()
        if (!address || typeof address === 'string') return reject(new Error('No port'))
        socket.close(() => done(address.port))
      })
    })
    await writeFile(
      join(dir, 'cloud-launch.json'),
      JSON.stringify({ ...fixture, extensionOrigin, port })
    )
    const relay = start(
      process.execPath,
      [join(cloud, 'dmsg-core/scripts/extension-probe-server.mjs')],
      cloud
    )
    const { relay: origin } = await readWhenReady('cloud-ready.json', relay)
    const page = await browser.newPage()
    await page.goto(`${extensionOrigin}/e2e/account-probe.html`)
    await page.waitForFunction(() => 'accountProbe' in window)
    const results = await page.evaluate((input) => (window as any).accountProbe(input), {
      ...fixture,
      relay: origin
    })
    expect(results.account).toMatch(/^[0-9a-v]{20}$/)
    expect(results.rootCommitted).toBe(true)
    expect(results.converted).toBe(true)
    expect(results.lostReplyResumed).toBe(true)
    expect(results.concurrentCreateBlocked).toBe(true)

    console.log('Local root derivation charged cycles:', results.deriveCost)
    await page.evaluate(() => (window as any).accountFollowup('capture-evidence'))
    const fixtureBackup = await page.evaluate(() => (window as any).accountFollowup('fixture'))
    async function freshPage(name: string) {
      const context = await chromium.launchPersistentContext(join(dir, name), {
        headless: true,
        ignoreDefaultArgs: ['--disable-extensions'],
        ...(process.env.DMSG_TEST_CHROME
          ? { executablePath: process.env.DMSG_TEST_CHROME }
          : { channel: 'chromium' }),
        args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
      })
      extraBrowsers.push(context)
      const tab = await context.newPage()
      await tab.goto(`${extensionOrigin}/e2e/account-probe.html`)
      await tab.waitForFunction(() => 'accountFollowup' in window)
      return { context, tab }
    }
    if (process.env.DMSG_DELIVERY_PROBE === '1') {
      const recipient = await freshPage('paid-contact-recipient')
      const other = await recipient.tab.evaluate(
        (input) => (window as any).accountProbe(input),
        { ...fixture, relay: origin, identitySeed: 62 }
      )
      await recipient.tab.evaluate(() =>
        (window as any).accountFollowup('inbox', { action: 'capability' })
      )
      await recipient.tab.evaluate(() => (window as any).accountFollowup('rotate'))
      await recipient.tab.evaluate(() =>
        (window as any).accountFollowup('inbox', { action: 'configure' })
      )
      const order = await page.evaluate(
        (recipient) =>
          (window as any).accountFollowup('inbox', { action: 'contact', recipient }),
        other.account
      )
      expect(order.state).toBe('quoted')
      const resumed = await page.evaluate(() =>
        (window as any).accountFollowup('inbox', { action: 'resume' })
      )
      expect(resumed.order_id).toBe(order.order_id)
      expect(resumed.ciphertext).toBe(order.ciphertext)
      const admitted = await page.evaluate(
        (order) => (window as any).accountFollowup('inbox', { action: 'pay-admit', order }),
        order
      )
      expect(admitted).toMatchObject({ state: 'visible', decision: 'SettlementCommitted' })
      const messages = await recipient.tab.evaluate(() =>
        (window as any).accountFollowup('inbox', { action: 'list' })
      )
      expect(messages[0]).toMatchObject({ text: 'Private paid contact', state: 'visible' })
      await writeFile(
        testInfo.outputPath('delivery-result.json'),
        JSON.stringify({ admitted, messages }, null, 2)
      )
      return
    }
    if (process.env.DMSG_COMMERCE_PROBE === '1') {
      await readWhenReady('commerce-ready.json', gateway)
      const quote = await page.evaluate(() =>
        (window as any).accountFollowup('commerce', { action: 'quote' })
      )
      const opened = await page.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'open', id }),
        quote.id
      )
      expect(opened).toMatchObject({ state: 'AwaitingFunding', calls: 1 })
      const funded = await page.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'fund', id }),
        quote.id
      )
      expect(funded).toMatchObject({ state: 'Applied', lost: true })
      const entitlement = await page.evaluate(() =>
        (window as any).accountFollowup('commerce', { action: 'entitlement' })
      )
      expect(entitlement.plan).toBe('Plus')
      const refunded = await page.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'refund', id }),
        quote.id
      )
      expect(refunded).toEqual({ state: 'Applied', refused: true })
      // A separate product beneficiary pays through the other registered ledger.
      const usdtDevice = await freshPage('usdt-subject')
      await usdtDevice.tab.evaluate((input) => (window as any).accountProbe(input), {
        ...fixture,
        relay: origin,
        identitySeed: 62
      })
      const usdtQuote = await usdtDevice.tab.evaluate(() =>
        (window as any).accountFollowup('commerce', { action: 'quote', asset: 'CkUsdt' })
      )
      expect(usdtQuote.asset).toBe('CkUsdt')
      const usdtOpen = await usdtDevice.tab.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'open', id }),
        usdtQuote.id
      )
      expect(usdtOpen).toMatchObject({ state: 'AwaitingFunding', calls: 1 })
      const usdtFunded = await usdtDevice.tab.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'fund', id }),
        usdtQuote.id
      )
      expect(usdtFunded).toMatchObject({ state: 'Applied', lost: true })
      expect(quote.asset).toBe('CkUsdc')
      const snsDevice = await freshPage('membership-subject')
      await snsDevice.tab.evaluate((input) => (window as any).accountProbe(input), {
        ...fixture,
        relay: origin,
        identitySeed: 61
      })
      const sns = await snsDevice.tab.evaluate(() =>
        (window as any).accountFollowup('commerce', { action: 'sns' })
      )
      const reviewedSns = await snsDevice.tab.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'review-sns', id }),
        sns.id
      )
      expect(reviewedSns.actor).toBe(sns.actor)
      expect(reviewedSns.neuron).toBe('4d'.repeat(32))
      expect(sns.eligibility).toBe('Eligible')
      expect(sns.status).toBe('CoolingDown')
      await writeFile(join(dir, 'advance-commerce-clock'), '')
      const advanced = await readWhenReady('commerce-clock.json', gateway)
      await snsDevice.tab.clock.setSystemTime(advanced.at + 1000)
      const activated = await snsDevice.tab.evaluate(
        (id) => (window as any).accountFollowup('commerce', { action: 'advance-sns', id }),
        sns.id
      )
      expect(activated.status).toBe('Active')
      await writeFile(
        testInfo.outputPath('commerce-result.json'),
        JSON.stringify(
          {
            quote,
            opened,
            funded,
            usdtQuote,
            usdtOpen,
            usdtFunded,
            entitlement,
            refunded,
            sns,
            active: activated.status
          },
          null,
          2
        )
      )
      return
    }
    if (process.env.DMSG_MIGRATION_PROBE === '1') {
      const second = await freshPage('migration-manager-two')
      const other = await second.tab.evaluate((input) => (window as any).accountProbe(input), {
        ...fixture,
        relay: origin,
        identitySeed: 60
      })
      let draft = await page.evaluate(() =>
        (window as any).accountFollowup('shared', { action: 'prepare' })
      )
      const pending = await page.evaluate(
        (draft) =>
          (window as any).accountFollowup('shared', {
            action: 'propose-lost',
            draft,
            seed: 52
          }),
        draft
      )
      expect(Object.keys(pending.votes)).toHaveLength(1)
      await expect(
        page.evaluate(
          (view) => (window as any).accountFollowup('shared', { action: 'commit', view }),
          pending
        )
      ).rejects.toThrow()
      await expect(
        second.tab.evaluate(
          (draft) =>
            (window as any).accountFollowup('shared', { action: 'consent', draft, seed: 54 }),
          draft
        )
      ).rejects.toThrow()
      const oldDraft = draft
      const candidates = await Promise.all([
        page.evaluate(() =>
          (window as any).accountFollowup('shared', { action: 'prepare', version: 2 })
        ),
        second.tab.evaluate(() =>
          (window as any).accountFollowup('shared', {
            action: 'prepare',
            version: 2,
            seed: 53
          })
        )
      ])
      const raced = await Promise.allSettled([
        page.evaluate(
          (draft) =>
            (window as any).accountFollowup('shared', { action: 'propose', draft, seed: 52 }),
          candidates[0]
        ),
        second.tab.evaluate(
          (draft) =>
            (window as any).accountFollowup('shared', { action: 'propose', draft, seed: 53 }),
          candidates[1]
        )
      ])
      expect(raced.filter((r) => r.status === 'fulfilled')).toHaveLength(1)
      if (raced[0].status === 'rejected')
        expect(String(raced[0].reason)).not.toContain('IDEMPOTENCY_CONFLICT')
      // Continue the same winning owner; use per-page aliases for the rest.
      const winner = raced.findIndex((r) => r.status === 'fulfilled')
      const ownerPage = winner === 0 ? page : second.tab,
        memberPage = winner === 0 ? second.tab : page
      const ownerSeed = winner === 0 ? 52 : 53,
        memberSeed = winner === 0 ? 53 : 52
      const memberAccount = winner === 0 ? other.account : results.account
      draft = candidates[winner]
      await expect(
        memberPage.evaluate(
          (draft) =>
            (window as any).accountFollowup('shared', { action: 'consent', draft, seed: 53 }),
          oldDraft
        )
      ).rejects.toThrow()
      const agreed = await memberPage.evaluate(
        (data) => (window as any).accountFollowup('shared', data),
        { action: 'consent', draft, seed: memberSeed }
      )
      expect(Object.keys(agreed.votes)).toHaveLength(2)
      const committed = await ownerPage.evaluate(
        (view) => (window as any).accountFollowup('shared', { action: 'commit', view }),
        agreed
      )
      const active = await ownerPage.evaluate(
        (view) => (window as any).accountFollowup('shared', { action: 'activate', view }),
        committed
      )
      expect(active.stage).toBe('active')
      const claimed = await memberPage.evaluate(
        (data) => (window as any).accountFollowup('shared', data),
        { action: 'claim', view: active, seed: memberSeed }
      )
      const invitation = await ownerPage.evaluate(
        (data) => (window as any).accountFollowup('channel', data),
        { action: 'invite', id: active.proposal.channel_id, account: memberAccount }
      )
      await memberPage.evaluate(
        (invitation) =>
          (window as any).accountFollowup('channel', { action: 'join', invitation }),
        invitation
      )
      await ownerPage.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'rotate', id }),
        active.proposal.channel_id
      )
      const joined = await memberPage.evaluate(
        (view) => (window as any).accountFollowup('shared', { action: 'joined', view }),
        claimed
      )
      expect(Object.values(joined.claims).some((value: any) => value.joined)).toBe(true)
      const granted = await ownerPage.evaluate(
        (data) => (window as any).accountFollowup('shared', data),
        {
          action: 'share',
          view: joined,
          member: Object.keys(joined.claims)[0],
          seed: ownerSeed
        }
      )
      const received = await memberPage.evaluate(
        (view) =>
          (window as any).accountFollowup('shared', {
            action: 'receive',
            view,
            grant: Object.keys(view.grants)[0]
          }),
        granted
      )
      await expect(
        memberPage.evaluate(
          (view) =>
            (window as any).accountFollowup('shared', {
              action: 'receive',
              view,
              grant: Object.keys(view.grants)[0],
              recovery: true,
              wrong: true
            }),
          granted
        )
      ).rejects.toThrow()
      const recovered = await memberPage.evaluate(
        (view) =>
          (window as any).accountFollowup('shared', {
            action: 'receive',
            view,
            grant: Object.keys(view.grants)[0],
            recovery: true
          }),
        granted
      )
      expect(recovered.messages.some((m: any) => m.text === 'Legacy shared history 0')).toBe(
        true
      )
      expect(received.messages.some((m: any) => m.text === 'Legacy shared history 0')).toBe(
        true
      )
      expect(received.stage).toBe('content_verified')
      await writeFile(
        testInfo.outputPath('shared-result.json'),
        JSON.stringify(
          {
            key: joined.key,
            proposal: joined.proposal_digest,
            managers: Object.keys(joined.votes).length,
            claims: Object.keys(joined.claims).length,
            stage: joined.stage
          },
          null,
          2
        )
      )
      return
    }
    if (process.env.DMSG_CHANNEL_PROBE === '1') {
      const second = await freshPage('channel-second')
      const other = await second.tab.evaluate((input) => (window as any).accountProbe(input), {
        ...fixture,
        relay: origin,
        identitySeed: 60
      })
      const created = await page.evaluate(() =>
        (window as any).accountFollowup('channel', { action: 'create' })
      )
      const attachment = await page.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'file', id }),
        created.id
      )
      const invitation = await page.evaluate(
        (data) => (window as any).accountFollowup('channel', data),
        { action: 'invite', id: created.id, account: other.account }
      )
      await second.tab.evaluate(
        (invitation) =>
          (window as any).accountFollowup('channel', { action: 'join', invitation }),
        invitation
      )
      const rotated = await page.evaluate(
        (id) =>
          (window as any).accountFollowup('channel', { action: 'rotate', id, lose: true }),
        created.id
      )
      expect(rotated.ledger.epoch).toBe(2)
      expect(rotated.lost).toBe(true)
      const initial = await second.tab.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'sync', id }),
        created.id
      )
      expect(initial.through).toBe(2)
      expect(initial.messages).toHaveLength(0)
      await expect(
        second.tab.evaluate((data) => (window as any).accountFollowup('channel', data), {
          action: 'download',
          id: created.id,
          seq: attachment.receipt.seq
        })
      ).rejects.toThrow()
      await page.evaluate(
        (id) =>
          (window as any).accountFollowup('channel', {
            action: 'send',
            id,
            text: 'new shared epoch message'
          }),
        created.id
      )
      const received = await second.tab.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'sync', id }),
        created.id
      )
      expect(received.messages.map((m: any) => m.text)).toEqual(['new shared epoch message'])
      await second.tab.evaluate(
        (id) =>
          (window as any).accountFollowup('channel', {
            action: 'send',
            id,
            text: 'member reply'
          }),
        created.id
      )
      await page.evaluate((data) => (window as any).accountFollowup('channel', data), {
        action: 'history',
        id: created.id,
        account: other.account,
        from: 1,
        to: 1
      })
      const historical = await second.tab.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'backfill', id }),
        created.id
      )
      expect(historical.messages.map((m: any) => m.text)).toContain(
        'owner history before invitation'
      )
      const downloadedAttachment = await second.tab.evaluate(
        (data) => (window as any).accountFollowup('channel', data),
        { action: 'download', id: created.id, seq: attachment.receipt.seq }
      )
      expect(downloadedAttachment.size).toBe(1024 * 1024 + 17)
      const proposal = await page.evaluate(
        (data) => (window as any).accountFollowup('channel', data),
        { action: 'owner-propose', id: created.id, account: other.account }
      )
      const accepted = await second.tab.evaluate(
        (packet) =>
          (window as any).accountFollowup('channel', { action: 'owner-accept', packet }),
        proposal
      )
      const transferred = await page.evaluate(
        (packet) =>
          (window as any).accountFollowup('channel', { action: 'owner-commit', packet }),
        accepted
      )
      expect(transferred.owner).toBe(other.account)
      const back = await second.tab.evaluate(
        (data) => (window as any).accountFollowup('channel', data),
        { action: 'owner-propose', id: created.id, account: created.account }
      )
      const backAccepted = await page.evaluate(
        (packet) =>
          (window as any).accountFollowup('channel', { action: 'owner-accept', packet }),
        back
      )
      await second.tab.evaluate(
        (packet) =>
          (window as any).accountFollowup('channel', { action: 'owner-commit', packet }),
        backAccepted
      )
      const billingProposal = await page.evaluate(
        (data) => (window as any).accountFollowup('channel', data),
        { action: 'sponsor-propose', id: created.id, account: other.account }
      )
      const billingAcceptance = await second.tab.evaluate(
        (packet) =>
          (window as any).accountFollowup('channel', { action: 'sponsor-accept', packet }),
        billingProposal
      )
      const billing = await page.evaluate(
        (packet) =>
          (window as any).accountFollowup('channel', { action: 'sponsor-commit', packet }),
        billingAcceptance
      )
      expect(billing.billing_account).toBe(other.account)
      const deviceBackup = await second.tab.evaluate(() =>
        (window as any).accountFollowup('export-local')
      )
      const third = await freshPage('channel-new-device')
      const thirdRestored = await third.tab.evaluate(
        (data) => (window as any).accountFollowup('restore', data),
        deviceBackup
      )
      const thirdPair = await third.tab.evaluate(() =>
        (window as any).accountFollowup('pair', 'Administrator')
      )
      await second.tab.evaluate(
        (packet) => (window as any).accountFollowup('approve', packet),
        thirdPair
      )
      await second.tab.evaluate(() => (window as any).accountFollowup('rotate'))
      await third.tab.evaluate(() => (window as any).accountFollowup('open'))
      const addedDeviceEpoch = await page.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'rotate', id }),
        created.id
      )
      expect(addedDeviceEpoch.ledger.epoch).toBe(3)
      await third.tab.evaluate(
        (id) =>
          (window as any).accountFollowup('channel', {
            action: 'send',
            id,
            text: 'new approved device'
          }),
        created.id
      )
      await third.tab.evaluate(
        (device) => (window as any).accountFollowup('revoke', device),
        other.device
      )
      await third.tab.evaluate(() => (window as any).accountFollowup('rotate'))
      const revokedDeviceEpoch = await page.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'rotate', id }),
        created.id
      )
      expect(revokedDeviceEpoch.ledger.epoch).toBe(4)
      await expect(
        second.tab.evaluate(
          (id) =>
            (window as any).accountFollowup('channel', {
              action: 'send',
              id,
              text: 'revoked device cannot send'
            }),
          created.id
        )
      ).rejects.toThrow()
      await third.tab.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'sync', id }),
        created.id
      )
      await page.evaluate((data) => (window as any).accountFollowup('channel', data), {
        action: 'remove',
        id: created.id,
        account: other.account
      })
      const afterRemoval = await page.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'rotate', id }),
        created.id
      )
      expect(afterRemoval.ledger.epoch).toBe(5)
      await expect(
        second.tab.evaluate(
          (id) =>
            (window as any).accountFollowup('channel', {
              action: 'send',
              id,
              text: 'revoked member cannot send'
            }),
          created.id
        )
      ).rejects.toThrow()
      const channelBackup = await third.tab.evaluate(() =>
        (window as any).accountFollowup('export-local')
      )
      expect(channelBackup.missing).toEqual([])
      const offlineChannel = await freshPage('channel-offline')
      await offlineChannel.context.setOffline(true)
      const restoredChannel = await offlineChannel.tab.evaluate(
        (data) => (window as any).accountFollowup('restore', data),
        channelBackup
      )
      expect(restoredChannel.authorized).toBe(false)
      const offlineView = await offlineChannel.tab.evaluate(
        (id) => (window as any).accountFollowup('channel', { action: 'offline', id }),
        created.id
      )
      expect(offlineView.messages.map((m: any) => m.text)).toContain(
        'owner history before invitation'
      )
      expect(offlineView.files).toContain('channel-history.bin')
      await writeFile(
        testInfo.outputPath('channel-result.json'),
        JSON.stringify(
          {
            channel: created.id,
            initial,
            received,
            epochs: [rotated.ledger.epoch, afterRemoval.ledger.epoch]
          },
          null,
          2
        )
      )
      return
    }
    if (process.env.DMSG_SIGNING_PROBE === '1') {
      const handle = await page.evaluate(() => (window as any).accountFollowup('handle'))
      expect(handle).toMatchObject({ phase: 'claimed', calls: 1, snapshotCount: '1' })
      const signing = await page.evaluate(() => (window as any).accountFollowup('sign'))
      expect(signing).toMatchObject({
        stage: 'complete',
        dispatches: 1,
        navigationRejected: true,
        artifact: true,
        receipt: true
      })
      await writeFile(
        testInfo.outputPath('signing-result.json'),
        JSON.stringify({ signing, handle }, null, 2)
      )
      return
    }
    if (process.env.DMSG_CONTENT_PROBE === '1') {
      const uploaded = await page.evaluate(() =>
        (window as any).accountFollowup('content', {
          file: true,
          push: true,
          lose: true,
          expirePlan: true
        })
      )
      expect(uploaded.lost).toBe(3)
      expect(uploaded.file).toBe(true)
      const second = await freshPage('content-second')
      const restored = await second.tab.evaluate(
        (data) => (window as any).accountFollowup('restore', data),
        fixtureBackup
      )
      expect(restored.authorized).toBe(false)
      await page.evaluate(() =>
        (window as any).accountFollowup('content', {
          edit: 'prepared before root rotation',
          prepare: true
        })
      )
      const pair = await second.tab.evaluate(() =>
        (window as any).accountFollowup('pair', 'Member')
      )
      await page.evaluate((packet) => (window as any).accountFollowup('approve', packet), pair)
      await second.tab.evaluate(() => (window as any).accountFollowup('open'))
      // Device approval invalidates vault writes until an administrator commits
      // a new root. Content sync must not bypass that canister policy.
      await page.evaluate(() => (window as any).accountFollowup('rotate'))
      await second.tab.evaluate(() => (window as any).accountFollowup('open'))
      await page.evaluate(() => (window as any).accountFollowup('content', { push: true }))
      const downloaded = await second.tab.evaluate(() =>
        (window as any).accountFollowup('content', {})
      )
      expect(downloaded.file).toBe(true)
      expect(downloaded.note).toBe('prepared before root rotation')
      await page.evaluate(() =>
        (window as any).accountFollowup('content', { backgroundRestore: true })
      )
      await second.tab.evaluate(() => (window as any).accountFollowup('content', {}))
      await page.evaluate(() =>
        (window as any).accountFollowup('content', { edit: 'first device revision' })
      )
      await second.tab.evaluate(() =>
        (window as any).accountFollowup('content', {
          edit: 'concurrent second device revision'
        })
      )
      await page.evaluate(() => (window as any).accountFollowup('content', { push: true }))
      const conflicted = await second.tab.evaluate(() =>
        (window as any).accountFollowup('content', { push: true })
      )
      expect(conflicted.conflicts).toBeGreaterThan(0)
      const backup = await second.tab.evaluate(() =>
        (window as any).accountFollowup('fixture')
      )
      const offline = await freshPage('content-offline')
      await offline.context.setOffline(true)
      const recovered = await offline.tab.evaluate(
        (data) => (window as any).accountFollowup('restore', data),
        backup
      )
      expect(recovered.entries).toBe(conflicted.entries)
      expect(recovered.authorized).toBe(false)
      await writeFile(
        testInfo.outputPath('content-result.json'),
        JSON.stringify({ uploaded, downloaded, conflicted, recovered }, null, 2)
      )
      return
    }
    const second = await freshPage('second-device')
    const restored = await second.tab.evaluate(
      (data) => (window as any).accountFollowup('restore', data),
      fixtureBackup
    )
    expect(restored.authorized).toBe(false)
    const pair = await second.tab.evaluate(() =>
      (window as any).accountFollowup('pair', 'Administrator')
    )
    await page.evaluate((packet) => (window as any).accountFollowup('approve', packet), pair)
    const race = await Promise.allSettled([
      page.evaluate(() => (window as any).accountFollowup('rotate')),
      second.tab.evaluate(() => (window as any).accountFollowup('rotate'))
    ])
    expect(race.filter((r) => r.status === 'fulfilled')).toHaveLength(1)
    const winner = race.findIndex((r) => r.status === 'fulfilled')
    const rotated = (race[winner] as PromiseFulfilledResult<any>).value
    expect(rotated.generation).toBeGreaterThan(1)
    const loserPage = winner === 0 ? second.tab : page
    const online = await loserPage.evaluate(() => (window as any).accountFollowup('open'))
    expect(online.generation).toBe(rotated.generation)
    await page.evaluate(
      (device) => (window as any).accountFollowup('revoke', device),
      restored.device
    )
    const afterRevocation = await page.evaluate(() =>
      (window as any).accountFollowup('rotate')
    )
    expect(afterRevocation.generation).toBeGreaterThan(rotated.generation)
    await expect(
      page.evaluate(() => (window as any).accountFollowup('stale-evidence'))
    ).rejects.toThrow(/POLICY_STALE|更旧/)
    await expect(
      second.tab.evaluate(() => (window as any).accountFollowup('open'))
    ).rejects.toThrow(/DeviceNotApproved/)
    const latestBackup = await page.evaluate(() => (window as any).accountFollowup('fixture'))
    const offline = await freshPage('offline-recovery')
    await offline.context.setOffline(true)
    const offlineResult = await offline.tab.evaluate(
      (data) => (window as any).accountFollowup('restore', data),
      latestBackup
    )
    expect(offlineResult.authorized).toBe(false)
    expect(offlineResult.generation).toBe(afterRevocation.generation)
    expect(offlineResult.entries).toBe(afterRevocation.entries)
    await second.tab.evaluate(() => (window as any).accountFollowup('request-recovery'))
    await page.evaluate(() => (window as any).accountFollowup('dispute'))
    const recovery = await second.tab.evaluate(() =>
      (window as any).accountFollowup('reconfirm')
    )
    expect(recovery.reconfirmed).toBe(true)
    await expect(
      second.tab.evaluate(() => (window as any).accountFollowup('complete'))
    ).rejects.toThrow(/等待期/)
    await writeFile(join(dir, 'advance-recovery-clock'), '')
    const advanced = await readWhenReady('clock.json', gateway)
    for (const target of [page, second.tab])
      await target.clock.setSystemTime(advanced.at + 1000)
    const completion = await second.tab.evaluate(async () => {
      try {
        return JSON.stringify({
          ok: true,
          result: await (window as any).accountFollowup('complete')
        })
      } catch (error) {
        return JSON.stringify({ ok: false, error: String(error) })
      }
    })
    expect(completion).toBeTruthy()
    const completionResult = JSON.parse(completion)
    expect(completionResult, completion).toMatchObject({ ok: true })
    const recovered = completionResult.result
    expect(recovered.device).toBe(restored.device)
    expect(recovered.rekey).toBe(true)
    expect(recovered.bindings).toHaveLength(1)
    await expect(
      page.evaluate(() => (window as any).accountFollowup('check-authority'))
    ).rejects.toThrow(/AuthRequired/)
    await writeFile(
      testInfo.outputPath('account-result.json'),
      JSON.stringify(
        { ...results, rotated, online, afterRevocation, offline: offlineResult, recovered },
        null,
        2
      )
    )
  } finally {
    await writeFile(join(dir, 'stop'), '')
    if (process.env.DMSG_MIGRATION_PROBE === '1')
      await writeFile(join(dir, 'legacy', 'stop'), '')
    for (const extra of extraBrowsers) await extra.close()
    await browser?.close()
    for (const child of [...children].reverse()) {
      if (child.exitCode !== null || child.signalCode !== null) continue
      // Give the Rust fixture time to read its stop file and drop PocketIC.
      // Killing cargo immediately can orphan its still-running test executable.
      if (child.spawnfile !== 'cargo') child.kill('SIGTERM')
      await new Promise<void>((done) => {
        const terminate = setTimeout(() => child.kill('SIGTERM'), 2000)
        const force = setTimeout(() => child.kill('SIGKILL'), 5000)
        child.once('exit', () => {
          clearTimeout(terminate)
          clearTimeout(force)
          done()
        })
      })
    }
    await writeFile(testInfo.outputPath('cloud-processes.log'), logs.join(''))
    await rm(dir, { recursive: true, force: true })
  }
})
