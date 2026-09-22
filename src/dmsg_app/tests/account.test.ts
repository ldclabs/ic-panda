import { beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
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
  encodeControl
} from '../src/lib/protocol/account'
import { b64, canonical, equal, hash, unb64 } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
import { ed25519 } from '../src/lib/crypto/primitives'
import { onlineKey, rootMaterial, rootTransport } from '../src/lib/crypto/root'
import type { AccountMutation } from '../src/lib/canisters/generated/user'

const home = Principal.fromUint8Array(new Uint8Array([1])),
  caller = Principal.fromUint8Array(new Uint8Array([2]))
const account = xidText(new Uint8Array(12).fill(1)),
  seed = new Uint8Array(32).fill(7)
const publicMeta = {
  deviceId: '07'.repeat(32),
  signingPublic: b64(ed25519.getPublicKey(seed)),
  hpkePublic: b64(new Uint8Array(32).fill(8))
}
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
describe('account authorization and encrypted local recovery state', () => {
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
            suite: 'dmsg-root-v1',
            home_cose: caller,
            derivation_version: 2,
            key_generation: 3n,
            bundle_digest: new Uint8Array(32).fill(10),
            recovery_generation: 1n
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
  it('keeps resumable journals and unfinished recovery codes encrypted and out of backups', async () => {
    const engine = new CryptoEngine(),
      password = 'account-test-password-only'
    const setup = await engine.initialize(password)
    await engine.verifyRecovery(setup.recoveryCode)
    await engine.controlPut('operation', JSON.stringify({ request: 'private-control-marker' }))
    const generated = await engine.accountRecovery({
      account,
      generation: 1,
      action: 'generate'
    })
    expect(
      (await engine.accountRecovery({ account, generation: 1, action: 'generate' })).code
    ).toBe(generated.code)
    const message = new Uint8Array(32).fill(17)
    const proof = await engine.accountRecovery({
      account,
      generation: 1,
      action: 'prove',
      code: generated.code,
      message,
      publicKey: generated.signingPublic
    })
    expect(ed25519.verify(proof.signature, message, unb64(generated.signingPublic))).toBe(true)
    await expect(
      engine.accountRecovery({
        account: xidText(new Uint8Array(12).fill(2)),
        generation: 1,
        action: 'prove',
        code: generated.code,
        message,
        publicKey: generated.signingPublic
      })
    ).rejects.toThrow('恢复码不匹配')
    const db = await WorkspaceDB.open((await currentWorkspace())!)
    const raw = JSON.stringify(await db.db.getAll('local_private'))
    db.db.close()
    expect(raw).not.toContain('private-control-marker')
    expect(raw).not.toContain(generated.code.replaceAll('-', ''))
    const backup = await engine.exportBackup(password),
      text = await backup.blob.text()
    expect(text).not.toContain('private-control-marker')
    expect(text).not.toContain(generated.code.replaceAll('-', ''))
    await engine.lock()
    await expect(engine.deviceSign(message)).rejects.toThrow('解锁')
    await engine.unlock(password)
    expect(await engine.controlGet('operation')).toContain('private-control-marker')
    await engine.accountRecovery({ account, generation: 1, action: 'clear' })
    expect(await engine.controlGet(`recovery:${account}:1`)).toBe('null')
    await engine.lock()
  })
  it('preserves transport keys across restart and rejects a descriptor substituted before decryption', async () => {
    const engine = new CryptoEngine(),
      password = 'account-test-password-only'
    const setup = await engine.initialize(password)
    await engine.verifyRecovery(setup.recoveryCode)
    const context = {
      account,
      environment: 'local',
      homeCose: home.toText(),
      generation: 3,
      opId: '01'.repeat(32),
      recoveryGeneration: 1,
      recoveryPublic: setup.meta.recoveryPublic,
      recoverySigningPublic: setup.meta.recoverySigningPublic
    }
    const first = await engine.prepareAccountRoot(context)
    await engine.lock()
    await engine.unlock(password)
    expect(await engine.prepareAccountRoot(context)).toEqual(first)
    await expect(engine.prepareAccountRoot({ ...context, generation: 4 })).rejects.toThrow(
      'IDEMPOTENCY_CONFLICT'
    )
    const material = rootMaterial(context)
    expect(rootTransport(material)).toHaveLength(48)
    expect(() =>
      onlineKey(
        material,
        {
          publicKey: b64(new Uint8Array(96)),
          fingerprint: '00'.repeat(32),
          keyId: '00'.repeat(32),
          keyName: 'key_1'
        },
        new Uint8Array()
      )
    ).toThrow('INTEGRITY_FAILED')
    expect(hash(first)).not.toBe(hash(new Uint8Array(48)))
    await engine.lock()
  })
})
