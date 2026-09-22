import 'fake-indexeddb/auto'
import { describe, expect, it } from 'vitest'
import { openDB, deleteDB } from 'idb'
import { Principal } from '@icp-sdk/core/principal'
import { Header } from '@ldclabs/cose-ts/header'
import { Encrypt0Message } from '@ldclabs/cose-ts/encrypt0'
import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { ECDHKey } from '@ldclabs/cose-ts/ecdh'
import * as iana from '@ldclabs/cose-ts/iana'
import { argon2idAsync } from '@noble/hashes/argon2.js'
import {
  LegacyLocalReader,
  LegacyCheckpointStore,
  unlockLegacyExchange,
  LegacyReader,
  type ReaderSource,
  digest,
  pack,
  OSS_CHUNK,
  legacyDerive,
  legacyDecrypt,
  unlockLegacyRoot,
  unlockLegacyChannel,
  decodeLegacyMessage
} from '../src'

const id = Principal.fromUint8Array(new Uint8Array([1, 2, 3])).toText()
const canister = 'aaaaa-aa'
const source: ReaderSource = {
  principal: id,
  message: canister,
  mode: 'Local',
  allowed: {
    message: [canister],
    channel: [canister],
    profile: [canister],
    cose: [canister],
    bucket: [canister]
  }
}
const user = Principal.fromText(id)
function reader(
  call: (method: string, args: any[]) => Promise<any>,
  mode: ReaderSource['mode'] = 'Local'
) {
  return new LegacyReader(
    { ...source, mode },
    { identity: () => id, call: async (_s, _c, method, args) => call(method, args) }
  )
}
async function encrypt(key: AesGcmKey, data: Uint8Array, aad: Uint8Array = new Uint8Array()) {
  return new Encrypt0Message(
    data,
    new Header().setParam(1, 3),
    new Header().setParam(5, crypto.getRandomValues(new Uint8Array(12)))
  ).toBytes(key, aad)
}
describe('legacy reader has no business write path', () => {
  it('does not create a missing original database and reads an existing one without changing wrappers', async () => {
    expect(await LegacyLocalReader.open(id)).toBeNull()
    expect(await indexedDB.databases()).toEqual([])
    const name = `ICPanda_${id}`
    const db = await openDB(name, 1, {
      upgrade(db) {
        for (const s of ['My', 'Keys', 'Channels', 'Messages']) db.createObjectStore(s)
      }
    })
    const wrapper = new Uint8Array([1, 2, 3])
    await db.put('Keys', wrapper, 'KEK:source:1')
    const local = await LegacyLocalReader.open(id)
    expect(await local!.get('Keys', 'KEK:source:1')).toEqual(wrapper)
    const values = []
    for await (const entry of local!.entries('Keys')) values.push(entry)
    expect(values).toHaveLength(1)
    expect(await db.get('Keys', 'KEK:source:1')).toEqual(wrapper)
    local!.close()
    db.close()
    await deleteDB(name)
  })
  it('denies unknown calls and destinations before transport, and checks identity after awaiting', async () => {
    let count = 0
    const r = reader(async () => {
      count++
      return { Ok: null }
    })
    await expect(r.call('cose', canister, 'setting_create')).rejects.toMatchObject({
      code: 'permission'
    })
    await expect(r.call('cose', id, 'setting_get')).rejects.toMatchObject({
      code: 'permission'
    })
    expect(count).toBe(0)
    let caller = id
    const changed = new LegacyReader(source, {
      identity: () => caller,
      call: async () => {
        caller = canister
        return { Ok: null }
      }
    })
    await expect(changed.call('message', canister, 'my_iv')).rejects.toMatchObject({
      code: 'permission'
    })
  })
  it('falls back only from definite NotFound and retains actual historical ownership', async () => {
    const paths: any[] = []
    const r = reader(async (_method, args) => {
      paths.push(args[0])
      return args[0].user_owned ? { Err: 'not found' } : { Ok: { dek: [new Uint8Array([7])] } }
    })
    const result = await r.setting({ user_owned: true, key: pack('PANDA') }, canister)
    expect(result.path.user_owned).toBe(false)
    expect(paths).toHaveLength(2)
    const denied = reader(async () => ({ Err: 'permission denied' }))
    await expect(denied.setting({ user_owned: true }, canister)).rejects.toMatchObject({
      code: 'permission'
    })
    expect(denied.inventory.calls).toHaveLength(1)
  })
  it('continues explicit message ranges after short pages and labels deleted and absent data', async () => {
    const ranges: number[] = []
    const r = reader(async (method, args) => {
      if (method === 'get_user')
        return { Ok: { id: user, profile_canister: Principal.fromText(canister) } }
      if (method === 'get_profile') return { Err: 'permission denied' }
      if (method === 'get_state')
        return {
          Ok: {
            channel_canisters: [Principal.fromText(canister)],
            matured_channel_canisters: []
          }
        }
      if (method === 'my_channel_ids') return { Ok: [1] }
      if (method === 'get_channel_if_update')
        return {
          Ok: args[1]
            ? []
            : [
                {
                  id: 1,
                  canister: Principal.fromText(canister),
                  message_start: 1,
                  latest_message_id: 41,
                  updated_at: 1n,
                  deleted_messages: [2]
                }
              ]
        }
      if (method === 'list_messages') {
        ranges.push(args[1][0])
        return { Ok: [{ id: args[1][0], kind: 0, payload: new Uint8Array([1]) }] }
      }
      if (method === 'get_message')
        return args[1] === 3
          ? { Ok: { id: 3, kind: 0, payload: new Uint8Array([1]) } }
          : { Err: 'not found' }
      throw new Error('unexpected call')
    })
    const report = await r.collect()
    expect(ranges).toEqual([1, 21, 41])
    expect(report.objects.filter((v) => v.kind === 'message')).toHaveLength(4)
    expect(report.gaps.some((g) => g.detail.includes('declares deleted'))).toBe(true)
    expect(report.snapshot).toBe('pre_migration')
    expect(
      report.calls.every((c) => !/^(create|update|leave|save|sign_in)/.test(c.method))
    ).toBe(true)
  })
  it('reassembles exact 256KiB transport chunks before whole-file AEAD, rejecting missing/reordered chunks', async () => {
    const key = AesGcmKey.fromSecret(new Uint8Array(32).fill(9))
    const plain = new Uint8Array(OSS_CHUNK + 33).fill(3)
    const encrypted = await encrypt(key, plain)
    const hash = Uint8Array.from(digest(encrypted).match(/../g)!, (x) => parseInt(x, 16))
    let corrupt = false
    const r = reader(async (method, args) =>
      method === 'get_file_info'
        ? {
            Ok: {
              size: BigInt(encrypted.length),
              filled: BigInt(encrypted.length),
              chunks: 2,
              hash: [hash]
            }
          }
        : {
            Ok: [
              [
                corrupt ? 7 : args[1],
                encrypted.slice(args[1] * OSS_CHUNK, (args[1] + 1) * OSS_CHUNK)
              ]
            ]
          }
    )
    const file = await r.file(canister, 1, new Uint8Array([1]))
    expect(file.chunks.map((c) => c.size)).toEqual([OSS_CHUNK, encrypted.length - OSS_CHUNK])
    expect(await legacyDecrypt(key, file.bytes)).toEqual(plain)
    corrupt = true
    await expect(r.file(canister, 1, new Uint8Array([1]))).rejects.toMatchObject({
      code: 'missing'
    })
  })
})
describe('legacy unlock never initializes or replaces keys', () => {
  const original = {
    principal: id,
    keyId: pack('PANDA'),
    myIV: new Uint8Array(32).fill(4),
    canister,
    namespace: id.replaceAll('-', '_'),
    userOwned: true,
    settingVersion: 0,
    contextVersion: 1 as const,
    keyName: 'key_1'
  }
  it.each(['Local', 'ECDH', 'VetKey'] as const)(
    'opens an original %s browser wrapper with exact Principal AAD',
    async (mode) => {
      const password = 'test password',
        salt = id
      const secret =
        mode === 'VetKey'
          ? new Uint8Array()
          : await argon2idAsync(password, salt, { t: 2, m: 19456, p: 1 })
      const master = AesGcmKey.fromSecret(new Uint8Array(32).fill(11))
      const wrapped = await encrypt(
        AesGcmKey.fromSecret(legacyDerive(secret, original.myIV)),
        master.toBytes(),
        user.toUint8Array()
      )
      const r = reader(async () => {
        throw new Error('network forbidden')
      }, mode)
      const cached = {
        kind: mode,
        version: 2,
        keyId: original.keyId,
        encryptedSecret: wrapped
      }
      const opened = await unlockLegacyRoot(
        r,
        { ...original, mode },
        { password, salt, cached }
      )
      expect(opened.key.getSecret()).toEqual(master.getSecret())
      expect(r.inventory.calls).toHaveLength(0)
      const kek = AesGcmKey.fromSecret(new Uint8Array(32).fill(12)),
        dek = AesGcmKey.fromSecret(new Uint8Array(32).fill(13))
      const keys = await unlockLegacyChannel(
        opened.key,
        await encrypt(master, kek.toBytes()),
        await encrypt(kek, dek.toBytes())
      )
      expect(
        (
          await decodeLegacyMessage(keys.dek, {
            kind: 0,
            payload: await encrypt(dek, pack('history'))
          })
        ).text
      ).toBe('history')
      await expect(
        unlockLegacyChannel(opened.key, null, await encrypt(kek, dek.toBytes()))
      ).rejects.toMatchObject({ code: 'missing_key' })
      await expect(
        unlockLegacyRoot(
          r,
          { ...original, mode, myIV: new Uint8Array(32).fill(8) },
          { password, salt, cached }
        )
      ).rejects.toMatchObject({ code: 'corrupt' })
    }
  )
  it('recovers an ECDH remote key without any setting write', async () => {
    const password = 'test password',
      salt = id,
      remoteSecret = new Uint8Array(32).fill(9)
    const r = reader(async (method, args) => {
      expect(method).toBe('ecdh_cose_encrypted_key')
      const server = ECDHKey.generate(iana.EllipticCurveX25519)
      const shared = server.ecdh(
        ECDHKey.fromPublic(iana.EllipticCurveX25519, args[1].public_key)
      )
      return {
        Ok: {
          public_key: server.getPublicKey(),
          payload: await encrypt(
            AesGcmKey.fromSecret(shared),
            AesGcmKey.fromSecret(remoteSecret).toBytes(),
            user.toUint8Array()
          )
        }
      }
    }, 'ECDH')
    const root = await unlockLegacyRoot(r, { ...original, mode: 'ECDH' }, { password, salt })
    const p = await argon2idAsync(password, salt, { t: 2, m: 19456, p: 1 })
    expect(root.key.getSecret()).toEqual(legacyDerive(p, remoteSecret))
    expect(r.inventory.calls.map((c) => c.method)).toEqual(['ecdh_cose_encrypted_key'])
  })
  it('rejects missing remote VetKey wrappers and unexpected roots without generating a new master', async () => {
    const r = reader(async () => ({ Err: 'not found' }), 'VetKey')
    await expect(
      unlockLegacyRoot(r, { ...original, mode: 'VetKey' }, {})
    ).rejects.toMatchObject({ code: 'missing' })
    expect(r.inventory.calls.map((c) => c.method)).toEqual(['setting_get', 'setting_get'])
  })
  it('rejects an unreviewed derivation context before querying and accepts OSS file ID zero', async () => {
    const r = reader(async () => {
      throw new Error('must not query')
    }, 'VetKey')
    await expect(
      unlockLegacyRoot(r, { ...original, mode: 'VetKey', contextVersion: 2 }, {})
    ).rejects.toMatchObject({ code: 'version' })
    expect(r.inventory.calls).toHaveLength(0)
    const dek = AesGcmKey.fromSecret(new Uint8Array(32).fill(4))
    const payload = pack([
      'file zero',
      'File',
      pack({
        canister: Principal.fromText(canister).toUint8Array(),
        id: 0,
        name: 'empty',
        size: 0,
        type: 'application/octet-stream'
      })
    ])
    expect(
      (await decodeLegacyMessage(dek, { kind: 0, payload: await encrypt(dek, payload) })).file
        ?.id
    ).toBe(0)
  })
})

describe('durable source preservation', () => {
  it('preserves incomplete chunks without reporting a complete file', async () => {
    const r = reader(async (method, args) =>
      method === 'get_file_info'
        ? {
            Ok: {
              size: BigInt(OSS_CHUNK + 20),
              filled: BigInt(OSS_CHUNK),
              chunks: 1,
              hash: []
            }
          }
        : { Ok: args[1] === 0 ? [[0, new Uint8Array(OSS_CHUNK).fill(4)]] : [] }
    )
    const partial = await r.file(canister, 0, new Uint8Array(), undefined, true)
    expect(partial.complete).toBe(false)
    expect(partial.bytes).toHaveLength(0)
    expect(partial.present[0].bytes).toHaveLength(OSS_CHUNK)
    expect(partial.missing).toEqual([1])
    await expect(r.file(canister, 0, new Uint8Array())).rejects.toMatchObject({
      code: 'missing'
    })
  })
  it('reopens encrypted checkpoints in a dedicated database without storing plaintext', async () => {
    const r = reader(async () => ({ Ok: null }))
    r.add('profile-marker', 'profile', { bio: 'private-original-profile-marker' })
    const cache = await LegacyCheckpointStore.open(source)
    await cache.save({ inventory: r.inventory, completed: ['range/1-20'] })
    const db = await openDB('dmsg-legacy-export/1')
    const row = await db.get('state', cache.id)
    expect(new TextDecoder().decode(row.bytes)).not.toContain(
      'private-original-profile-marker'
    )
    expect((await db.get('state', 'key')).extractable).toBe(false)
    db.close()
    cache.close()
    const reopened = await LegacyCheckpointStore.open(source)
    expect(await reopened.load()).toEqual({
      inventory: r.inventory,
      completed: ['range/1-20']
    })
    await reopened.clear()
    expect(await reopened.load()).toBeNull()
    reopened.close()
  })
  it('opens a pending static ECDH exchange locally and rejects another recipient', async () => {
    const recipient = ECDHKey.generate(iana.EllipticCurveX25519),
      sender = ECDHKey.generate(iana.EllipticCurveX25519)
    const kek = AesGcmKey.fromSecret(new Uint8Array(32).fill(31)),
      dek = AesGcmKey.fromSecret(new Uint8Array(32).fill(32))
    const secret = sender.ecdh(
      ECDHKey.fromPublic(iana.EllipticCurveX25519, recipient.getPublicKey())
    )
    const wrapped = await encrypt(AesGcmKey.fromSecret(secret), kek.toBytes()),
      wrappedDek = await encrypt(kek, dek.toBytes())
    const result = await unlockLegacyExchange(
      recipient.toBytes(),
      recipient.getPublicKey(),
      [sender.getPublicKey(), wrapped],
      wrappedDek
    )
    expect(result.dek.toBytes()).toEqual(dek.toBytes())
    await expect(
      unlockLegacyExchange(
        recipient.toBytes(),
        sender.getPublicKey(),
        [sender.getPublicKey(), wrapped],
        wrappedDek
      )
    ).rejects.toMatchObject({ code: 'missing_key' })
  })
})
