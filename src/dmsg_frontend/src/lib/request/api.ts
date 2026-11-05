// The relay's public file request routes. The token shows the whole link was
// opened; the relay's cookie, limited to this link, recognizes this browser on
// a return visit. No route gives bytes back.

import { API_ORIGIN } from '$lib/constants'

export type LinkStatus = 'open' | 'full' | 'paused' | 'expired' | 'revoked'
export type LinkView = {
  status: LinkStatus
  descriptor?: string
  expires_at?: number
  max_file_bytes?: number | null
}
export type MineEntry = {
  upload_id: string
  status: 'uploading' | 'delivered' | 'saved' | 'withdrawn' | 'removed'
  stored_at: number
  bytes: number
  receipt: string | null
}
export type Mine = { persistent: boolean; entries: MineEntry[] }
export type UploadPlan = {
  upload_id: string
  object_id: string
  version_id: string
  root_generation: 0
  epoch: 0
  control_head: null
  kind: string
  chunks: { digest: string; size: number }[]
  manifest_digest: string
  manifest_size: number
  expires_at: number
}

export class RequestApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly retryable: boolean
  ) {
    super(message)
    this.name = 'RequestApiError'
  }
}

export class RequestApi {
  constructor(
    private readonly base: string,
    private readonly token: string | null,
    private readonly send: typeof fetch = (...args) => fetch(...args)
  ) {}

  static forLink(account: string, requestId: string, token: string | null) {
    return new RequestApi(
      `${API_ORIGIN}/public/requests/${account}/${requestId}`,
      token
    )
  }

  private async call<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers: Record<string, string> = {}
    if (this.token) headers['X-Dmsg-Request-Token'] = this.token
    if (typeof init.body === 'string')
      headers['Content-Type'] = 'application/json'
    const response = await this.send(`${this.base}${path}`, {
      ...init,
      headers,
      credentials: 'include',
      referrerPolicy: 'no-referrer',
      cache: 'no-store'
    })
    const body = (await response.json().catch(() => null)) as {
      data?: T
      error?: { code: string; message: string; retryable: boolean }
    } | null
    if (response.ok && body && 'data' in body) return body.data as T
    throw new RequestApiError(
      response.status,
      body?.error?.code ?? 'UNAVAILABLE',
      body?.error?.message ?? `${response.status}`,
      // A limit in front of the relay answers without a body of ours.
      body?.error?.retryable ??
        (response.status >= 500 || response.status === 429)
    )
  }

  status(): Promise<LinkView> {
    return this.call('')
  }
  mine(): Promise<Mine> {
    return this.call('/mine')
  }
  reserve(plan: UploadPlan, receipt: string) {
    return this.call<{ upload_id: string; status: string }>('/uploads', {
      method: 'POST',
      body: JSON.stringify({ plan, receipt })
    })
  }
  put(upload: string, chunk: number | 'manifest', bytes: Uint8Array) {
    return this.call(`/uploads/${upload}/chunks/${chunk}`, {
      method: 'PUT',
      body: bytes as BodyInit
    })
  }
  finalize(upload: string) {
    return this.call(`/uploads/${upload}/finalize`, { method: 'POST' })
  }
  withdraw(upload: string) {
    return this.call(`/uploads/${upload}/withdraw`, { method: 'POST' })
  }
  forget() {
    return this.call('/forget', { method: 'POST' })
  }
}
