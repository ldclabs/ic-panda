import { canonical, hash, unb64, unhex, equal } from '../protocol/codec'
import { readCloudCommand, verifyCloudCommand, verifyCloudHttp } from '../protocol/cloud'
import { ensure } from '../errors'
import type { WorkspaceDB } from '../db'
export interface CipherDispatch {
  id: string
  format: 'dmsg-cipher-dispatch/1'
  account: string
  origin: string
  deadline: number
  state: 'queued' | 'sending' | 'unknown' | 'complete' | 'paused' | 'blocked'
  attempts: number
  recordKey: string
  requestId: string
  revision: string
  tombstone: boolean
  command: string
  postProof: string
  statusProof: string
  result?: any
  error?: string
}
export async function flushCipherDispatches(
  db: WorkspaceDB,
  origin: string,
  now = Date.now()
) {
  const meta = await db.meta()
  ensure(meta?.account && origin, 'AUTH_REQUIRED')
  const pending = (await db.db.getAll('meta'))
    .filter((row) => row.id.startsWith('dispatch:'))
    .map((row) => row.value as CipherDispatch)
  for (const job of pending
    .filter((v) => ['queued', 'sending', 'unknown'].includes(v.state))
    .slice(0, 25)) {
    const save = () => db.db.put('meta', { id: `dispatch:${job.id}`, value: job })
    if (job.deadline <= now) {
      job.state = 'paused'
      job.error = 'AUTHORIZATION_EXPIRED'
      await save()
      continue
    }
    try {
      ensure(
        job.format === 'dmsg-cipher-dispatch/1' &&
          job.origin === origin &&
          job.account === meta.account.id &&
          job.deadline <= now + 60000,
        'FORBIDDEN'
      )
      const publicKey = unb64(meta.signingPublic),
        signed = { cose_sign1: job.command },
        parsed = readCloudCommand(signed)
      const body = verifyCloudCommand(
        signed,
        { issuer: meta.account.issuer, deviceId: meta.deviceId },
        publicKey,
        'dmsg/vault/revision/v1'
      )
      ensure(
        body.request_id === job.requestId &&
          body.deadline === job.deadline &&
          body.payload.revision_id === job.revision &&
          body.payload.tombstone === job.tombstone,
        'INTEGRITY_FAILED'
      )
      const base = `/v1/accounts/${job.account}`,
        postBody = canonical(signed)
      const post = verifyCloudHttp(job.postProof, publicKey),
        status = verifyCloudHttp(job.statusProof, publicKey)
      ensure(
        post.body.audience === origin &&
          post.body.target === `${base}/vault` &&
          post.body.method === 'POST' &&
          post.body.body_digest === hash(postBody) &&
          status.body.audience === origin &&
          status.body.target === `${base}/operations/${job.requestId}` &&
          status.body.method === 'GET' &&
          status.body.body_digest === hash(new Uint8Array()) &&
          post.body.deadline === job.deadline &&
          status.body.deadline === job.deadline &&
          post.issuer === meta.account.issuer &&
          status.issuer === meta.account.issuer &&
          equal(post.kid, unhex(meta.deviceId)) &&
          equal(status.kid, unhex(meta.deviceId)) &&
          post.body.request_id === job.requestId &&
          status.body.request_id === job.requestId,
        'FORBIDDEN'
      )
      const request = async (
        path: string,
        method: string,
        proof: string,
        bytes?: Uint8Array
      ) => {
        const response = await fetch(origin + path, {
          method,
          credentials: 'omit',
          cache: 'no-store',
          redirect: 'error',
          signal: AbortSignal.timeout(15000),
          headers: {
            'x-dmsg-pop': proof,
            ...(bytes ? { 'content-type': 'application/cbor' } : {})
          },
          ...(bytes ? { body: new Uint8Array(bytes) } : {})
        })
        ensure(response.body, 'INTEGRITY_FAILED')
        const reader = response.body.getReader(),
          chunks: Uint8Array[] = []
        let length = 0
        try {
          for (;;) {
            const row = await reader.read()
            if (row.done) break
            length += row.value.length
            if (length > 65536) {
              await reader.cancel()
              throw new Error('INTEGRITY_FAILED')
            }
            chunks.push(row.value)
          }
        } finally {
          reader.releaseLock()
        }
        const receivedBytes = new Uint8Array(length)
        let offset = 0
        for (const chunk of chunks) {
          receivedBytes.set(chunk, offset)
          offset += chunk.length
        }
        const value = JSON.parse(
          new TextDecoder('utf-8', { fatal: true }).decode(receivedBytes)
        )
        if (response.status === 404 && value.error?.code === 'NOT_FOUND')
          return { found: false }
        ensure(response.ok && value.ok === true, 'EXECUTION_UNKNOWN')
        return value.data
      }
      const prior = await request(
        `${base}/operations/${job.requestId}`,
        'GET',
        job.statusProof
      )
      let result = prior.result
      if (!prior.found) {
        job.state = 'sending'
        job.attempts++
        await save()
        result = await request(`${base}/vault`, 'POST', job.postProof, postBody)
      }
      ensure(
        result?.revision_id === job.revision &&
          result.tombstone === job.tombstone &&
          typeof result.conflict === 'boolean' &&
          /^[0-9a-f]{64}$/.test(result.head),
        'INTEGRITY_FAILED'
      )
      job.result = result
      job.state = 'complete'
      delete job.error
      await save()
    } catch (error) {
      job.state =
        error instanceof Error && ['FORBIDDEN', 'INTEGRITY_FAILED'].includes(error.message)
          ? 'blocked'
          : 'unknown'
      job.error = 'DISPATCH_REQUIRES_RECONCILIATION'
      await save()
    }
  }
}
