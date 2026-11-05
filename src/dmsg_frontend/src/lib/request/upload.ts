// One file from a browser to a request link, in two passes. The relay needs
// every chunk's digest at the reservation, and a phone can't hold the
// ciphertext of a large file: the first pass seals each chunk only for its
// digest, the second seals it again (same key, nonce and AAD give the same
// bytes) and sends it. At most a few chunks are in memory at a time.

import { sha256 } from '@noble/hashes/sha2.js'
import { bytesHex, sha256Hex } from '$lib/share/decrypt'
import { RequestApiError, type RequestApi, type UploadPlan } from './api'
import {
  base64url,
  CHUNK,
  randomId,
  REQUEST_FILE,
  REQUEST_FILE_FORMAT,
  sealChunk,
  sealManifest,
  sealReceipt,
  type Receipt,
  type RequestFile,
  type RequestSender
} from './seal'

const PARALLEL = 3
/** How long a chunk keeps retrying through a broken connection. */
const RETRY_MS = 10 * 60_000
const FIRST_RETRY_MS = 1_000
const LAST_RETRY_MS = 30_000
/** A slow connection may take hours; a tab left behind should not hold the link for long. */
const PLAN_MS = 6 * 3_600_000

export type UploadLink = {
  account: string
  requestId: string
  hpkePub: Uint8Array
  receiptKey: Uint8Array
}
export type UploadInput = {
  file: Blob & { name: string }
  sender: RequestSender | null
  replaces: string | null
}
export type UploadProgress = {
  stage: 'preparing' | 'uploading'
  done: number
  total: number
}
export type Sent = { uploadId: string; receipt: Receipt }
/** Why a file stopped. */
export class UploadStop extends Error {
  constructor(readonly reason: 'empty' | 'changed' | 'stopped') {
    super(reason)
    this.name = 'UploadStop'
  }
}

type Hooks = {
  progress(progress: UploadProgress): void
  /** Keep the ID before reserving, so a lost or cancelled reply can still be withdrawn. */
  reserved?(uploadId: string): void
  signal?: AbortSignal
  now?: () => number
  /** How to wait between retries; a test passes its own. */
  wait?: (ms: number, signal?: AbortSignal) => Promise<void>
}

const pause = (ms: number, signal?: AbortSignal) =>
  new Promise<void>((resolve, reject) => {
    if (signal?.aborted) return reject(new UploadStop('stopped'))
    const stop = () => {
      clearTimeout(timer)
      reject(new UploadStop('stopped'))
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', stop)
      resolve()
    }, ms)
    signal?.addEventListener('abort', stop, { once: true })
  })

const isAbort = (error: unknown) =>
  error instanceof DOMException && error.name === 'AbortError'

function plainOf(file: Blob, index: number): Promise<ArrayBuffer> {
  return file.slice(index * CHUNK, (index + 1) * CHUNK).arrayBuffer()
}

/** Keep trying what a network or a busy relay refused; a refusal that means it stays. */
async function retrying<T>(
  attempt: () => Promise<T>,
  hooks: Hooks,
  now: () => number
): Promise<T> {
  const wait = hooks.wait ?? pause
  const giveUp = now() + RETRY_MS
  let delay = FIRST_RETRY_MS
  for (;;) {
    if (hooks.signal?.aborted) throw new UploadStop('stopped')
    try {
      return await attempt()
    } catch (error) {
      if (hooks.signal?.aborted || isAbort(error))
        throw new UploadStop('stopped')
      const again =
        error instanceof TypeError ||
        (error instanceof RequestApiError && error.retryable)
      if (!again || now() >= giveUp) throw error
    }
    await wait(delay, hooks.signal)
    delay = Math.min(delay * 2, LAST_RETRY_MS)
  }
}

/** Seal and send one file once; the caller decides whether to start over. */
async function once(
  api: RequestApi,
  link: UploadLink,
  input: UploadInput,
  hooks: Hooks
): Promise<Sent> {
  const { file } = input
  const now = hooks.now ?? Date.now
  const controller = new AbortController()
  const stop = () => controller.abort()
  const callerSignal = hooks.signal
  if (callerSignal?.aborted) stop()
  else callerSignal?.addEventListener('abort', stop, { once: true })
  hooks = { ...hooks, signal: controller.signal }
  let workers: Promise<void>[] = []
  const key = crypto.getRandomValues(new Uint8Array(32))
  try {
    const [uploadId, id, version] = [randomId(), randomId(), randomId()]
    const count = Math.ceil(file.size / CHUNK)
    const whole = sha256.create()
    const chunks: { digest: string; sealed: number; size: number }[] = []
    hooks.progress({ stage: 'preparing', done: 0, total: count })
    for (let index = 0; index < count; index++) {
      if (hooks.signal?.aborted) throw new UploadStop('stopped')
      const plain = new Uint8Array(await plainOf(file, index))
      whole.update(plain)
      const sealed = await sealChunk(key, id, version, index, count, plain)
      chunks.push({
        digest: await sha256Hex(sealed),
        sealed: sealed.length,
        size: plain.length
      })
      hooks.progress({ stage: 'preparing', done: index + 1, total: count })
    }
    const sent_at = now()
    const mime = file.type || 'application/octet-stream'
    const fileSha = bytesHex(whole.digest())
    const manifestPlain: RequestFile = {
      format: REQUEST_FILE_FORMAT,
      request_id: link.requestId,
      upload_id: uploadId,
      file: {
        id,
        version,
        name: file.name,
        mime,
        size: file.size,
        sha256: fileSha,
        chunks: chunks.map((chunk, index) => ({
          id: `${id}:${version}:${index}`,
          digest: chunk.digest,
          size: chunk.size
        })),
        key: base64url(key)
      },
      sender: input.sender,
      replaces: input.replaces,
      sent_at
    }
    const manifest = await sealManifest(
      link.hpkePub,
      link.account,
      manifestPlain
    )
    const plan: UploadPlan = {
      upload_id: uploadId,
      object_id: id,
      version_id: version,
      root_generation: 0,
      epoch: 0,
      control_head: null,
      kind: REQUEST_FILE,
      chunks: chunks.map(({ digest, sealed }) => ({ digest, size: sealed })),
      manifest_digest: await sha256Hex(manifest),
      manifest_size: manifest.length,
      expires_at: now() + PLAN_MS
    }
    const receipt: Receipt = {
      name: file.name,
      size: file.size,
      mime,
      sha256: fileSha,
      sent_at,
      replaces: input.replaces
    }
    const sealedReceipt = await sealReceipt(
      link.receiptKey,
      link.requestId,
      uploadId,
      receipt
    )
    hooks.reserved?.(uploadId)
    await retrying(
      () => api.reserve(plan, sealedReceipt, hooks.signal),
      hooks,
      now
    )

    let next = 0
    let done = 0
    hooks.progress({ stage: 'uploading', done: 0, total: count })
    const worker = async () => {
      while (next < count) {
        const index = next++
        const plain = new Uint8Array(await plainOf(file, index))
        const sealed = await sealChunk(key, id, version, index, count, plain)
        // A file changed since the first pass seals to other bytes.
        if ((await sha256Hex(sealed)) !== chunks[index]!.digest)
          throw new UploadStop('changed')
        await retrying(
          () => api.put(uploadId, index, sealed, hooks.signal),
          hooks,
          now
        )
        done++
        hooks.progress({ stage: 'uploading', done, total: count })
      }
    }
    workers = Array.from({ length: PARALLEL }, worker)
    await Promise.all(workers)
    await retrying(
      () => api.put(uploadId, 'manifest', manifest, hooks.signal),
      hooks,
      now
    )
    await retrying(() => api.finalize(uploadId, hooks.signal), hooks, now)
    return { uploadId, receipt }
  } finally {
    controller.abort()
    await Promise.allSettled(workers)
    callerSignal?.removeEventListener('abort', stop)
    key.fill(0)
  }
}

/**
 * Seal and send a file. A reservation that was stopped meanwhile starts over
 * once with a new key and upload.
 */
export async function uploadFile(
  api: RequestApi,
  link: UploadLink,
  input: UploadInput,
  hooks: Hooks
): Promise<Sent> {
  if (input.file.size === 0) throw new UploadStop('empty')
  try {
    return await once(api, link, input, hooks)
  } catch (error) {
    const lapsed =
      error instanceof RequestApiError && error.code === 'RESULT_EXPIRED'
    if (!lapsed) throw error
    return once(api, link, input, hooks)
  }
}
