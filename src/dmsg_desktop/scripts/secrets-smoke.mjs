// Real Electron windows and IPC against a local metadata-only daemon fixture.
// No network credential use, OS presence prompt or GitHub side effect.
import assert from 'node:assert/strict'
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g',
  token = 'secrets-ui-fixture',
  actor = `did:agent:${'A'.repeat(43)}`
const item = '11'.repeat(32),
  policyId = '22'.repeat(16),
  requests = new Map(),
  calls = [],
  sockets = new Set()
let policies = [],
  uses = [],
  sequence = 1,
  channelPrepared = false
const status = () => ({
  state: 'unlocked',
  account,
  device_id: '33'.repeat(32),
  approvals_waiting: [...requests.values()].filter(
    (r) => r.state === 'awaiting_user'
  ).length,
  pending_sync: 0,
  background_online: false,
  fast_unlock: false,
  fast_presence: null,
  idle_lock_ms: 900000,
  lock_at: Date.now() + 900000
})
const emit = () => {
  for (const s of sockets)
    s.send(
      JSON.stringify({
        jsonrpc: '2.0',
        method: 'event',
        params: { seq: sequence++, type: 'approval/changed', data: null }
      })
    )
}
const makeRequest = (type, detail) => {
  const r = {
    id: (sequence++).toString(16).padStart(64, '0'),
    source: type === 'secret_use' ? `agent:${actor}` : 'gui',
    type,
    kind: 'secret',
    risk: 'high',
    state: 'awaiting_user',
    created_at: Date.now(),
    expires_at: Date.now() + 300000,
    updated_at: Date.now(),
    digest: 'dd'.repeat(32),
    detail
  }
  r.digest = r.id
  requests.set(r.id, r)
  return r
}
const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null,
  'daemon/status': status,
  'session/status': status,
  'profile/show': () => null,
  'profile/pending': () => null,
  'channel/list': () => [{ id: 'c1'.repeat(32), name: 'Snapshot destination' }],
  'attest/history': () => [],
  'grants/list': () => [],
  'files/search': () => [],
  'vault/health': () => [],
  'vault/conflicts': () => [],
  'legacy/pairs': () => [],
  'legacy/channels': () => [],
  'channel/mark': () => ({}),
  'channel/names': () => ({}),
  'profile/display': () => null,
  'plan/status': () => ({}),
  'vault/templates': () => ({ types: [], templates: {} }),
  'vault/list': () => [
    {
      id: item,
      revision: '44'.repeat(32),
      deleted: false,
      item: {
        type: 'api',
        title: 'GitHub credential',
        notes: '',
        tags: [],
        fields: [
          {
            id: 'credential',
            type: 'concealed',
            label: 'Token',
            section: '',
            value: '********'
          }
        ]
      }
    }
  ],
  'agent/ids': () => ({
    agents: [
      { agent_id: actor, label: 'Builder', paused: false, credential: true }
    ]
  }),
  'secrets/policies/list': () => ({ policies }),
  'secrets/uses/list': (p) => {
    const start = p.before
      ? uses.findIndex((u) => u.request_id === p.before) + 1
      : 0
    const page = uses.slice(start, start + 50)
    return { uses: page, device_local: true, next: page.at(-1)?.request_id }
  },
  'secrets/ref': (p) => ({
    uri: `dmsg-secret://${account}/${p.item_id}/${p.field_id}`,
    reference: {
      schema_version: 1,
      account_id: account,
      item_id: p.item_id,
      field_id: p.field_id
    }
  }),
  'secrets/policies/save_request': (p) => {
    assert.equal(p.source, 'gui')
    return makeRequest('secret_policy', {
      id: policyId,
      account_id: account,
      device_id: '33'.repeat(32),
      base_version: p.base_version ?? null,
      policy: p.policy,
      grant_to: p.grant_to
    })
  },
  'secrets/policies/disable': (p) => {
    assert.equal(p.id, policyId)
    policies[0].policy.enabled = false
    policies[0].version++
    emit()
    return policies[0]
  },
  'secrets/uses/mark_reviewed': (p) => {
    const u = uses.find((u) => u.request_id === p.request_id)
    assert.equal(u.state, 'lost')
    u.reviewed_at = Date.now()
    emit()
    return u
  },
  'approval/list': (p) =>
    p.ids_only
      ? [...requests.values()]
          .filter((r) => r.state === 'awaiting_user')
          .map((r) => r.id)
      : [...requests.values()],
  'approval/show': (p) => requests.get(p.id),
  'approval/respond': (p) => {
    const r = requests.get(p.id)
    assert.equal(p.digest, r.digest)
    assert.equal(p.source, 'gui')
    assert.equal(p.human, true)
    r.state = p.approve ? 'done' : 'rejected'
    if (p.approve && r.type === 'secret_policy')
      policies = [{ id: policyId, version: 1, policy: r.detail.policy }]
    if (p.approve && r.type === 'secret_use') {
      r.state = 'lost'
      r.error = 'EXECUTION_UNKNOWN'
      uses = [
        {
          request_id: r.id,
          state: r.state,
          error: r.error,
          created_at: r.created_at,
          approval: r
        }
      ]
    }
    emit()
    return r
  }
}
Object.assign(methods, {
  'secrets/mounts/list': () => ({
    mounts: [
      {
        id: 'c2'.repeat(32),
        input: {
          project_label: 'Fixture project',
          mount_point: '/synthetic/project/.dmsg-secrets'
        }
      }
    ],
    supported: true,
    daemon_ready: true,
    platform: 'macos'
  }),
  'secrets/mounts/status': () => ({
    state: 'stopped',
    error: 'FSKIT_EXTENSION_DISABLED',
    mount_point: '/synthetic/project/.dmsg-secrets'
  }),
  'secrets/ssh/keys': () => ({ keys: [], bindings: [] }),
  'secrets/ssh/status': () => ({
    available: true,
    supported: true,
    socket: '/synthetic/agent.sock',
    identities: 0
  }),
  'secrets/health': () => ({ findings: [] }),
  'secrets/references': (p) => ({
    reference: p.reference,
    digest: 'b1'.repeat(32),
    dependencies: [
      {
        kind: 'policy',
        id: policyId,
        label: 'Repository policy',
        state: 'current',
        action: 'review_policy_and_grants'
      }
    ],
    device_local: true
  }),
  'secrets/rotation/list': () => ({
    rotations: [
      {
        id: 'b2'.repeat(32),
        reference: { item_id: item, field_id: 'credential' },
        new_revision: '44'.repeat(32),
        created: true,
        tested: true,
        references_checked: false
      }
    ]
  }),
  'secrets/rotation/dependencies': () => ({
    revision: '44'.repeat(32),
    digest: 'b3'.repeat(32),
    dependencies: [
      {
        kind: 'snapshot_channel',
        id: 'b4'.repeat(32),
        label: 'Snapshot destination',
        state: 'published',
        action: 'recipients_may_retain_snapshot_rotate_upstream'
      }
    ],
    device_local: true
  }),
  'secrets/rotation/update': (p) => {
    if (p.step === 'references_checked')
      assert.equal(p.dependencies_digest, 'b3'.repeat(32))
    return {}
  },
  'secrets/shares/list': () => ({ shares: [
    { id: 'progress-known', state: 'uploading', uploaded_chunks: 1, total_chunks: 4, plan: { title: 'Upload fixture', expires_at: Date.now() + 60000 } },
    { id: 'progress-unknown', state: 'uploading', uploaded_chunks: 0, total_chunks: 0, plan: { title: 'Preparing upload', expires_at: Date.now() + 60000 } }
  ] }),
  'secrets/shares/prepare': (p) => ({
    id: 'a1'.repeat(32),
    state: 'prepared',
    plan: {
      ...p.input,
      title: 'GitHub credential',
      plain_bytes: 300,
      estimated_ciphertext_bytes: 67000,
      expires_at: Date.now() + 86400000,
      fields: p.input.field_ids.map((id) => ({
        id,
        type: 'concealed',
        label: 'Token'
      }))
    }
  }),
  'secrets/shares/channel_prepare': (p) => {
    channelPrepared = true
    assert.equal(p.channel_id, 'c1'.repeat(32))
    assert.equal(p.input.expires_in_ms, undefined)
    assert.equal(p.password, undefined)
    return {
      id: 'b5'.repeat(32),
      plan: {
        channel: { name: 'Snapshot destination' },
        plain_bytes: 300,
        estimated_ciphertext_bytes: 67000
      }
    }
  },
  'secrets/shares/request': (p) =>
    makeRequest('secret_share', {
      ...(channelPrepared
        ? {
            channel: {
              channel_id: 'c1'.repeat(32),
              name: 'Snapshot destination'
            },
            expires_at: null,
            max_download_sessions: null
          }
        : {}),
      title: 'GitHub credential',
      share_id: 'a2'.repeat(32),
      fields: [{ id: 'credential', type: 'concealed', label: 'Token' }],
      input: { include_notes: false, attachments: [] },
      expires_at: Date.now() + 86400000,
      max_download_sessions: 1
    }),
  'secrets/packages/import_preview': () => ({
    package_id: 'a3'.repeat(32),
    digest: 'a4'.repeat(32),
    preview: {
      title: 'Incoming snapshot',
      fields: [
        { id: 'f0_credential', type: 'concealed', label: 'Imported token' }
      ],
      notes: false,
      attachments: []
    }
  }),
  'secrets/packages/channel_preview': (p) => {
    assert.equal(p.channel_id, 'c1'.repeat(32))
    assert.equal(p.seq, 1)
    return {
      ...methods['secrets/packages/import_preview'](),
      channel_source: { channel_id: p.channel_id, seq: p.seq }
    }
  },
  'secrets/packages/import_request': (p) =>
    makeRequest('secret_package_import', {
      selection: p.selection,
      preview: {
        title: 'Incoming snapshot',
        fields: [
          { id: 'f0_credential', type: 'concealed', label: 'Imported token' }
        ]
      }
    })
})
const channelView = () => ({
  id: 'c1'.repeat(32),
  name: 'Snapshot destination',
  message_cursor: 1,
  read: 0,
  unread: 1,
  readable_epochs: [1],
  draft: '',
  invites: {},
  ledger: {
    status: 'active',
    kind: 'collaboration',
    owner: account,
    epoch: 1,
    members: {
      [account]: { account, role: 'owner', history: [], joined_epoch: 1 }
    },
    head: 'd1'.repeat(32)
  }
})
Object.assign(methods, {
  'channel/list': () => [channelView()],
  'channel/show': channelView,
  'channel/outbox': () => [],
  'channel/sync': () => ({}),
  'channel/receipts': () => ({}),
  'channel/cursor': () => ({}),
  'channel/messages': () => [
    {
      seq: 1,
      account,
      device: '33'.repeat(32),
      stored_at: Date.now(),
      text: '',
      digest: 'd2'.repeat(32),
      file: {
        name: 'secret.dmsg-package',
        mime: 'application/vnd.dmsg.secret-package',
        size: 300
      }
    }
  ]
})
const server = createServer((_, res) => res.writeHead(404).end()),
  wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (req, socket, head) => {
  if (req.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(req, socket, head, (ws) => {
    sockets.add(ws)
    ws.on('close', () => sockets.delete(ws))
    ws.on('message', async (bytes) => {
      const m = JSON.parse(String(bytes))
      calls.push(m)
      try {
        const handler = methods[m.method]
        if (!handler) console.error('Unsupported fixture method:', m.method)
        ws.send(
          JSON.stringify(
            handler
              ? {
                  jsonrpc: '2.0',
                  id: m.id,
                  result: await handler(m.params ?? {})
                }
              : {
                  jsonrpc: '2.0',
                  id: m.id,
                  error: {
                    code: -32601,
                    message: 'Not found',
                    data: { code: 'NOT_FOUND' }
                  }
                }
          )
        )
      } catch (e) {
        ws.send(
          JSON.stringify({
            jsonrpc: '2.0',
            id: m.id,
            error: {
              code: -32000,
              message: String(e),
              data: { code: e.code ?? 'INVALID_INPUT' }
            }
          })
        )
      }
    })
  })
})
await new Promise((r) => server.listen(0, '127.0.0.1', r))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-secrets-ui-')),
  shots = resolve('test-results/secrets')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')
const app = await electron.launch({
  executablePath: electronPath,
  args: [resolve('out/main/index.js')],
  env: {
    ...process.env,
    DMSG_DESKTOP_TEST: '1',
    DMSG_DESKTOP_USER_DATA: profile,
    DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`,
    DMSG_DESKTOP_TOKEN: token,
    DMSG_HOME: join(profile, 'home')
  }
})
async function approve(page) {
  await app.evaluate(({ BrowserWindow }) => {
    for (const w of BrowserWindow.getAllWindows())
      w.webContents.setBackgroundThrottling(false)
  })
  const button = page.getByTestId('approve')
  await button.waitFor()
  await page.waitForFunction(
    () => !document.querySelector('[data-testid="approve"]')?.disabled
  )
  await button.focus()
  await page.keyboard.down('Space')
  try {
    await page.waitForSelector('[data-testid="approval-outcome"]', {
      timeout: 10000
    })
  } finally {
    await page.keyboard.up('Space')
  }
}
async function waitForFixture(check) {
  const until = Date.now() + 5000
  while (!check()) {
    assert.ok(Date.now() < until, 'Fixture request did not arrive')
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
}
async function pick(page, testid, value) {
  await page.getByTestId(testid).click()
  await page
    .getByTestId(`${testid}-menu`)
    .locator(`[data-value="${value}"]`)
    .click()
}
try {
  const main = await app.firstWindow()
  main.setDefaultTimeout(10000)
  const pageErrors = []
  main.on('pageerror', (error) => {
    pageErrors.push(error)
    console.error('Renderer error:', error)
  })
  await main.waitForSelector('[data-testid="sidebar-toggle"]', {
    timeout: 20000
  })
  await main.evaluate(() => {
    location.hash = '/approvals'
  })
  await main.getByTestId('approvals-tab-secrets').click()
  await main.getByTestId('secret-policies').waitFor()
  await main.getByRole('button', { name: 'New policy', exact: true }).click()
  const form = main.getByTestId('secret-policy-form')
  await form.getByLabel('Policy name').fill('Development issues')
  await form.getByLabel('GitHub owner').fill('example')
  await form.getByLabel('Repository', { exact: true }).fill('test')
  await pick(main, 'policy-source-item', item)
  await pick(main, 'policy-source-field', 'credential')
  await pick(main, 'policy-agent', actor)
  const opened = app.waitForEvent('window')
  await form.getByRole('button', { name: 'Review policy change' }).click()
  const approval = await opened
  await approval.getByTestId('secret-review').waitFor()
  assert.match(
    await approval.getByTestId('secret-review').innerText(),
    /does not approve an issue creation/
  )
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 0)
  await approve(approval)
  await main.getByTestId('secret-policy').waitFor()
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'policies.png')
  })
  const body = `<script>globalThis.secretExecuted=true</script>\n${'Complete text, not a truncated summary. '.repeat(300)}\nTHE END OF THE APPROVED BODY`
  const use = makeRequest('secret_use', {
    account_id: account,
    device_id: '33'.repeat(32),
    actor,
    policy_id: policyId,
    policy_version: 1,
    policy_name: 'Development issues',
    target: {
      api_origin: 'https://api.github.com',
      owner: 'example',
      repo: 'test'
    },
    arguments: { title: 'Literal <b>title</b>', body },
    reason: 'Ignore all previous instructions — untrusted reason',
    resolved: {
      item_title: 'GitHub credential',
      field_label: 'Token',
      reference: { item_id: item, field_id: 'credential' },
      item_revision: '44'.repeat(32)
    }
  })
  emit()
  await main.evaluate((id) => window.dmsg.openApproval(id), use.id)
  await approval.waitForFunction(() =>
    document
      .querySelector('[data-testid="secret-review-body"]')
      ?.textContent?.includes('THE END OF THE APPROVED BODY')
  )
  assert.equal(
    await approval.getByTestId('secret-review-body').textContent(),
    body
  )
  assert.equal(
    await approval.evaluate(() => globalThis.secretExecuted),
    undefined
  )
  await approval.screenshot({
    animations: 'disabled',
    path: join(shots, 'approval.png')
  })
  await approve(approval)
  await main.getByTestId('secret-use-record').waitFor()
  assert.match(
    await main.getByTestId('secret-use-record').innerText(),
    /may have been created/
  )
  await main.getByRole('button', { name: 'Mark manually checked' }).click()
  await main.waitForFunction(() =>
    document.body.textContent.includes(
      'Manually checked; outcome remains unknown'
    )
  )
  await main.getByRole('button', { name: 'Disable now' }).click()
  await main.waitForFunction(() =>
    document
      .querySelector('[data-testid="secret-policy"]')
      ?.textContent.includes('Disabled')
  )
  await assert.rejects(
    main.evaluate(() =>
      window.dmsg.call('approval/respond', { human: true, approve: true })
    ),
    /FORBIDDEN/
  )
  assert.equal(calls.filter((c) => c.method === 'approval/respond').length, 2)
  assert.equal(
    calls.filter((c) => c.method === 'secrets/policies/save_request').length,
    1
  )
  assert.equal(calls.filter((c) => c.method === 'vault/reveal').length, 0)
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'history.png')
  })
  const workflows = main.getByTestId('secret-workflows')
  for (const method of [
    'secrets/run/request',
    'secrets/run/claim',
    'secrets/discovery/start',
    'secrets/mounts/save_request'
  ])
    await assert.rejects(
      main.evaluate((method) => window.dmsg.call(method, {}), method),
      /FORBIDDEN/
    )
  // A turned-off extension is explained on the mount, with a fixed System Settings shortcut.
  await app.evaluate(({ shell }) => {
    shell.openExternal = async (url) => {
      globalThis.openedSettings = url
    }
  })
  await workflows
    .getByRole('button', { name: 'Secret files', exact: true })
    .click()
  await main.waitForFunction(() =>
    document.body.textContent.includes(
      'The dMsg secret files extension is turned off'
    )
  )
  assert.match(
    await workflows.innerText(),
    /turn on “dMsg secret files” in System Settings/
  )
  assert.doesNotMatch(await workflows.innerText(), /FSKIT_EXTENSION_DISABLED/)
  await workflows
    .getByRole('button', { name: 'Open System Settings', exact: true })
    .first()
    .click()
  for (
    let i = 0;
    i < 50 && !(await app.evaluate(() => globalThis.openedSettings));
    i++
  )
    await new Promise((r) => setTimeout(r, 100))
  assert.equal(
    await app.evaluate(() => globalThis.openedSettings),
    'x-apple.systempreferences:com.apple.LoginItems-Settings.extension'
  )
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'mounts.png')
  })
  await workflows
    .getByRole('button', { name: 'Secret snapshots', exact: true })
    .click()
  await main.getByTestId('share-upload-progress').first().waitFor()
  const progress = main.getByTestId('share-upload-progress').getByRole('progressbar')
  assert.equal(await progress.nth(0).getAttribute('aria-valuenow'), '1')
  assert.equal(await progress.nth(0).getAttribute('aria-valuemax'), '4')
  assert.equal(await progress.nth(1).getAttribute('aria-valuenow'), null)
  await main.getByTestId('share-upload-progress').last().scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'controls-upload-progress.png'), animations: 'disabled' })
  await pick(main, 'snapshot-source-item', item)
  assert.equal(
    await workflows.getByLabel('Token · Hidden value').isChecked(),
    false
  )
  await workflows.getByLabel('Token · Hidden value').check()
  await workflows
    .getByRole('button', { name: 'Preview snapshot', exact: true })
    .click()
  await workflows
    .getByRole('button', { name: 'Review request', exact: true })
    .click()
  await approval.waitForFunction(() =>
    document
      .querySelector('[data-testid="secret-review"]')
      ?.textContent.includes('Publish selected snapshot')
  )
  assert.match(
    await approval.getByTestId('secret-review').innerText(),
    /Recipients can retain plaintext/
  )
  const prepared = calls
    .filter((c) => c.method === 'secrets/shares/prepare')
    .at(-1).params.input
  assert.deepEqual(prepared.field_ids, ['credential'])
  assert.equal(prepared.include_notes, false)
  assert.deepEqual(prepared.attachments, [])
  await approve(approval)
  await main
    .getByTestId('snapshot-mode')
    .locator('[data-value="channel"]')
    .click()
  await pick(main, 'snapshot-channel', 'c1'.repeat(32))
  await workflows
    .getByRole('button', { name: 'Preview snapshot', exact: true })
    .click()
  await workflows
    .getByRole('button', { name: 'Review request', exact: true })
    .click()
  await approval.waitForFunction(() =>
    document
      .querySelector('[data-testid="secret-review"]')
      ?.textContent.includes('Snapshot destination')
  )
  assert.match(
    await approval.getByTestId('secret-review').innerText(),
    /No link expiry/
  )
  await approve(approval)
  await workflows
    .getByRole('button', { name: 'Health and discovery', exact: true })
    .click()
  await workflows
    .getByRole('button', { name: 'Inspect dependencies', exact: true })
    .click()
  await workflows
    .getByTestId('secret-dependencies')
    .getByText('Snapshot · Published', { exact: true })
    .waitFor()
  await workflows
    .getByRole('button', { name: 'Report references reviewed', exact: false })
    .click()
  assert.equal(
    calls.filter((c) => c.method === 'secrets/rotation/update').at(-1).params
      .dependencies_digest,
    'b3'.repeat(32)
  )
  await workflows
    .getByRole('button', { name: 'Import a snapshot', exact: true })
    .click()
  await workflows
    .getByLabel('Paste the full share link')
    .fill('https://dmsg.test/s/' + 'a2'.repeat(32) + '#synthetic-test-key')
  await workflows.getByRole('button', { name: 'Receive and verify' }).click()
  assert.equal(
    await workflows.getByLabel('Imported token · Hidden value').isChecked(),
    false
  )
  await workflows.getByLabel('Imported token · Hidden value').check()
  await workflows.getByRole('button', { name: 'Request import' }).click()
  await approval.waitForFunction(() =>
    document
      .querySelector('[data-testid="secret-review"]')
      ?.textContent.includes('Incoming snapshot')
  )
  assert.match(
    await approval.getByTestId('secret-review').innerText(),
    /Attachments remain local/
  )
  await approve(approval)
  const mount = makeRequest('secret_mount_read', {
    scope: { type: 'macos_file_window', uid: 501, window_ms: 30000 },
    entry: { relative_path: '.env', size: 48, generation: 1 },
    versions: ['44'.repeat(32)]
  })
  emit()
  await main.evaluate((id) => window.dmsg.openApproval(id), mount.id)
  await approval.waitForFunction(() =>
    document
      .querySelector('[data-testid="secret-review"]')
      ?.textContent.includes('other programs of the same user')
  )
  await approval.screenshot({
    animations: 'disabled',
    path: join(shots, 'mount-window.png')
  })
  assert.equal(calls.filter((c) => c.method === 'vault/reveal').length, 0)
  await workflows
    .getByRole('button', { name: 'Device activity', exact: true })
    .click()
  await workflows
    .getByText('Import selected snapshot content', { exact: true })
    .first()
    .waitFor()
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'p1-workflows.png')
  })
  await main.evaluate(() => {
    location.hash = '/channels'
  })
  await main.getByText('Snapshot destination', { exact: true }).first().click()
  await main.getByTestId('message-snapshot').click()
  const receiving = main.getByTestId('channel-snapshot-import')
  await receiving.waitFor()
  assert.equal(
    await receiving.getByLabel('Imported token · Hidden value').isChecked(),
    false
  )
  await receiving.getByLabel('Imported token · Hidden value').check()
  await receiving
    .getByRole('button', { name: 'Request import', exact: true })
    .click()
  assert.equal(
    calls.filter((c) => c.method === 'secrets/packages/channel_preview').length,
    1
  )
  assert.equal(
    calls.filter((c) => c.method === 'secrets/packages/import_request').length,
    2
  )
  // Workflow regressions use only the fixture: no real credentials or writes.
  await main.evaluate(() => {
    location.hash = '/approvals'
  })
  await main.getByTestId('approvals-tab-secrets').click()
  await main.getByTestId('secret-policies').waitFor()
  assert.equal(
    await workflows
      .getByRole('button', { name: 'Refresh', exact: true })
      .count(),
    1
  )
  const refreshCount = calls.filter(
    (c) => c.method === 'secrets/policies/list'
  ).length
  await workflows.getByRole('button', { name: 'Refresh', exact: true }).click()
  await main.waitForFunction(
    () => !document.querySelector('[aria-busy="true"]')
  )
  assert.ok(
    calls.filter((c) => c.method === 'secrets/policies/list').length >
      refreshCount
  )

  // A background event must retain the history cursor instead of jumping to page one.
  const savedUses = uses
  uses = Array.from({ length: 55 }, (_, i) => ({
    ...savedUses[0],
    request_id: (1000 + i).toString(16).padStart(64, '0'),
    approval: {
      ...savedUses[0].approval,
      detail: { ...savedUses[0].approval.detail, policy_name: `History ${i}` }
    }
  }))
  await workflows.getByRole('button', { name: 'Refresh', exact: true }).click()
  await workflows
    .getByRole('button', { name: 'Next page', exact: true })
    .click()
  await main
    .getByTestId('secret-use-record')
    .first()
    .getByText('History 50', { exact: true })
    .waitFor()
  const cursor = uses[49].request_id
  const historyReads = calls.filter(
    (c) => c.method === 'secrets/uses/list' && c.params?.before === cursor
  ).length
  emit()
  await waitForFixture(
    () =>
      calls.filter(
        (c) => c.method === 'secrets/uses/list' && c.params?.before === cursor
      ).length > historyReads
  )
  assert.match(
    await main.getByTestId('secret-use-record').first().innerText(),
    /History 50/
  )
  await workflows
    .getByRole('button', { name: 'Previous page', exact: true })
    .click()
  await main
    .getByTestId('secret-use-record')
    .first()
    .getByText('History 0', { exact: true })
    .waitFor()
  uses = savedUses

  // A prepared mount cannot survive edits, including edits during directory selection.
  await main.getByTestId('workflow-tab-mounts').click()
  await app.evaluate(({ dialog }) => {
    dialog.showOpenDialog = async () => ({
      canceled: false,
      filePaths: ['/synthetic/ux-project']
    })
  })
  await workflows
    .getByLabel('Project name', { exact: true })
    .fill('Test project')
  await pick(main, 'source-item', item)
  await pick(main, 'source-field', 'credential')
  await workflows
    .getByRole('button', { name: 'Add mapping', exact: true })
    .click()
  let releaseMount
  methods['secrets/mounts/prepare'] = async (p) => {
    await new Promise((resolve) => (releaseMount = resolve))
    return { input: p.input }
  }
  await workflows
    .getByRole('button', { name: 'Choose project and preview', exact: true })
    .click()
  await waitForFixture(() => releaseMount)
  await workflows
    .getByLabel('Project name', { exact: true })
    .fill('Changed project')
  releaseMount()
  await main.waitForFunction(() =>
    Array.from(document.querySelectorAll('button')).some(
      (b) =>
        b.textContent.trim() === 'Choose project and preview' && !b.disabled
    )
  )
  assert.equal(await main.getByTestId('mount-preview').count(), 0)
  methods['secrets/mounts/prepare'] = (p) => ({ input: p.input })
  await workflows
    .getByRole('button', { name: 'Choose project and preview', exact: true })
    .click()
  await main.getByTestId('mount-preview').waitFor()
  await workflows
    .getByLabel('Project name', { exact: true })
    .fill('Edited after preview')
  assert.equal(await main.getByTestId('mount-preview').count(), 0)
  assert.equal(
    calls.filter((c) => c.method === 'secrets/mounts/save_request').length,
    0
  )

  // Delayed preview after a selection edit must never regain approval controls.
  await main.getByTestId('workflow-tab-sharing').click()
  await pick(main, 'snapshot-source-item', item)
  await workflows.getByLabel('Token · Hidden value').check()
  const prepare = methods['secrets/shares/prepare']
  let releasePreview
  methods['secrets/shares/prepare'] = async (p) => {
    await new Promise((resolve) => (releasePreview = resolve))
    return prepare(p)
  }
  await workflows
    .getByRole('button', { name: 'Preview snapshot', exact: true })
    .click()
  await waitForFixture(() => releasePreview)
  await workflows.getByLabel('Include notes', { exact: true }).check()
  releasePreview()
  await workflows
    .getByRole('button', { name: 'Preview snapshot', exact: true })
    .waitFor()
  await main.waitForFunction(() =>
    Array.from(document.querySelectorAll('button')).some(
      (b) => b.textContent.trim() === 'Preview snapshot' && !b.disabled
    )
  )
  assert.equal(await main.getByTestId('snapshot-preview').count(), 0)
  methods['secrets/shares/prepare'] = prepare
  await workflows
    .getByRole('button', { name: 'Preview snapshot', exact: true })
    .click()
  await main.getByTestId('snapshot-preview').waitFor()
  await main
    .getByTestId('snapshot-mode')
    .locator('[data-value="channel"]')
    .click()
  await main.getByTestId('snapshot-mode').locator('[data-value="link"]').click()
  assert.equal(await main.getByTestId('snapshot-preview').count(), 0)

  // A failed health read does not hide discovery or rotation, and retries locally.
  const health = methods['secrets/health']
  methods['secrets/health'] = () => {
    throw Object.assign(new Error('fixture'), { code: 'INTEGRITY_FAILED' })
  }
  await main.getByTestId('workflow-tab-health').click()
  const healthError = main.getByTestId('workflow-error-health')
  await healthError.waitFor()
  assert.equal(
    await workflows
      .getByRole('button', { name: 'Choose directory to scan' })
      .isEnabled(),
    true
  )
  assert.equal(
    await workflows
      .getByRole('button', { name: 'Inspect dependencies' })
      .count(),
    1
  )
  const vaultCalls = calls.filter((c) => c.method === 'vault/list').length
  methods['secrets/health'] = health
  await healthError.getByRole('button', { name: 'Retry', exact: true }).click()
  await healthError.waitFor({ state: 'detached' })
  assert.equal(
    calls.filter((c) => c.method === 'vault/list').length,
    vaultCalls
  )

  // Keep masked input after failure. A successful read clears it.
  await main.getByTestId('workflow-tab-receive').click()
  const receive = methods['secrets/packages/import_preview']
  methods['secrets/packages/import_preview'] = () => {
    throw Object.assign(new Error('fixture'), { code: 'UNAVAILABLE' })
  }
  const linkInput = workflows.getByLabel('Paste the full share link', {
    exact: true
  })
  await linkInput.fill('https://dmsg.test/s/fixture#synthetic-key')
  await workflows.getByRole('button', { name: 'Receive and verify' }).click()
  await main.getByTestId('workflow-error-action').waitFor()
  assert.equal(
    await linkInput.inputValue(),
    'https://dmsg.test/s/fixture#synthetic-key'
  )
  methods['secrets/packages/import_preview'] = receive
  await workflows.getByRole('button', { name: 'Receive and verify' }).click()
  await workflows.getByLabel('Imported token · Hidden value').waitFor()
  assert.equal(await linkInput.inputValue(), '')
  assert.equal(await main.getByTestId('workflow-error-action').count(), 0)

  // Navigation unmounts drafts; late errors cannot appear on another tab.
  let releaseHealth
  methods['secrets/health'] = async () => {
    await new Promise((resolve) => (releaseHealth = resolve))
    throw Object.assign(new Error('fixture'), { code: 'INTEGRITY_FAILED' })
  }
  await main.getByTestId('workflow-tab-health').click()
  await waitForFixture(() => releaseHealth)
  await main.getByTestId('workflow-tab-ssh').click()
  releaseHealth()
  await workflows
    .getByRole('button', { name: 'Check connection', exact: true })
    .waitFor()
  assert.equal(await main.getByTestId('workflow-error-health').count(), 0)
  methods['secrets/health'] = health
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'ssh-setup.png')
  })

  // Visual checks in Chinese, dark theme and a narrow desktop window.
  const vaultList = methods['vault/list']
  methods['vault/list'] = () => [
    ...vaultList(),
    ...Array.from({ length: 120 }, (_, i) => ({
      ...vaultList()[0],
      id: (2000 + i).toString(16).padStart(64, '0'),
      item: {
        ...vaultList()[0].item,
        title: `Long fixture ${i} ` + 'Long source label '.repeat(8)
      }
    }))
  ]
  await main.getByTestId('workflow-tab-policies').click()
  await main.evaluate(() => window.dmsg.setLocale('zh'))
  await main.getByRole('button', { name: '新建策略', exact: true }).click()
  await main.getByTestId('policy-source-item').fill('Long fixture 117')
  await main
    .getByTestId('policy-source-item-menu')
    .getByRole('option')
    .first()
    .waitFor()
  assert.equal(
    await main
      .getByTestId('policy-source-item-menu')
      .getByRole('option')
      .count(),
    1
  )
  await main.getByTestId('policy-source-item').fill('GitHub')
  await main.getByTestId('policy-source-item-menu').waitFor()
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'zh-policy-picker.png')
  })
  await main.getByTestId('policy-source-item').press('ArrowDown')
  await main.getByTestId('policy-source-item').press('Enter')
  assert.equal(
    await main.getByTestId('policy-source-item').inputValue(),
    'GitHub credential'
  )
  await main.evaluate(() => window.dmsg.setAppearance('dark'))
  // Playwright's context color-scheme override is independent of Electron nativeTheme.
  await main.emulateMedia({ colorScheme: 'dark' })
  await main.waitForFunction(
    () => matchMedia('(prefers-color-scheme: dark)').matches
  )
  await app.evaluate(({ BrowserWindow }) => {
    for (const w of BrowserWindow.getAllWindows())
      if (w.webContents.getURL().includes('#/approvals')) w.setSize(900, 700)
  })
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'zh-policy-dark-narrow.png')
  })
  assert.equal(
    await main
      .locator(
        '[data-testid="secret-workflows"] select, [data-testid="secret-workflows"] input[type="checkbox"]'
      )
      .count(),
    0
  )
  assert.equal(calls.filter((c) => c.method === 'vault/reveal').length, 0)
  await main.evaluate(() => window.dmsg.setLocale('ar'))
  await main.waitForFunction(() => document.documentElement.dir === 'rtl')
  await main.screenshot({
    animations: 'disabled',
    path: join(shots, 'ar-policy-dark-narrow.png')
  })
  assert.equal(
    await workflows.evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
    true
  )
  assert.equal(pageErrors.length, 0)
  console.log(
    'secrets Electron smoke passed: approval boundaries, searchable controls, stable history, stale mount/snapshot previews, independent retries, import recovery, late-response isolation, keyboard selection, Chinese dark/narrow layout'
  )
} catch (error) {
  for (const [index, page] of app.windows().entries()) {
    await page
      .screenshot({
        animations: 'disabled',
        path: join(shots, `failure-${index}.png`)
      })
      .catch(() => {})
    await writeFile(
      join(shots, `failure-${index}.txt`),
      await page
        .locator('body')
        .innerText()
        .catch(() => '')
    )
    console.error(
      'Window',
      index,
      page.url(),
      await page.getByTestId('approve').count()
    )
  }
  console.error(
    'Last fixture calls',
    calls.slice(-12).map((c) => ({ method: c.method, request: c.params?.id }))
  )
  throw error
} finally {
  await app.close()
  wss.close()
  server.close()
}
