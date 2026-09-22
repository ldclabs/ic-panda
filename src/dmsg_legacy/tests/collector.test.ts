import 'fake-indexeddb/auto'
import { beforeEach, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { openDB } from 'idb'
import { Principal } from '@icp-sdk/core/principal'
import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'
import { Encrypt0Message } from '@ldclabs/cose-ts/encrypt0'
import { Header } from '@ldclabs/cose-ts/header'
import { argon2idAsync } from '@noble/hashes/argon2.js'
import {
  LegacyReader,
  collectArchive,
  encodeArchive,
  decodeArchive,
  verifyArchiveContent,
  openArchiveFile,
  legacyDerive,
  pack,
  OSS_CHUNK
} from '../src'
beforeEach(() => {
  globalThis.indexedDB = new IDBFactory()
})
const canister = Principal.fromUint8Array(new Uint8Array([1])),
  user = Principal.fromUint8Array(new Uint8Array([2])),
  principal = user.toText(),
  source = canister.toText()
const encrypt = (key: AesGcmKey, data: Uint8Array, aad: Uint8Array = new Uint8Array()) =>
  new Encrypt0Message(
    data,
    new Header().setParam(1, 3),
    new Header().setParam(5, crypto.getRandomValues(new Uint8Array(12)))
  ).toBytes(key, aad)
it.each(['Local', 'ECDH', 'VetKey'] as const)(
  'collects a complete %s browser archive and opens historical messages/files offline without any old write',
  async (mode) => {
    const password = 'old synthetic password',
      iv = new Uint8Array(32).fill(4),
      keyId = pack('PANDA')
    const master = AesGcmKey.fromSecret(new Uint8Array(32).fill(8)),
      kek = AesGcmKey.fromSecret(new Uint8Array(32).fill(9)),
      dek = AesGcmKey.fromSecret(new Uint8Array(32).fill(10))
    const secret =
      mode === 'VetKey'
        ? new Uint8Array()
        : await argon2idAsync(password, principal, { t: 2, m: 19456, p: 1 })
    const root = {
      kind: mode,
      keyId,
      encryptedSecret: await encrypt(
        AesGcmKey.fromSecret(legacyDerive(secret, iv)),
        master.toBytes(),
        user.toUint8Array()
      )
    }
    const name = `ICPanda_${principal}`,
      local = await openDB(name, 1, {
        upgrade(db) {
          for (const key of ['My', 'Keys', 'Channels', 'Messages']) db.createObjectStore(key)
        }
      })
    await local.put('Keys', [root], 'MK')
    await local.put('Keys', await encrypt(master, kek.toBytes()), `KEK:${source}:1`)
    const before = await local.getAll('Keys')
    const plaintext = new Uint8Array(OSS_CHUNK + 23).fill(37),
      ciphertext = await encrypt(dek, plaintext)
    const message = {
      id: 1,
      kind: 0,
      reply_to: 0,
      created_by: user,
      created_at: 10n,
      payload: await encrypt(
        dek,
        pack([
          'attachment',
          'File',
          pack({
            canister: canister.toUint8Array(),
            id: 0,
            name: 'old.bin',
            size: plaintext.length,
            type: 'application/octet-stream'
          })
        ])
      )
    }
    const channel = {
      id: 1,
      canister,
      message_start: 1,
      latest_message_id: 1,
      updated_at: 1n,
      deleted_messages: [],
      dek: await encrypt(kek, dek.toBytes()),
      files_state: [{ file_storage: [canister, 1] }]
    }
    const calls: string[] = []
    const reader = new LegacyReader(
      {
        principal,
        message: source,
        mode,
        allowed: {
          message: [source],
          profile: [source],
          channel: [source],
          cose: [source],
          bucket: [source]
        }
      },
      {
        identity: () => principal,
        call: async (_service, _canister, method, args: any[]) => {
          calls.push(method)
          switch (method) {
            case 'get_user':
              return {
                Ok: { id: user, profile_canister: canister, cose_canister: [], image: '' }
              }
            case 'get_profile':
              return { Ok: { bio: 'legacy profile' } }
            case 'get_state':
              return { Ok: { channel_canisters: [canister], matured_channel_canisters: [] } }
            case 'my_channel_ids':
              return { Ok: [1] }
            case 'get_channel_if_update':
              return { Ok: args[1] ? [] : [channel] }
            case 'list_messages':
              return { Ok: [message] }
            case 'my_iv':
              return { Ok: iv }
            case 'download_files_token':
              return {
                Ok: { storage: [canister, 1], access_token: new Uint8Array([91, 92, 93]) }
              }
            case 'get_folder_info':
              return { Ok: { id: 1, files: [0] } }
            case 'get_file_info':
              return {
                Ok: {
                  id: 0,
                  parent: 1,
                  name: 'cipher',
                  size: BigInt(ciphertext.length),
                  filled: BigInt(ciphertext.length),
                  chunks: 2,
                  hash: []
                }
              }
            case 'get_file_chunks':
              return {
                Ok: [
                  [args[1], ciphertext.slice(args[1] * OSS_CHUNK, (args[1] + 1) * OSS_CHUNK)]
                ]
              }
            default:
              throw new Error(`Unexpected legacy method ${method}`)
          }
        }
      }
    )
    const archive = await collectArchive(reader, {
      password,
      salt: principal,
      keyId,
      contextVersion: 1,
      keyName: 'test_key_1'
    })
    const recovered = decodeArchive(encodeArchive(archive)),
      report = await verifyArchiveContent(recovered)
    expect(report.partial).toBe(false)
    expect(report.messages[0]?.file?.name).toBe('old.bin')
    expect((await openArchiveFile(recovered, `${source}/file/0`)).bytes).toEqual(plaintext)
    expect(await local.getAll('Keys')).toEqual(before)
    expect(
      calls.every(
        (method) => !/^(create|update|save|delete|leave|try_|setting_set)/.test(method)
      )
    ).toBe(true)
    local.close()
  }
)
