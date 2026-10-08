import { readFileSync } from 'node:fs'
import { beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { MasterPublicKey } from '@icp-sdk/vetkeys'
import { Principal } from '@icp-sdk/core/principal'
import { CryptoEngine } from '../src/lib/crypto/engine'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import {
  accountApprovalMessage,
  accountOperationDigest,
  accountValue,
  createAccountMessage,
  decodeControl,
  deviceInput,
  encodeControl,
  recoveryDeviceMessage,
  rootBundleDigest,
  rootRecipientsDigest
} from '../src/lib/protocol/account'
import { b64, canonical, equal, hash, hex, unhex, random } from '../src/lib/protocol/codec'
import { xidBytes, xidText } from '../src/lib/protocol/identity'
import { ed25519, hpkePublic } from '../src/lib/crypto/primitives'
import {
  bundleDigests,
  openRoot,
  parseRootBundle,
  readRootBundle,
  recoverRoot,
  rootMaterial,
  rootTransport,
  wrapRoot,
  type RecoveryKey
} from '../src/lib/crypto/root'
import type { AccountMutation } from '../src/lib/canisters/generated/user'
import { AccountClient } from '../src/lib/services/account'
import { boundEngine } from './support/engine'

const home = Principal.fromUint8Array(new Uint8Array([1])),
  caller = Principal.fromUint8Array(new Uint8Array([2]))
const account = xidText(new Uint8Array(12).fill(1)),
  seed = new Uint8Array(32).fill(7)
const publicMeta = {
  deviceId: '07'.repeat(32),
  signingPublic: b64(ed25519.getPublicKey(seed)),
  hpkePublic: b64(new Uint8Array(32).fill(8))
}
const vectors = JSON.parse(
  readFileSync(
    new URL('../../dmsg_types/tests/protocol_vectors.json', import.meta.url),
    'utf8'
  )
) as { name: string; sha256_hex: string }[]
const vector = (name: string) => vectors.find((v) => v.name === name)!.sha256_hex
/** A real vetKD public key: the mainnet master key derived for a test canister. */
const recoveryKey = (): RecoveryKey => ({
  homeCose: home.toText(),
  keyName: 'key_1',
  publicKey: b64(
    MasterPublicKey.productionKey()
      .deriveCanisterKey(home.toUint8Array())
      .deriveSubKey(canonical(['dmsg/content-root/v2', 'Local', 2]))
      .publicKeyBytes()
  )
})
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
describe('account authorization and encrypted local state', () => {
  it('persists the exact recovery request identity for completion retries', () => {
    const accountId = new Uint8Array(12).fill(1),
      requestId = new Uint8Array(32).fill(9)
    const encoded = encodeControl('complete_recovery', [accountId, requestId])
    const decoded = decodeControl('complete_recovery', encoded)
    expect(decoded).toEqual([accountId, requestId])
    expect(encodeControl('complete_recovery', decoded)).toBe(encoded)
    expect(() => encodeControl('complete_recovery', [accountId])).toThrow()
  })

  it('binds account creation to the actual caller, user home, device and deadline', () => {
    const request = {
      device: deviceInput(publicMeta),
      op_id: new Uint8Array(32).fill(9),
      expires_at: 1700000000000n
    }
    const message = createAccountMessage(home, caller, request),
      signature = ed25519.sign(message, seed)
    for (const other of [
      createAccountMessage(caller, caller, request),
      createAccountMessage(home, home, request),
      createAccountMessage(home, caller, { ...request, expires_at: request.expires_at + 1n }),
      createAccountMessage(home, caller, {
        ...request,
        device: deviceInput(publicMeta, 'Member')
      })
    ]) {
      expect(ed25519.verify(signature, other, ed25519.getPublicKey(seed))).toBe(false)
    }
  })
  it('persists exact Candid mutation bytes and binds all approval and root fields', () => {
    const request: AccountMutation = {
      account_id: new Uint8Array(12).fill(1),
      expected_version: 3n,
      command: {
        CommitRoot: {
          expected_generation: 1n,
          op_id: new Uint8Array(32).fill(9),
          root: {
            generation: 3n,
            suite: 'dmsg-root-v2',
            bundle_digest: new Uint8Array(32).fill(10),
            recipients_digest: new Uint8Array(32).fill(11),
            body_digest: new Uint8Array(32).fill(12)
          }
        }
      },
      approval: {
        device_id: new Uint8Array(32).fill(7),
        security_epoch: 2n,
        sequence: 4n,
        request_id: new Uint8Array(32).fill(11),
        expires_at: 1700000000000n,
        signature: new Uint8Array()
      }
    }
    request.approval.signature = ed25519.sign(accountApprovalMessage(home, request), seed)
    const encoded = encodeControl('mutate_account', [request]),
      decoded = decodeControl('mutate_account', encoded)[0] as AccountMutation
    expect(equal(accountOperationDigest(decoded), accountOperationDigest(request))).toBe(true)
    expect(encodeControl('mutate_account', [decoded])).toBe(encoded)
    for (const changed of [
      { ...decoded, expected_version: 4n },
      { ...decoded, approval: { ...decoded.approval, security_epoch: 3n } },
      { ...decoded, approval: { ...decoded.approval, sequence: 5n } },
      { ...decoded, account_id: new Uint8Array(12).fill(2) }
    ]) {
      expect(
        ed25519.verify(
          Uint8Array.from(request.approval.signature),
          accountApprovalMessage(home, changed),
          ed25519.getPublicKey(seed)
        )
      ).toBe(false)
    }
    expect(
      equal(
        canonical(accountValue({ signing_pub: [1, 2, 3] })),
        canonical({ signing_pub: new Uint8Array([1, 2, 3]) })
      )
    ).toBe(true)
  })
  it('computes the root recipients and bundle digests the user home binds (Rust vectors)', () => {
    const recipients = rootRecipientsDigest(
      [new Uint8Array(32).fill(3), new Uint8Array(32).fill(2)],
      2
    )
    expect(hex(recipients)).toBe(vector('root_recipients_v1'))
    expect(
      hex(
        rootBundleDigest(recipients, unhex(hash(new TextEncoder().encode('root bundle body'))))
      )
    ).toBe(vector('root_bundle_digest_v2'))
    expect(
      hex(rootRecipientsDigest([new Uint8Array(32).fill(2), new Uint8Array(32).fill(3)], 2))
    ).toBe(vector('root_recipients_v1'))
  })
  it('binds a recovery request to the home, account and replacement device', () => {
    const request = {
      op_id: new Uint8Array(32).fill(4),
      device: deviceInput(publicMeta),
      new_auth: caller,
      expires_at: 1700000000000n
    }
    const message = recoveryDeviceMessage(home, account, request)
    expect(message).toEqual(recoveryDeviceMessage(home, unhex('01'.repeat(12)), request))
    expect(message).not.toEqual(recoveryDeviceMessage(caller, account, request))
    expect(message).not.toEqual(
      recoveryDeviceMessage(home, account, { ...request, new_auth: home })
    )
  })
  it('keeps control journals encrypted and the candidate transport stable across restarts', async () => {
    const { engine } = await boundEngine()
    await engine.controlPut('operation', JSON.stringify({ request: 'private-control-marker' }))
    const context = {
      account,
      environment: 'local',
      generation: 3,
      opId: '01'.repeat(32),
      securityEpoch: 1
    }
    const first = await engine.prepareAccountRoot(context)
    const db = await WorkspaceDB.open((await currentWorkspace())!)
    const raw = JSON.stringify(await db.db.getAll('local_private'))
    db.db.close()
    expect(raw).not.toContain('private-control-marker')
    expect(raw).not.toContain(b64(first))
    await engine.lock()
    await expect(engine.deviceSign(new Uint8Array(32))).rejects.toThrow('解锁')
    const reopened = new CryptoEngine()
    await reopened.unlock()
    expect(await reopened.controlGet('operation')).toContain('private-control-marker')
    expect(await reopened.prepareAccountRoot(context)).toEqual(first)
    await expect(reopened.prepareAccountRoot({ ...context, generation: 4 })).rejects.toThrow(
      'IDEMPOTENCY_CONFLICT'
    )
    expect(rootTransport(rootMaterial(context))).toHaveLength(48)
    await reopened.lock()
  })
  it('wraps a root to every device and the vetKD identity, and only those devices open it', async () => {
    const context = {
      account,
      environment: 'local',
      generation: 2,
      opId: '02'.repeat(32),
      securityEpoch: 1
    }
    const material = rootMaterial(context)
    const devices = [new Uint8Array(32).fill(21), new Uint8Array(32).fill(22)].map(
      (hpke, i) => ({
        hpkeSeed: hpke,
        deviceId: hex(new Uint8Array(32).fill(31 + i))
      })
    )
    const recipients = []
    for (const device of devices)
      recipients.push({
        deviceId: device.deviceId,
        hpkePublic: await hpkePublic(device.hpkeSeed)
      })
    const previousRoot = random()
    const bytes = await wrapRoot(
      material,
      recipients,
      recoveryKey(),
      { deviceId: devices[1].deviceId, seed },
      {
        digest: hash(new Uint8Array([1])),
        uploadId: '03'.repeat(32),
        generation: 1,
        root: previousRoot
      }
    )
    const bundle = parseRootBundle(bytes)
    expect(bundle.body.envelopes.map((e) => e.device)).toEqual(
      recipients.map((r) => r.deviceId).sort()
    )
    const digests = bundleDigests(bundle)
    expect(hex(digests.recipientsDigest)).toBe(
      hex(
        rootRecipientsDigest(
          devices.map((d) => unhex(d.deviceId)),
          2
        )
      )
    )
    expect(hex(digests.bundleDigest)).toBe(
      hex(rootBundleDigest(digests.recipientsDigest, digests.bodyDigest))
    )
    const expected = hex(digests.bundleDigest)
    // The previous bundle is not supplied here, so the chain is incomplete.
    await expect(openRoot(material, devices[0], [bytes], expected)).rejects.toThrow(
      'RECOVERY_INCOMPLETE'
    )
    await expect(
      openRoot(
        material,
        { deviceId: hex(new Uint8Array(32).fill(99)), hpkeSeed: random() },
        [bytes],
        expected
      )
    ).rejects.toMatchObject({ code: 'DeviceNotApproved' })
    await expect(
      openRoot(material, { ...devices[0], hpkeSeed: random() }, [bytes], expected)
    ).rejects.toThrow()
    expect(() => readRootBundle(bytes, '00'.repeat(64))).toThrow('INTEGRITY_FAILED')
    const tampered = Uint8Array.from(bytes)
    tampered[tampered.length - 1] ^= 1
    expect(() => parseRootBundle(tampered)).toThrow()
    // A descriptor that differs from the bundle's recovery key never reaches decryption.
    await expect(
      recoverRoot(
        material,
        { ...recoveryKey(), keyName: 'test_key_1' },
        new Uint8Array(192),
        [bytes],
        expected
      )
    ).rejects.toThrow('恢复公钥')
    expect(
      await wrapRoot(
        { ...material, bytes: b64(bytes) },
        [],
        recoveryKey(),
        { deviceId: '', seed },
        null
      )
    ).toEqual(bytes)
  })

  it('asks for an admission ticket only when the home requires one', async () => {
    const { engine, meta } = await boundEngine(account, home.toText())
    const crypto = {
      call: (method: string, ...args: unknown[]) => (engine as any)[method](...args)
    }
    for (const key of [[], [new Uint8Array(32).fill(3)]] as const) {
      const calls: unknown[][] = []
      const admitted: Uint8Array[][] = []
      const user = {
        my_account: async () => [],
        user_config: async () => ({ admission_key: key }),
        create_account: async (...args: unknown[]) => {
          calls.push(args)
          return { Err: { Forbidden: null } }
        }
      }
      const client = new AccountClient(
        user as any,
        {} as any,
        caller,
        crypto as any,
        meta,
        home.toText()
      )
      await expect(
        client.create(async (h, p) => {
          admitted.push([h, p])
          return { expires_at: 9n, signature: new Uint8Array(64).fill(4) }
        })
      ).rejects.toThrow('Forbidden')
      const input = calls[0]![0] as { admission: unknown[] }
      if (key.length) {
        expect(admitted).toEqual([[home.toUint8Array(), caller.toUint8Array()]])
        expect(input.admission).toEqual([
          { expires_at: 9n, signature: new Uint8Array(64).fill(4) }
        ])
      } else {
        expect(admitted).toEqual([])
        expect(input.admission).toEqual([])
      }
      // The rejected creation leaves no pending journal for the next attempt.
      expect(await client.pending()).toBeNull()
    }
    const user = {
      my_account: async () => [],
      user_config: async () => ({ admission_key: [new Uint8Array(32)] })
    }
    const client = new AccountClient(
      user as any,
      {} as any,
      caller,
      crypto as any,
      meta,
      home.toText()
    )
    await expect(client.create()).rejects.toMatchObject({ code: 'UNAVAILABLE' })
  })
  it('accepts a binding only with the nonce of its own request', async () => {
    const { engine, meta } = await boundEngine(account, home.toText())
    const crypto = {
      call: (method: string, ...args: unknown[]) => (engine as any)[method](...args)
    }
    const accepted: unknown[][] = []
    const user = {
      accept_auth_binding: async (...args: unknown[]) => {
        accepted.push(args)
        return { Err: { NotFound: null } }
      }
    }
    const client = new AccountClient(
      user as any,
      {} as any,
      caller,
      crypto as any,
      meta,
      home.toText()
    )
    await expect(client.acceptBinding(account)).rejects.toMatchObject({ code: 'NOT_FOUND' })
    const packet = JSON.parse(await client.bindingRequest(account))
    expect(packet).toMatchObject({
      format: 'dmsg-auth-request/1',
      account,
      principal: caller.toText()
    })
    // Another login cannot accept with this request's nonce.
    const other = new AccountClient(
      user as any,
      {} as any,
      home,
      crypto as any,
      meta,
      home.toText()
    )
    await expect(other.acceptBinding(account)).rejects.toMatchObject({ code: 'NOT_FOUND' })
    await expect(client.acceptBinding(account)).rejects.toThrow('NotFound')
    expect(accepted).toEqual([[xidBytes(account), unhex(packet.nonce)]])
  })
})
