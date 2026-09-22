import { beforeEach, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import { CryptoEngine } from '../src/lib/crypto/engine'
import {
  advanceChannel,
  readable,
  recipientDigest,
  startChannel,
  type EpochRecipient
} from '../src/lib/protocol/channel'
import { hash, id, unb64, hex } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
const owner = xidText(new Uint8Array(12).fill(1)),
  member = xidText(new Uint8Array(12).fill(2))
const channel = '01'.repeat(32),
  genesis = '02'.repeat(32)
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
it('requires accepted membership, preserves default history isolation, and rejects owner takeover', () => {
  let state = startChannel(
    { channel_id: channel, nonce: '03'.repeat(32), type: 'collaboration' },
    owner,
    genesis
  )
  state.epoch = 1
  state.status = 'active'
  const step = (action: any, actor = owner) =>
    advanceChannel(
      state,
      { expected_head: state.head, expected_acl_version: state.acl_version, action },
      actor,
      id(),
      1000
    )
  state = step({
    type: 'invite',
    invitation_id: '04'.repeat(32),
    account: member,
    role: 'member',
    expires_at: 3000
  })
  expect(state.members[member]).toBeUndefined()
  state = step({ type: 'accept', invitation_id: '04'.repeat(32) }, member)
  expect(state.status).toBe('rotation_required')
  state.epoch = 2
  state.status = 'active'
  expect(readable(state, member, 1)).toBe(false)
  expect(readable(state, member, 2)).toBe(true)
  expect(() =>
    step({ type: 'role', account: owner, role: 'admin', can_rotate: true }, member)
  ).toThrow()
  expect(() =>
    step({ type: 'transfer', account: member, device: '05'.repeat(32), signature: '' }, member)
  ).toThrow()
  state = step({
    type: 'history',
    account: member,
    from_epoch: 1,
    to_epoch: 1,
    envelope_upload: '06'.repeat(32)
  })
  expect(readable(state, member, 1)).toBe(true)
  expect(() => step({ type: 'accept', invitation_id: '04'.repeat(32) }, member)).toThrow()
  state = step({ type: 'remove', account: member })
  expect(state.status).toBe('rotation_required')
  expect(readable(state, member, 1)).toBe(false)
})
it('wraps 600 recipients in bounded pages and reuses exact ciphertext across restart', async () => {
  const engine = new CryptoEngine(),
    password = 'channel-six-hundred-test-only'
  const setup = await engine.initialize(password)
  await engine.verifyRecovery(setup.recoveryCode)
  setup.meta.account = {
    id: owner,
    issuer: `https://dmsg.test/u/${owner}`,
    homeUser: 'aaaaa-aa'
  }
  const db = await WorkspaceDB.open((await currentWorkspace())!)
  await db.db.put('meta', {
    id: 'workspace',
    value: { ...(await db.meta())!, account: setup.meta.account }
  })
  db.db.close()
  await engine.lock()
  await engine.unlock(password)
  await engine.channelRemember({ channel, name: '600 recipients', genesis })
  const ledger = startChannel(
    { channel_id: channel, nonce: '03'.repeat(32), type: 'collaboration' },
    owner,
    genesis
  )
  await engine.channelAdvance(channel, ledger, 1, [])
  const recipients: EpochRecipient[] = []
  for (let account = 1; account <= 100; account++) {
    const accountId =
      account === 1
        ? owner
        : xidText(Uint8Array.from({ length: 12 }, (_, i) => (i === 11 ? account : 1)))
    for (let device = 0; device < 5; device++)
      recipients.push({
        recipient: `device:${accountId}:${account === 1 && device === 0 ? setup.meta.deviceId : hash(new TextEncoder().encode(`${account}:${device}`))}`,
        hpke_pub: hex(unb64(setup.meta.hpkePublic))
      })
    recipients.push({
      recipient: `recovery:${accountId}:1`,
      hpke_pub: hex(unb64(setup.meta.recoveryPublic))
    })
  }
  recipients.sort((a, b) => a.recipient.localeCompare(b.recipient))
  const lease = {
    id: '07'.repeat(32),
    account: owner,
    device: setup.meta.deviceId,
    epoch: 1,
    head: genesis,
    recipients_digest: recipientDigest(recipients),
    security_versions: { [owner]: 1 },
    fencing: 1,
    expires_at: Date.now() + 60000
  }
  const first = await engine.channelRotation(channel, lease, recipients)
  expect(first.uploads).toHaveLength(5)
  expect(first.activation.envelopes).toHaveLength(600)
  expect(
    first.uploads.every((u) => u.chunks.length <= 120 && u.plan.manifest_size < 65536)
  ).toBe(true)
  expect(first.uploads.every((u) => u.plan.chunks.every((c) => c.size <= 2048))).toBe(true)
  await engine.lock()
  await engine.unlock(password)
  // An exact retry sees the same persisted candidate and every HPKE ciphertext.
  expect(await engine.channelRotation(channel, lease, recipients)).toEqual(first)
  await expect(engine.channelRotation(channel, lease, recipients.slice(1))).rejects.toThrow()
  await engine.lock()
}, 30000)
