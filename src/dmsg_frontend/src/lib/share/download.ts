import { ShareError, sha256Hex, type Descriptor } from './decrypt'

type Slot = { url: string; size: number; digest: string }
type Grant = {
  upload_id: string
  expires_at: number
  session_expires_at: number
  manifest: Slot
  chunks: Slot[]
}

class RetryableDownloadError extends Error {
  constructor(readonly status?: number) {
    super(status ? `File download: ${status}` : 'File download unavailable')
  }
}

async function read(slot: Slot): Promise<Uint8Array> {
  const response = await fetch(slot.url, {
    credentials: 'omit',
    redirect: 'error',
    referrerPolicy: 'no-referrer',
    cache: 'no-store',
    signal: AbortSignal.timeout(30_000)
  }).catch(() => {
    // Network errors may include the bearer URL; keep it out of UI errors.
    throw new RetryableDownloadError()
  })
  if (response.status !== 200) {
    await response.body?.cancel().catch(() => {})
    if ([403, 404, 429].includes(response.status) || response.status >= 500)
      throw new RetryableDownloadError(response.status)
    throw new Error(`File download: ${response.status}`)
  }
  const length = response.headers.get('content-length')
  if (length !== null && Number(length) !== slot.size) {
    await response.body?.cancel().catch(() => {})
    throw new ShareError('INTEGRITY_FAILED', 'chunk size')
  }
  const reader = response.body?.getReader()
  if (!reader) throw new ShareError('INTEGRITY_FAILED', 'missing body')
  const bytes = new Uint8Array(slot.size)
  let at = 0
  try {
    for (;;) {
      const { done, value } = await reader.read().catch(() => {
        throw new RetryableDownloadError()
      })
      if (done) break
      if (at + value.length > bytes.length) {
        await reader.cancel().catch(() => {})
        throw new ShareError('INTEGRITY_FAILED', 'chunk size')
      }
      bytes.set(value, at)
      at += value.length
    }
  } finally {
    reader.releaseLock()
  }
  if (at !== bytes.length || (await sha256Hex(bytes)) !== slot.digest)
    throw new ShareError('INTEGRITY_FAILED', 'chunk digest')
  return bytes
}

/** A page-local download session. Retries reuse its random ID; URLs never persist. */
export class ShareDownload {
  private readonly id = Array.from(
    crypto.getRandomValues(new Uint8Array(32)),
    (n) => n.toString(16).padStart(2, '0')
  ).join('')
  private started = false
  private attemptedAt: number | undefined
  private grant?: Grant
  private pending: Promise<Grant> | undefined

  constructor(
    private readonly api: string,
    private readonly descriptor: Descriptor
  ) {}

  private authorize(force = false): Promise<Grant> {
    if (!force && this.grant && this.grant.expires_at > Date.now() + 1000)
      return Promise.resolve(this.grant)
    if (this.pending) return this.pending
    this.pending = this.request().finally(() => {
      this.pending = undefined
    })
    return this.pending
  }

  private async request(): Promise<Grant> {
    this.attemptedAt ??= Date.now()
    if (!this.started && Date.now() >= this.attemptedAt + 3_600_000)
      throw new Error(
        'Download session expired; reload to begin a new download'
      )
    const response = await fetch(
      `${this.api}/public/shares/${this.descriptor.share_id}/downloads/${this.id}`,
      {
        method: this.started ? 'GET' : 'POST',
        credentials: 'omit',
        redirect: 'error',
        referrerPolicy: 'no-referrer',
        cache: 'no-store',
        signal: AbortSignal.timeout(30_000)
      }
    )
    if (response.status === 410) {
      const body = await response.json()
      const reason = body.error?.message
      if (!['expired', 'revoked', 'exhausted'].includes(reason))
        throw new Error(
          'Download session expired; reload to begin a new download'
        )
      throw Object.assign(new Error('Download is no longer available'), {
        gone: reason
      })
    }
    if (!response.ok)
      throw new Error(`Download authorization: ${response.status}`)
    const grant: Grant = (await response.json()).data
    if (
      !grant ||
      grant.upload_id !== this.descriptor.upload_id ||
      !Number.isSafeInteger(grant.expires_at) ||
      !Number.isSafeInteger(grant.session_expires_at) ||
      grant.expires_at > grant.session_expires_at ||
      grant.expires_at > this.descriptor.expires_at ||
      grant.chunks?.length !== this.descriptor.chunks.length ||
      grant.chunks.length > 128
    )
      throw new ShareError('INTEGRITY_FAILED', 'download grant')
    const slots = [grant.manifest, ...grant.chunks]
    const expected = [this.descriptor.manifest, ...this.descriptor.chunks]
    for (let i = 0; i < slots.length; i++) {
      const slot = slots[i]
      const expectedSlot = expected[i]
      if (
        !slot ||
        !expectedSlot ||
        !Number.isSafeInteger(slot.size) ||
        slot.size < 0 ||
        slot.digest !== expectedSlot.digest ||
        slot.size !== expectedSlot.size ||
        slot.size > 1024 * 1024 + 64
      )
        throw new ShareError('INTEGRITY_FAILED', 'download slot')
      const url = new URL(slot.url)
      const s3 =
        url.protocol === 'https:' &&
        !url.port &&
        /^[0-9a-f]{32}(?:\.eu|\.fedramp)?\.r2\.cloudflarestorage\.com$/.test(
          url.hostname
        )
      const api = new URL(this.api)
      const local =
        api.protocol === 'http:' &&
        ['localhost', '127.0.0.1', '[::1]'].includes(api.hostname) &&
        url.origin === api.origin &&
        url.pathname.startsWith('/__r2/')
      if (url.username || url.password || url.hash || !(s3 || local))
        throw new ShareError('INTEGRITY_FAILED', 'download URL')
    }
    this.started = true
    this.grant = grant
    return grant
  }

  async bytes(path: string): Promise<Uint8Array> {
    for (let attempt = 0; attempt < 2; attempt++) {
      const grant = await this.authorize(attempt > 0)
      const slot =
        path === 'manifest'
          ? grant.manifest
          : /^chunks\/\d+$/.test(path)
            ? grant.chunks[Number(path.slice(7))]
            : undefined
      if (!slot) throw new ShareError('INTEGRITY_FAILED', 'download slot')
      try {
        return await read(slot)
      } catch (error) {
        if (error instanceof RetryableDownloadError) {
          if (attempt === 0) continue
          if (error.status === 404)
            throw new ShareError('INTEGRITY_FAILED', 'missing committed chunk')
        }
        throw error
      }
    }
    throw new Error('File download unavailable')
  }
}
