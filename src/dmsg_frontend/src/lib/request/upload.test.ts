// @vitest-environment node
// WebCrypto needs buffers of its own realm; jsdom's would be refused.
import { Aes256Gcm, CipherSuite, HkdfSha256 } from '@hpke/core'
import { DhkemX25519HkdfSha256 } from '@hpke/dhkem-x25519'
import { decode } from 'cborg'
import { describe, expect, it } from 'vitest'
import { hexBytes, open, sha256Hex } from '$lib/share/decrypt'
import { chunkAad } from '$lib/share/decrypt'
import { RequestApi, RequestApiError, type UploadPlan } from './api'
import { readRecords } from './records'
import { fileInfo, receiptKey, sealReceipt, type RequestFile } from './seal'
import { uploadFile, UploadStop, type UploadLink } from './upload'

const account = 'aaaaa-aa'
const requestId = 'ab'.repeat(32)
const seed = hexBytes('11'.repeat(32))
const suite = new CipherSuite({
  kem: new DhkemX25519HkdfSha256(),
  kdf: new HkdfSha256(),
  aead: new Aes256Gcm()
})
const pair = await suite.kem.deriveKeyPair(seed.slice().buffer)
const hpkePub = new Uint8Array(
  await suite.kem.serializePublicKey(pair.publicKey)
)
const pageKey = new Uint8Array(32).fill(2)
const link: UploadLink = {
  account,
  requestId,
  hpkePub,
  receiptKey: receiptKey(pageKey, account, requestId)
}

function named(
  bytes: Uint8Array,
  name = 'report.pdf',
  type = 'application/pdf'
) {
  const blob = new Blob([bytes as BlobPart], { type })
  return Object.assign(blob, { name })
}

const reply = (data: unknown, status = 200) =>
  new Response(JSON.stringify({ ok: status < 400, data }), { status })
const failure = (
  status: number,
  code: string,
  message = code,
  retryable = false
) =>
  new Response(
    JSON.stringify({ ok: false, error: { code, message, retryable } }),
    { status }
  )

/** A relay that keeps what it is given and checks digests as the real one does. */
class Relay {
  plans: UploadPlan[] = []
  receipts: string[] = []
  stored = new Map<string, Uint8Array>()
  finalized: string[] = []
  fail: (path: string, attempt: number) => Response | Error | null = () => null
  attempts = new Map<string, number>()
  active = 0
  most = 0

  send: typeof fetch = async (input, init) => {
    const path = String(input).replace('https://relay.test', '')
    const seen = (this.attempts.get(path) ?? 0) + 1
    this.attempts.set(path, seen)
    const planned = this.fail(path, seen)
    if (planned instanceof Error) throw planned
    if (planned) return planned
    this.active++
    this.most = Math.max(this.most, this.active)
    await new Promise((resolve) => setTimeout(resolve, 0))
    this.active--
    if (path === '/uploads') {
      const { plan, receipt } = JSON.parse(String(init?.body))
      this.plans.push(plan)
      this.receipts.push(receipt)
      return reply({ upload_id: plan.upload_id, status: 'staging' })
    }
    const put = /^\/uploads\/(\w+)\/chunks\/(\w+)$/.exec(path)
    if (put) {
      const plan = this.plans.find((p) => p.upload_id === put[1])!
      const bytes = new Uint8Array(init?.body as ArrayBuffer)
      const digest =
        put[2] === 'manifest'
          ? plan.manifest_digest
          : plan.chunks[Number(put[2])]!.digest
      if ((await sha256Hex(bytes)) !== digest)
        return failure(422, 'INTEGRITY_FAILED')
      this.stored.set(`${put[1]}/${put[2]}`, bytes)
      return reply({ stored: true })
    }
    const finalize = /^\/uploads\/(\w+)\/finalize$/.exec(path)
    if (finalize) {
      this.finalized.push(finalize[1]!)
      return reply({ upload_id: finalize[1], status: 'committed' })
    }
    return failure(404, 'NOT_FOUND')
  }
  api() {
    return new RequestApi('https://relay.test', 'token', this.send)
  }
}

const hooks = () => ({ progress: () => {}, wait: async () => {} })

async function opened(relay: Relay, plan: UploadPlan): Promise<RequestFile> {
  const sealed = relay.stored.get(`${plan.upload_id}/manifest`)!
  const info = fileInfo(
    account,
    requestId,
    plan.upload_id,
    plan.object_id,
    plan.version_id
  )
  const plain = await suite.open(
    {
      recipientKey: pair.privateKey,
      enc: sealed.slice(0, 32).buffer,
      info: info.slice().buffer
    },
    sealed.slice(32).buffer,
    info.slice().buffer
  )
  return decode(new Uint8Array(plain)) as RequestFile
}

describe('sending a file through a request link', () => {
  it('sends chunks that open with the key only the manifest holds', async () => {
    const relay = new Relay()
    const bytes = new Uint8Array(1024 * 1024 + 7).map((_, i) => i * 31)
    const seen: string[] = []
    const { uploadId: id, receipt } = await uploadFile(
      relay.api(),
      link,
      {
        file: named(bytes),
        sender: { name: 'Ada', note: 'Q3' },
        replaces: 'cd'.repeat(32)
      },
      {
        ...hooks(),
        progress: (p) => seen.push(`${p.stage}:${p.done}/${p.total}`)
      }
    )
    expect(relay.finalized).toEqual([id])
    expect(receipt).toMatchObject({
      name: 'report.pdf',
      size: bytes.length,
      sha256: await sha256Hex(bytes),
      replaces: 'cd'.repeat(32)
    })
    expect(seen[0]).toBe('preparing:0/2')
    expect(seen).toContain('preparing:2/2')
    expect(seen.at(-1)).toBe('uploading:2/2')
    const plan = relay.plans[0]!
    expect(plan.kind).toBe('request_file')
    expect(plan.root_generation).toBe(0)
    const file = await opened(relay, plan)
    expect(file.upload_id).toBe(id)
    expect(file.sender).toEqual({ name: 'Ada', note: 'Q3' })
    expect(file.replaces).toBe('cd'.repeat(32))
    expect(file.file.size).toBe(bytes.length)
    expect(file.file.sha256).toBe(await sha256Hex(bytes))
    const key = Uint8Array.from(
      atob(file.file.key.replace(/-/g, '+').replace(/_/g, '/').padEnd(44, '=')),
      (c) => c.charCodeAt(0)
    )
    const parts = await Promise.all(
      file.file.chunks.map((chunk, index) =>
        open(
          key,
          relay.stored.get(`${id}/${index}`)!,
          chunkAad(file.file.id, file.file.version, index, 2, chunk.size)
        )
      )
    )
    expect(
      new Uint8Array(await new Blob(parts as BlobPart[]).arrayBuffer())
    ).toEqual(bytes)
    // The relay reads only what the plan says: no name, no size of the plaintext.
    expect(JSON.stringify(plan)).not.toContain('report.pdf')
  })

  it('never holds more than a few chunks at once', async () => {
    const relay = new Relay()
    await uploadFile(
      relay.api(),
      link,
      {
        file: named(new Uint8Array(1024 * 1024 * 7 + 1)),
        sender: null,
        replaces: null
      },
      hooks()
    )
    expect(relay.plans[0]!.chunks).toHaveLength(8)
    expect(relay.most).toBeLessThanOrEqual(3)
  })

  it('refuses an empty file before anything leaves', async () => {
    const relay = new Relay()
    await expect(
      uploadFile(
        relay.api(),
        link,
        { file: named(new Uint8Array()), sender: null, replaces: null },
        hooks()
      )
    ).rejects.toMatchObject({ reason: 'empty' })
    expect(relay.plans).toEqual([])
  })

  it('rides out a broken connection and a busy relay', async () => {
    const relay = new Relay()
    relay.fail = (path, attempt) => {
      if (path.endsWith('/chunks/0') && attempt === 1)
        return new TypeError('network')
      if (path.endsWith('/chunks/manifest') && attempt === 1)
        return failure(429, 'SYNC_BUSY', 'busy', true)
      if (path.endsWith('/finalize') && attempt === 1)
        return failure(503, 'UNAVAILABLE', 'later', true)
      return null
    }
    const { uploadId: id } = await uploadFile(
      relay.api(),
      link,
      { file: named(new Uint8Array(9)), sender: null, replaces: null },
      hooks()
    )
    expect(relay.finalized).toEqual([id])
  })

  it('gives up on what a relay refuses for good', async () => {
    const relay = new Relay()
    relay.fail = (path) =>
      path === '/uploads' ? failure(429, 'QUOTA_EXCEEDED', 'full') : null
    await expect(
      uploadFile(
        relay.api(),
        link,
        { file: named(new Uint8Array(9)), sender: null, replaces: null },
        hooks()
      )
    ).rejects.toMatchObject({ code: 'QUOTA_EXCEEDED', message: 'full' })
    // Not even once more: the reason does not change by asking again.
    expect(relay.attempts.get('/uploads')).toBeLessThanOrEqual(1)
  })

  it('starts over once with a new key when a reservation lapses', async () => {
    const relay = new Relay()
    let lapsed = 0
    relay.fail = (path) =>
      path.endsWith('/chunks/0') && lapsed++ === 0
        ? failure(410, 'RESULT_EXPIRED')
        : null
    const { uploadId: id } = await uploadFile(
      relay.api(),
      link,
      { file: named(new Uint8Array(9)), sender: null, replaces: null },
      hooks()
    )
    expect(relay.plans).toHaveLength(2)
    expect(relay.plans[0]!.upload_id).not.toBe(relay.plans[1]!.upload_id)
    expect(relay.plans[0]!.chunks[0]!.digest).not.toBe(
      relay.plans[1]!.chunks[0]!.digest
    )
    expect(relay.finalized).toEqual([id])
    relay.fail = (path) =>
      path.includes('/chunks/') ? failure(410, 'RESULT_EXPIRED') : null
    await expect(
      uploadFile(
        relay.api(),
        link,
        { file: named(new Uint8Array(9)), sender: null, replaces: null },
        hooks()
      )
    ).rejects.toBeInstanceOf(RequestApiError)
  })

  it('stops when a file changes under it, and when asked', async () => {
    const relay = new Relay()
    let reads = 0
    const shifting = {
      name: 'moving.txt',
      type: 'text/plain',
      size: 4,
      slice: () => new Blob([new Uint8Array(4).fill(++reads)])
    } as unknown as Blob & { name: string }
    await expect(
      uploadFile(
        relay.api(),
        link,
        { file: shifting, sender: null, replaces: null },
        hooks()
      )
    ).rejects.toMatchObject({ reason: 'changed' })
    expect(relay.stored.size).toBe(0)
    const abort = new AbortController()
    abort.abort()
    await expect(
      uploadFile(
        relay.api(),
        link,
        { file: named(new Uint8Array(9)), sender: null, replaces: null },
        { ...hooks(), signal: abort.signal }
      )
    ).rejects.toBeInstanceOf(UploadStop)
  })

  it('aborts an in-flight request when stopped and keeps its reservation ID', async () => {
    for (const phase of ['reserve', 'chunk', 'manifest', 'finalize']) {
      const relay = new Relay()
      const abort = new AbortController()
      const blocked = Promise.withResolvers<void>()
      const released = Promise.withResolvers<Response>()
      let signal: AbortSignal | null | undefined
      let reserved: string | undefined
      const api = new RequestApi(
        'https://relay.test',
        'token',
        async (input, init) => {
          const path = String(input)
          if (
            (phase === 'reserve' && path.endsWith('/uploads')) ||
            (phase === 'chunk' && path.endsWith('/chunks/0')) ||
            (phase === 'manifest' && path.endsWith('/chunks/manifest')) ||
            (phase === 'finalize' && path.endsWith('/finalize'))
          ) {
            signal = init?.signal
            signal?.addEventListener(
              'abort',
              () => released.reject(new DOMException('Stopped', 'AbortError')),
              { once: true }
            )
            blocked.resolve()
            return released.promise
          }
          return relay.send(input, init)
        }
      )
      const outcome = uploadFile(
        api,
        link,
        {
          file: named(new Uint8Array(9)),
          sender: null,
          replaces: null
        },
        {
          ...hooks(),
          signal: abort.signal,
          reserved: (id) => {
            reserved = id
          }
        }
      ).catch((error) => error)
      await blocked.promise
      abort.abort()
      const interrupted = signal?.aborted
      // Release a broken implementation too, so the regression fails without
      // leaving a hung fetch or background upload in the test runner.
      released.reject(new DOMException('Stopped', 'AbortError'))
      expect(await outcome).toBeInstanceOf(UploadStop)
      expect(interrupted, phase).toBe(true)
      expect(reserved, phase).toMatch(/^[0-9a-f]{64}$/)
    }
  })

  it('stops sibling chunk requests before a failed upload returns', async () => {
    const relay = new Relay()
    const first = Promise.withResolvers<Response>()
    const sibling = Promise.withResolvers<Response>()
    const started = Promise.withResolvers<void>()
    let signal: AbortSignal | null | undefined
    const api = new RequestApi(
      'https://relay.test',
      'token',
      async (input, init) => {
        const path = String(input)
        if (path.endsWith('/chunks/0')) return first.promise
        if (path.endsWith('/chunks/1')) {
          signal = init?.signal
          signal?.addEventListener(
            'abort',
            () => sibling.reject(new DOMException('Stopped', 'AbortError')),
            { once: true }
          )
          started.resolve()
          return sibling.promise
        }
        return relay.send(input, init)
      }
    )
    const outcome = uploadFile(
      api,
      link,
      {
        file: named(new Uint8Array(1024 * 1024 + 1)),
        sender: null,
        replaces: null
      },
      hooks()
    ).catch((error) => error)
    await started.promise
    first.resolve(failure(403, 'FORBIDDEN'))
    const error = await outcome
    const interrupted = signal?.aborted
    sibling.reject(new DOMException('Stopped', 'AbortError'))
    expect(error).toMatchObject({ code: 'FORBIDDEN' })
    expect(interrupted).toBe(true)
  })
})

describe('what this browser sent', () => {
  const key = receiptKey(pageKey, account, requestId)
  const receipt = (replaces: string | null, name = 'a.pdf') => ({
    name,
    size: 3,
    mime: 'application/pdf',
    sha256: '00'.repeat(32),
    sent_at: 1,
    replaces
  })

  it('tells a replaced file from one withdrawn and survives a bad receipt', async () => {
    const [old, next, gone, bad] = ['11', '22', '33', '44'].map((b) =>
      b.repeat(32)
    ) as [string, string, string, string]
    const records = await readRecords(
      [
        {
          upload_id: old,
          status: 'withdrawn',
          stored_at: 1,
          bytes: 9,
          receipt: await sealReceipt(key, requestId, old, receipt(null))
        },
        {
          upload_id: next,
          status: 'delivered',
          stored_at: 4,
          bytes: 9,
          receipt: await sealReceipt(
            key,
            requestId,
            next,
            receipt(old, 'b.pdf')
          )
        },
        {
          upload_id: gone,
          status: 'withdrawn',
          stored_at: 2,
          bytes: 9,
          receipt: await sealReceipt(key, requestId, gone, receipt(null))
        },
        {
          upload_id: bad,
          status: 'saved',
          stored_at: 3,
          bytes: 9,
          receipt: 'AAAA'
        }
      ],
      key,
      requestId
    )
    expect(records.map((r) => [r.uploadId, r.state, r.receipt?.name])).toEqual([
      [next, 'delivered', 'b.pdf'],
      [bad, 'saved', undefined],
      [gone, 'withdrawn', 'a.pdf'],
      [old, 'replaced', 'a.pdf']
    ])
  })
})
