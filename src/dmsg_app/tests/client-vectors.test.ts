// Language-independent vectors for the Rust client (`tests/vectors/client`).
// Every artifact comes from this extension's real code. Randomness and the clock
// are fixed, so a normal run regenerates each file and must match it byte for
// byte; `DMSG_WRITE_CLIENT_VECTORS=1` rewrites them. See the directory README.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { sha256 } from '@noble/hashes/sha2.js'
import { MasterPublicKey } from '@icp-sdk/vetkeys'
import { Principal } from '@icp-sdk/core/principal'
import { currentWorkspace, WorkspaceDB } from '../src/lib/db'
import type { EncryptedObject, Item } from '../src/lib/models'
import {
  Tagged,
  b64,
  canonical,
  decodeCanonical,
  hash,
  hex,
  unb64,
  unhex,
  utf8
} from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
import {
  derive,
  deviceRootContext,
  ed25519,
  hpke,
  hpkeOpen,
  hpkePublic,
  open,
  seal,
  unlockKey
} from '../src/lib/crypto/primitives'
import {
  bundleDigests,
  parseRootBundle,
  wrapRoot,
  type RecoveryKey,
  type RootMaterial
} from '../src/lib/crypto/root'
import { emptyFile, objectBytes, openContentManifest } from '../src/lib/protocol/content'
import {
  advanceChannel,
  recipientDigest,
  startChannel,
  type ChannelLedger,
  type EpochRecipient
} from '../src/lib/protocol/channel'
import { boundEngine } from './support/engine'

const directory = new URL('../../../tests/vectors/client/', import.meta.url)
const generator = 'src/dmsg_app/tests/client-vectors.test.ts'
const now = 1_760_000_000_000

/** Typed JSON for one CBOR item, the same shape as `dmsg_types/tests/protocol_vectors.json`. */
type Typed =
  | { uint: string }
  | { int: string }
  | { text: string }
  | { bytes: string }
  | { bool: boolean }
  | { null: true }
  | { array: Typed[] }
  | { map: [Typed, Typed][] }
  | { tag: number; value: Typed }
function typed(value: unknown): Typed {
  if (value === null) return { null: true }
  if (typeof value === 'boolean') return { bool: value }
  if (typeof value === 'number' || typeof value === 'bigint') {
    const n = BigInt(value)
    return n < 0n ? { int: String(n) } : { uint: String(n) }
  }
  if (typeof value === 'string') return { text: value }
  if (value instanceof Uint8Array) return { bytes: hex(value) }
  if (value instanceof Tagged) return { tag: value.tag, value: typed(value.value) }
  if (Array.isArray(value)) return { array: value.map(typed) }
  if (value instanceof Map) return { map: [...value].map(([k, v]) => [typed(k), typed(v)]) }
  return {
    map: Object.entries(value as Record<string, unknown>).map(([k, v]) => [typed(k), typed(v)])
  }
}
/** An AAD/context with its exact canonical bytes. */
const context = (value: unknown) => ({ value: typed(value), hex: hex(canonical(value)) })

/** SHA-256 counter stream standing in for `crypto.getRandomValues`. */
function seeded(label: string) {
  const seed = utf8(`dmsg-client-vectors/1:${label}`)
  let counter = 0,
    block = new Uint8Array(0),
    offset = 0
  return <T extends ArrayBufferView | null>(array: T): T => {
    const out = new Uint8Array(array!.buffer, array!.byteOffset, array!.byteLength)
    for (let i = 0; i < out.length; i++) {
      if (offset === block.length) {
        const c = new Uint8Array(4)
        new DataView(c.buffer).setUint32(0, counter++)
        block = sha256(Uint8Array.from([...seed, ...c]))
        offset = 0
      }
      out[i] = block[offset++]!
    }
    return array
  }
}
function settle(file: string, value: unknown) {
  const text = JSON.stringify(value, null, 2) + '\n',
    path = new URL(file, directory)
  if (process.env.DMSG_WRITE_CLIENT_VECTORS === '1') {
    mkdirSync(directory, { recursive: true })
    writeFileSync(path, text)
  }
  expect(readFileSync(path, 'utf8')).toBe(text)
}
const header = (description: string) => ({
  schema: 1,
  test_only: true,
  generator,
  description
})
const fill = (n: number, length = 32) => new Uint8Array(length).fill(n)

beforeEach((test) => {
  globalThis.indexedDB = new IDBFactory()
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(now)
  vi.spyOn(globalThis.crypto, 'getRandomValues').mockImplementation(
    seeded(test.task.name) as typeof crypto.getRandomValues
  )
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.useRealTimers()
})

it('primitives', async () => {
  const cbor = [
    ['unsigned boundaries', [0, 23, 24, 255, 256, 65535, 65536, 4294967295, 4294967296]],
    ['max safe integer', Number.MAX_SAFE_INTEGER],
    ['u64 max', 18446744073709551615n],
    ['negative boundaries', [-1, -24, -25, -256, -257, -65536, -65537]],
    ['i64 min', -9223372036854775808n],
    ['bignum tag 2', new Tagged(2, Uint8Array.from([1, 0, 0, 0, 0, 0, 0, 0, 0]))],
    ['text', ['', 'a', 'IETF', '你好🌍', 'x'.repeat(24), 'y'.repeat(256)]],
    ['bytes', [new Uint8Array(), fill(7, 1), fill(8, 24), fill(9, 256)]],
    ['simple values', [true, false, null]],
    ['text key order', { b: 1, a: 2, aa: 3, '': 4, ba: 5 }],
    [
      'mixed key order',
      new Map<unknown, unknown>([
        [-1, 'negative'],
        [100, 'two-byte uint'],
        ['a', 'text'],
        [new Uint8Array([1]), 'bytes'],
        [1, 'uint']
      ])
    ],
    [
      'content aad',
      [
        'dmsg/content/1',
        'local',
        'subject',
        2,
        'object',
        'revision',
        'device',
        'vault',
        null,
        false
      ]
    ],
    ['nested', { list: [[[]], { k: new Uint8Array([0xff]) }], n: -2 }]
  ] as const
  const rejects = [
    ['non-minimal integer', '1801'],
    ['trailing item', '0101'],
    ['duplicate key', 'a2616101616102'],
    ['unsorted keys', 'a2616201616101'],
    ['indefinite array', '9fff'],
    ['indefinite text', '7f6161ff'],
    ['undefined', 'f7'],
    ['half float', 'f93c00'],
    ['single float', 'fa3f800000'],
    ['double float', 'fb3ff0000000000000'],
    ['simple 16', 'f0'],
    ['tag 0', 'c001'],
    ['tag 24', 'd81840'],
    ['bignum of 8 bytes', 'c2480102030405060708'],
    ['bignum with leading zero', 'c2490001000000000000000000'],
    ['bignum of 17 bytes', 'c25101' + '00'.repeat(16)],
    ['invalid utf-8', '62c328'],
    ['lone surrogate', '63eda080'],
    ['truncated', '83010203'.slice(0, 6)],
    ['nested 25 levels', '81'.repeat(25) + '01'],
    ['empty input', '']
  ] as const
  for (const [, input] of rejects) expect(() => decodeCanonical(unhex(input))).toThrow()
  expect(decodeCanonical(unhex('81'.repeat(24) + '01'))).toBeTruthy()

  const base64url = {
    accept: [
      ['', ''],
      ['Zg', '66'],
      ['Zm8', '666f'],
      ['Zm9v', '666f6f'],
      ['-_8', 'fbff']
    ].map(([text, bytes]) => {
      expect(b64(bytes ? unhex(bytes) : new Uint8Array())).toBe(text)
      return { text, hex: bytes }
    }),
    reject: ['Zh', 'Zm9=', 'Zg==', 'Z', 'Zm9v+', 'Zm9v/', ' Zg', 'Zg\n']
  }
  for (const text of base64url.reject) expect(() => unb64(text)).toThrow()

  const xidSamples = [fill(0, 12), fill(1, 12), fill(0xff, 12)]
  xidSamples.push(Uint8Array.from({ length: 12 }, (_, i) => i * 21))
  const xid = xidSamples.map((bytes) => ({ hex: hex(bytes), text: xidText(bytes) }))
  const workspace = `dmsg:local:${xidText(fill(1, 12))}:${'22'.repeat(32)}`
  const deriveCases = [
    { name: 'generic', key: fill(3), context: ['dmsg/root-commitment/1'] },
    { name: 'map context', key: fill(4), context: { purpose: 'test', n: 1 } }
  ].map(({ name, key, context: c }) => ({
    name,
    key_hex: hex(key),
    context: context(c),
    output_hex: hex(derive(key, c))
  }))
  const secret = fill(5)
  const local = {
    workspace,
    secret_hex: hex(secret),
    unlock_hex: hex(unlockKey(secret, workspace))
  }
  expect(local.unlock_hex).toBe(hex(derive(secret, ['dmsg/local-unlock/2', workspace])))

  const sealCases = []
  for (const [name, key, iv, plaintext, aad] of [
    ['short', fill(10), fill(11, 12), utf8('hello'), ['dmsg/content/1', 'local', 'a', 1]],
    ['empty plaintext', fill(12), fill(13, 12), new Uint8Array(), []],
    ['map aad', fill(14), fill(15, 12), fill(16, 100), { kind: 'vault', generation: 3 }]
  ] as const) {
    const sealed = await seal(key, plaintext, aad, iv)
    expect(await open(key, sealed, aad)).toEqual(plaintext)
    sealCases.push({
      name,
      key_hex: hex(key),
      iv_hex: hex(iv),
      plaintext_hex: hex(plaintext),
      aad: context(aad),
      enc_structure_hex: hex(
        canonical(['Encrypt0', canonical(new Map([[1, 3]])), canonical(aad)])
      ),
      sealed,
      cose_hex: hex(unb64(sealed))
    })
  }
  const base = sealCases[0]!,
    sample = ['dmsg/content/1', 'local', 'a', 1]
  const cose = unb64(base.sealed),
    [prot, , body] = decodeCanonical<[Uint8Array, Map<number, Uint8Array>, Uint8Array]>(cose)
  const iv = new Map<number, unknown>([[5, fill(11, 12)]])
  const variant = (header: Uint8Array, unprotected: Map<number, unknown>, ciphertext = body) =>
    b64(canonical([header, unprotected, ciphertext]))
  const flipped = Uint8Array.from(cose, (b, i) => (i === cose.length - 1 ? b ^ 1 : b))
  const openRejects = [
    ['wrong key', hex(fill(99)), base.sealed, sample],
    ['wrong aad', base.key_hex, base.sealed, ['dmsg/content/1', 'local', 'b', 1]],
    ['flipped tag', base.key_hex, b64(flipped), sample],
    ['A128GCM header', base.key_hex, variant(canonical(new Map([[1, 1]])), iv), sample],
    ['short iv', base.key_hex, variant(prot, new Map([[5, fill(11, 11)]])), sample],
    [
      'extra unprotected header',
      base.key_hex,
      variant(prot, new Map<number, unknown>([[4, fill(1, 1)], ...iv])),
      sample
    ],
    ['tagged COSE_Encrypt0', base.key_hex, b64(Uint8Array.from([0xd0, ...cose])), sample],
    ['tag shorter than 16 bytes', base.key_hex, variant(prot, iv, body.subarray(0, 15)), sample]
  ] as const
  const openReject = []
  for (const [name, key, sealed, aad] of openRejects) {
    await expect(open(unhex(key), sealed, aad)).rejects.toThrow()
    openReject.push({ name, key_hex: key, sealed, aad: context(aad) })
  }

  const hpkeCases = []
  for (const [name, seed, ekm, plaintext, info] of [
    [
      'device root',
      fill(21),
      fill(40),
      fill(50),
      deviceRootContext('local', xidText(fill(1, 12)), 2, '1f'.repeat(32))
    ],
    [
      'inbox envelope',
      fill(22),
      fill(41),
      canonical({ text: 'hello' }),
      ['dmsg/inbox-envelope/1', 'sender', 'recipient', 'order', 1]
    ]
  ] as const) {
    const publicKey = await hpkePublic(seed)
    const result = await hpke.seal(
      {
        recipientPublicKey: await hpke.kem.deserializePublicKey(unb64(publicKey)),
        info: canonical(info),
        ekm: Uint8Array.from(ekm).buffer
      },
      Uint8Array.from(plaintext).buffer,
      canonical(info)
    )
    const envelope = {
      enc: b64(new Uint8Array(result.enc)),
      ciphertext: b64(new Uint8Array(result.ct))
    }
    expect(await hpkeOpen(seed, envelope, info)).toEqual(Uint8Array.from(plaintext))
    hpkeCases.push({
      name,
      seed_hex: hex(seed),
      public_hex: hex(unb64(publicKey)),
      ekm_hex: hex(ekm),
      plaintext_hex: hex(plaintext),
      context: context(info),
      enc_hex: hex(unb64(envelope.enc)),
      ciphertext_hex: hex(unb64(envelope.ciphertext)),
      envelope_hex: hex(canonical(envelope))
    })
  }
  const signatures = [
    [fill(7), utf8('dMsg')],
    [fill(8), sha256(utf8('digest'))]
  ].map(([seed, message]) => ({
    seed_hex: hex(seed!),
    public_hex: hex(ed25519.getPublicKey(seed!)),
    message_hex: hex(message!),
    signature_hex: hex(ed25519.sign(message!, seed!))
  }))

  settle('primitives-v1.json', {
    ...header(
      'Canonical CBOR, base64url, Xid, D(K,C), COSE_Encrypt0, HPKE and Ed25519. Every output is deterministic and must be reproduced exactly.'
    ),
    cbor: cbor.map(([name, value]) => ({ name, value: typed(value), hex: hex(canonical(value)) })),
    cbor_reject: rejects.map(([name, input]) => ({ name, hex: input })),
    cbor_limits: { max_depth: 24, max_nodes: 50000, max_container_items: 25000 },
    base64url,
    xid,
    derive: deriveCases,
    local_unlock: local,
    seal: sealCases,
    open_reject: openReject,
    hpke: hpkeCases,
    ed25519: signatures
  })
})

const note = (title: string): Item => ({
  type: 'api',
  title,
  body: 'private deployment instructions',
  username: 'private-account',
  secret: 'secret-test-token-no-plaintext',
  tags: ['hidden-tag'],
  url: '',
  favorite: false,
  createdAt: 1,
  updatedAt: 1
})
async function objectSample(engine: any, root: Uint8Array, name: string, record: EncryptedObject) {
  const aad = [
    'dmsg/content/1',
    'local',
    record.subjectId,
    record.generation,
    record.id,
    record.revision,
    record.deviceId,
    record.kind,
    record.parent,
    record.tombstone
  ]
  const wrap = [
    'dmsg/wrap/1',
    record.subjectId,
    record.id,
    record.generation,
    record.revision,
    'item-version'
  ]
  expect(hash(canonical([aad, record.ciphertext, record.keyEnvelope]))).toBe(record.digest)
  const versionKey = await open(root, record.keyEnvelope, wrap)
  const plaintext = await open(versionKey, record.ciphertext, aad)
  expect(plaintext).toEqual(canonical(await engine.decode(record)))
  return {
    name,
    record,
    content_aad: context(aad),
    wrap_aad: context(wrap),
    digest_input_hex: hex(canonical([aad, record.ciphertext, record.keyEnvelope])),
    version_key_hex: hex(versionKey),
    plaintext_hex: hex(plaintext),
    payload: typed(decodeCanonical(plaintext)),
    object_hex: hex(objectBytes(record))
  }
}

it('content', async () => {
  const { engine, meta } = await boundEngine()
  const internal = engine as any,
    root = unb64(internal.bundle.root)
  const first = (await engine.saveItem({ item: note('Vector item') })) as EncryptedObject
  const edited = (await engine.saveItem({
    item: note('Vector item, edited'),
    id: first.id,
    base: first.revision
  })) as EncryptedObject
  const deleted = (await engine.deleteItem({
    id: edited.id,
    base: edited.revision
  })) as EncryptedObject
  const text = utf8('A small file. '.repeat(20))
  const file = (await engine.importFile(
    new File([text], 'vector.txt', { type: 'text/plain' })
  )) as EncryptedObject
  const empty = (await engine.importFile(new File([], 'empty.bin'))) as EncryptedObject
  const objects = []
  for (const [name, record] of [
    ['vault item', first],
    ['edit with parent', edited],
    ['tombstone', deleted],
    ['file entry', file],
    ['empty file entry', empty]
  ] as const)
    objects.push(await objectSample(internal, root, name, record))

  const db = await WorkspaceDB.open((await currentWorkspace())!)
  const manifest = (await internal.decode(file)).file
  const chunks = []
  for (const [index, ref] of manifest.chunks.entries()) {
    const stored = await db.db.get('chunks', ref.id)
    const aad = [
      'dmsg/file-chunk/1',
      manifest.id,
      manifest.version,
      index,
      manifest.chunks.length,
      ref.size
    ]
    const nonce = new Uint8Array(12)
    nonce.set([0x64, 0x4d, 0x73, 0x67])
    new DataView(nonce.buffer).setBigUint64(4, BigInt(index))
    const [, unprotected] = decodeCanonical<[Uint8Array, Map<number, Uint8Array>, Uint8Array]>(
      unb64(stored.ciphertext)
    )
    expect(unprotected.get(5)).toEqual(nonce)
    expect(hash(unb64(stored.ciphertext))).toBe(ref.digest)
    chunks.push({
      id: stored.id,
      index,
      nonce_hex: hex(nonce),
      aad: context(aad),
      ciphertext: stored.ciphertext,
      digest: ref.digest,
      plaintext_hex: hex(await open(unb64(manifest.key), stored.ciphertext, aad))
    })
  }
  db.db.close()

  // The engine always uses 1 MiB chunks; these fix the nonce and AAD rule for
  // later indices with small illustrative plaintexts.
  const chunkRules = []
  for (const [index, count, size] of [
    [0, 3, 1048576],
    [1, 3, 1048576],
    [2, 3, 19]
  ] as const) {
    const key = fill(60),
      plaintext = fill(61 + index, 19),
      fileId = '6a'.repeat(32),
      version = '6b'.repeat(32)
    const nonce = new Uint8Array(12)
    nonce.set([0x64, 0x4d, 0x73, 0x67])
    new DataView(nonce.buffer).setBigUint64(4, BigInt(index))
    const aad = ['dmsg/file-chunk/1', fileId, version, index, count, size]
    const sealed = await seal(key, plaintext, aad, nonce)
    chunkRules.push({
      file_id: fileId,
      version,
      index,
      count,
      plaintext_length: size,
      key_hex: hex(key),
      plaintext_hex: hex(plaintext),
      nonce_hex: hex(nonce),
      aad: context(aad),
      sealed,
      digest: hash(unb64(sealed))
    })
  }

  const uploads = []
  for (const [name, record] of [
    ['vault upload', first],
    ['file upload', file],
    ['empty file upload', empty]
  ] as const) {
    const job = await engine.contentPrepare(record.key)
    for (const upload of job.uploads) {
      const manifestBytes = unb64(upload.manifest)
      const content = await openContentManifest(root, meta.account.id, upload.plan, manifestBytes)
      uploads.push({
        name: `${name}: ${upload.plan.kind}`,
        plan: upload.plan,
        manifest: upload.manifest,
        manifest_aad: context([
          'dmsg/cloud-manifest/1',
          meta.account.id,
          upload.plan.upload_id,
          upload.plan.object_id,
          upload.plan.version_id,
          upload.plan.kind,
          upload.plan.root_generation
        ]),
        manifest_plaintext_hex: hex(
          await open(root, upload.manifest, [
            'dmsg/cloud-manifest/1',
            meta.account.id,
            upload.plan.upload_id,
            upload.plan.object_id,
            upload.plan.version_id,
            upload.plan.kind,
            upload.plan.root_generation
          ])
        ),
        content: content === null ? null : typed(content),
        object: upload.object ?? null
      })
    }
  }

  settle('content-v1.json', {
    ...header(
      'Encrypted objects, file chunks and upload manifests produced by the extension engine. Readers must recompute every AAD, digest and container exactly and open each ciphertext; ciphertexts used random keys and nonces except the chunk_rules.'
    ),
    environment: 'local',
    account: meta.account.id,
    subject_id: meta.subjectId,
    device_id: meta.deviceId,
    root_generation: meta.rootGeneration,
    root_hex: hex(root),
    objects,
    file_chunks: chunks,
    chunk_rules: chunkRules,
    empty_file_hex: hex(emptyFile()),
    uploads
  })
})

it('root', async () => {
  const account = xidText(fill(1, 12)),
    home = Principal.fromText('aaaaa-aa')
  const recoveryKey: RecoveryKey = {
    homeCose: home.toText(),
    keyName: 'key_1',
    publicKey: b64(
      MasterPublicKey.productionKey()
        .deriveCanisterKey(home.toUint8Array())
        .deriveSubKey(canonical(['dmsg/content-root/v2', 'Local', 2]))
        .publicKeyBytes()
    )
  }
  const devices = [fill(21), fill(22)].map((seed, i) => ({
    hpkeSeed: seed,
    deviceId: hex(fill(31 + i))
  }))
  const recipients: { deviceId: string; hpkePublic: string }[] = []
  for (const device of devices)
    recipients.push({ deviceId: device.deviceId, hpkePublic: await hpkePublic(device.hpkeSeed) })
  const signer = { deviceId: devices[1]!.deviceId, seed: fill(7) }
  const generations = []
  let previous: { digest: string; uploadId: string; generation: number; root: Uint8Array } | null =
    null
  for (const generation of [1, 2]) {
    const root = fill(70 + generation)
    const material: RootMaterial = {
      context: {
        account,
        environment: 'local',
        generation,
        opId: hex(fill(80 + generation)),
        securityEpoch: 4 + generation
      },
      root: b64(root),
      transport: b64(fill(0))
    }
    const bytes = await wrapRoot(
      material,
      recipients,
      recoveryKey,
      signer,
      previous && { ...previous, root: Uint8Array.from(previous.root) }
    )
    const bundle = parseRootBundle(bytes),
      digests = bundleDigests(bundle)
    for (const [i, envelope] of bundle.body.envelopes.entries()) {
      const device = devices.find((d) => d.deviceId === envelope.device)!
      expect(
        await hpkeOpen(
          device.hpkeSeed,
          envelope,
          deviceRootContext('local', account, generation, device.deviceId)
        )
      ).toEqual(root)
      expect(i === 0 || envelope.device > bundle.body.envelopes[i - 1]!.device).toBe(true)
    }
    generations.push({
      generation,
      root_hex: hex(root),
      context: material.context,
      bundle_hex: hex(bytes),
      bundle_sha256: hash(bytes),
      body: typed(bundle.body),
      signed_digest_hex: hex(
        sha256(canonical([1, 'dmsg/root-bundle/2', bundle.body]))
      ),
      commitment: bundle.body.commitment,
      commitment_input_hex: hex(derive(root, ['dmsg/root-commitment/1'])),
      device_contexts: bundle.body.envelopes.map((e) =>
        context(deviceRootContext('local', account, generation, e.device))
      ),
      previous_context:
        previous &&
        context([
          'dmsg/previous-root/1',
          account,
          generation,
          previous.generation,
          previous.digest
        ]),
      recipients_digest: hex(digests.recipientsDigest),
      body_digest: hex(digests.bodyDigest),
      bundle_digest: hex(digests.bundleDigest)
    })
    previous = { digest: hash(bytes), uploadId: hex(fill(90 + generation)), generation, root }
  }
  settle('root-v2.json', {
    ...header(
      'RootBundle v2 for two generations. Readers must parse the canonical bundle, verify the signer, recompute the three on-chain digests and the commitment, open every device envelope and follow the previous-root link. The IBE recovery envelope is covered by PocketIC, not here.'
    ),
    account,
    environment: 'local',
    recovery_key: recoveryKey,
    devices: devices.map((d, i) => ({
      device_id: d.deviceId,
      hpke_seed_hex: hex(d.hpkeSeed),
      hpke_public: recipients[i]!.hpkePublic
    })),
    signer: {
      device_id: signer.deviceId,
      seed_hex: hex(signer.seed),
      public_hex: hex(ed25519.getPublicKey(signer.seed))
    },
    generations
  })
})

it('channel', async () => {
  const owner = xidText(fill(1, 12)),
    member = xidText(fill(2, 12))
  const { engine, meta } = await boundEngine(owner)
  const internal = engine as any
  const channel = '0c'.repeat(32),
    genesis = '0d'.repeat(32)
  await engine.channelRemember({ channel, name: 'Vector channel', genesis })
  let ledger: ChannelLedger = startChannel(
    { channel_id: channel, nonce: '0e'.repeat(32), type: 'collaboration' },
    owner,
    genesis
  )
  await engine.channelAdvance(channel, ledger, 1, [])
  const memberSeed = fill(23),
    memberDevice = hex(fill(33))
  const recipients: EpochRecipient[] = [
    { recipient: `device:${owner}:${meta.deviceId}`, hpke_pub: hex(unb64(meta.hpkePublic)) },
    {
      recipient: `device:${member}:${memberDevice}`,
      hpke_pub: hex(unb64(await hpkePublic(memberSeed)))
    }
  ].sort((a, b) => a.recipient.localeCompare(b.recipient))
  const lease = {
    id: '0f'.repeat(32),
    account: owner,
    device: meta.deviceId,
    epoch: 1,
    head: genesis,
    recipients_digest: recipientDigest(recipients),
    security_versions: { [owner]: 1, [member]: 1 },
    fencing: 1,
    expires_at: now + 60000
  }
  const { activation, uploads } = await engine.channelRotation(channel, lease, recipients)
  const envelopeContext = (recipient: string) => [
    'dmsg/channel-epoch-envelope/1',
    channel,
    activation.epoch,
    activation.lease_id,
    activation.fencing,
    activation.expected_head,
    activation.recipients_digest,
    recipient
  ]
  const memberIndex = activation.envelopes.findIndex((e) => e.recipient.includes(member))
  const memberChunk = unb64(uploads[0]!.chunks[memberIndex]!)
  const epochKey = await hpkeOpen(
    memberSeed,
    decodeCanonical(memberChunk),
    envelopeContext(recipients[memberIndex]!.recipient)
  )
  // Activate epoch 1 with the member joined, then install this device's envelope.
  const step = (action: unknown, actor: string, head: string) => {
    ledger = advanceChannel(
      ledger,
      { expected_head: ledger.head, expected_acl_version: ledger.acl_version, action },
      actor,
      head,
      now
    )
  }
  ledger = { ...ledger, epoch: 1, status: 'active' }
  step(
    {
      type: 'invite',
      invitation_id: '10'.repeat(32),
      account: member,
      role: 'member',
      expires_at: now + 1000
    },
    owner,
    '11'.repeat(32)
  )
  step({ type: 'accept', invitation_id: '10'.repeat(32) }, member, '12'.repeat(32))
  ledger = { ...ledger, status: 'active' }
  await engine.channelAdvance(channel, ledger, 3, [])
  const ownIndex = activation.envelopes.findIndex((e) => e.recipient.includes(meta.deviceId))
  await engine.channelInstall(channel, activation, unb64(uploads[0]!.chunks[ownIndex]!))
  const messageId = '13'.repeat(32)
  const job = await engine.channelMessage(channel, 'Hello, channel vector', messageId)
  const { ciphertext, ...messageAad } = job.payload
  const messageKeyContext = ['dmsg/channel-message-key/1', channel, 1, messageId, meta.deviceId]
  const messageKey = derive(epochKey, messageKeyContext)
  const messagePlaintext = await open(messageKey, ciphertext, [
    'dmsg/channel-message/1',
    owner,
    meta.deviceId,
    messageAad
  ])
  const history = await engine.channelHistory(
    channel,
    member,
    1,
    1,
    [
      {
        recipient: `device:${member}:${memberDevice}`,
        hpke_pub: recipients[memberIndex]!.hpke_pub
      }
    ],
    '14'.repeat(32)
  )
  const historyContext = [
    'dmsg/channel-history-envelope/1',
    history.scope,
    `device:${member}:${memberDevice}`,
    1,
    1
  ]
  const historyPlaintext = await hpkeOpen(
    memberSeed,
    decodeCanonical(unb64(history.uploads[0]!.chunks[0]!)),
    historyContext
  )
  const { db } = await internal.ready()
  const state = await internal.decode(await db.getHead(channel, 'formal_channel'))
  expect(state.keys['1']).toBe(b64(epochKey))

  // A message with an attachment: the vault file's chunks plus a manifest
  // sealed under the file key, which the message plaintext carries.
  const attachmentId = '16'.repeat(32)
  const source = (await engine.importFile(
    new File([utf8('Attachment body')], 'attachment.txt', { type: 'text/plain' })
  )) as EncryptedObject
  const { fileRecord: _local, ...prepared } = await engine.channelFilePrepare(
    channel,
    attachmentId,
    source.key
  )
  const attached = await engine.channelMessage(channel, 'With a file', attachmentId)
  const { ciphertext: attachedCiphertext, ...attachedAad } = attached.payload
  const attachedKey = derive(epochKey, [
    'dmsg/channel-message-key/1',
    channel,
    1,
    attachmentId,
    meta.deviceId
  ])
  const attachedPlaintext = await open(attachedKey, attachedCiphertext, [
    'dmsg/channel-message/1',
    owner,
    meta.deviceId,
    attachedAad
  ])
  const reference = decodeCanonical<{
    file: { key: string; epoch: number; upload_id: string; file_id: string; version: string }
  }>(attachedPlaintext).file
  const manifestContext = [
    'dmsg/channel-file-manifest/1',
    channel,
    reference.epoch,
    reference.upload_id,
    reference.file_id,
    reference.version
  ]
  const manifestPlaintext = await open(unb64(reference.key), prepared.manifest, manifestContext)

  settle('channel-v1.json', {
    ...header(
      'One channel epoch: rotation envelopes and manifest, key confirmation, a message, a history grant and a message with an attachment, produced by the extension engine. Readers open the member envelopes with the member seed and must reproduce every context, digest and key derivation.'
    ),
    channel,
    genesis,
    owner: { account: owner, device_id: meta.deviceId, hpke_public: meta.hpkePublic },
    member: { account: member, device_id: memberDevice, hpke_seed_hex: hex(memberSeed) },
    recipients,
    recipients_digest_input: context(recipients),
    lease,
    activation,
    rotation_uploads: uploads,
    envelope_contexts: activation.envelopes.map((e) => context(envelopeContext(e.recipient))),
    epoch_key_hex: hex(epochKey),
    key_confirmation_input: context([channel, 1, epochKey]),
    message: {
      sender: owner,
      device_id: meta.deviceId,
      payload: job.payload,
      key_context: context(messageKeyContext),
      key_hex: hex(messageKey),
      aad: context(['dmsg/channel-message/1', owner, meta.deviceId, messageAad]),
      plaintext: typed(decodeCanonical(messagePlaintext)),
      plaintext_hex: hex(messagePlaintext)
    },
    history: {
      scope: history.scope,
      uploads: history.uploads,
      context: context(historyContext),
      plaintext: typed(decodeCanonical(historyPlaintext)),
      plaintext_hex: hex(historyPlaintext)
    },
    attachment: {
      message_id: attachmentId,
      payload: attached.payload,
      aad: context(['dmsg/channel-message/1', owner, meta.deviceId, attachedAad]),
      key_hex: hex(attachedKey),
      plaintext: typed(decodeCanonical(attachedPlaintext)),
      plaintext_hex: hex(attachedPlaintext),
      upload: prepared,
      manifest_context: context(manifestContext),
      manifest_plaintext: typed(decodeCanonical(manifestPlaintext)),
      manifest_plaintext_hex: hex(manifestPlaintext)
    }
  })
})

it('inbox', async () => {
  const sender = xidText(fill(1, 12)),
    recipient = xidText(fill(2, 12))
  const { engine, meta } = await boundEngine(sender)
  const internal = engine as any
  const key = await engine.inboxKey(1)
  const objectId = hash(canonical(['dmsg/inbox-key/1', meta.subjectId, 1]))
  const record = await (await internal.ready()).db.getHead(objectId, 'inbox')
  const keyPayload = await internal.decode(record)
  const order = '15'.repeat(32),
    text = 'A paid letter for the vector'
  const envelope = await engine.inboxSeal(recipient, 1, key.publicKey, order, text)
  const info = ['dmsg/inbox-envelope/1', sender, recipient, order, 1]
  const sealed = decodeCanonical<{ enc: string; ciphertext: string }>(unb64(envelope))
  const plaintext = await hpkeOpen(unb64(keyPayload.seed), sealed, info)
  expect(decodeCanonical(plaintext)).toEqual({ text })
  settle('inbox-v1.json', {
    ...header(
      'Recipient inbox key object and a letter envelope produced by the extension engine. The key object ID is the cross-device lookup key; the envelope is canonical {enc, ciphertext} with base64url text values.'
    ),
    subject_id: meta.subjectId,
    key_version: 1,
    key_object_id: objectId,
    key_object_id_input: context(['dmsg/inbox-key/1', meta.subjectId, 1]),
    key_payload: typed(keyPayload),
    public_key_hex: key.publicKey,
    sender,
    recipient,
    order_id: order,
    envelope,
    envelope_hex: hex(unb64(envelope)),
    context: context(info),
    plaintext_hex: hex(plaintext)
  })
})
