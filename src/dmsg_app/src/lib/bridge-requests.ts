import { browserOperationDigest, hex, unbase64, type BrowserCommand } from '@dmsg/sdk/browser'
import {
  decodeCanonical,
  equalBytes,
  sha256,
  validateAuthentication,
  type AuthenticationRequest
} from '@dmsg/sdk'
import { services } from './services/ic'
import { registeredApplication } from './services/registration'
import { enqueueRequest, requestDatabase } from './requests'
import type { PendingRequest, SourceBinding } from './protocol/requests'
import { config } from './config'
import { hpkeSeal } from './crypto/primitives'
import { canonical } from './protocol/codec'
import { ensure } from './errors'

export interface AuthenticationPayload {
  requestCbor: string
}

export async function validateAuthenticationPayload(
  command: BrowserCommand,
  source: SourceBinding,
  app: import('@dmsg/sdk').AppRegistration,
  now = BigInt(Date.now())
) {
  const request = decodeCanonical(
    unbase64(command.payload!)
  ) as unknown as AuthenticationRequest
  validateAuthentication(request, app, now)
  ensure(
    request.origin === source.origin &&
      request.app_id === command.appId &&
      hex(request.operation_id) === command.operationId &&
      equalBytes(request.session_key_hash, await sha256(unbase64(command.publicKey))),
    'INTEGRITY_FAILED'
  )
  return request
}

/** Called only after verifying the fresh P-256 connection proof. */
export async function createBrowserOperation(command: BrowserCommand, source: SourceBinding) {
  const authentication = command.method === 'authenticate'
  ensure(authentication || command.method === 'signDocument', 'INVALID_INPUT')
  const app = await registeredApplication(
    await services(),
    command.appId,
    source.origin,
    authentication ? 'Authenticate' : 'SignDocument'
  )
  const db = await requestDatabase()
  try {
    const meta = await db.meta()
    ensure(meta?.registered && meta.account?.homeUser === config.canisters.user, 'LOCKED')
    const binding = {
      appId: command.appId,
      publicKey: command.publicKey,
      operationDigest: hex(await browserOperationDigest(command, source.origin)),
      accountId: meta.account.id,
      appVersion: app.config_version.toString()
    }
    if (!authentication) {
      const payload = JSON.parse(
        new TextDecoder('utf-8', { fatal: true }).decode(unbase64(command.payload!))
      )
      ensure(payload.requestId === command.operationId, 'INTEGRITY_FAILED')
      const profile =
        payload.statement?.content?.kind === 'text'
          ? 'TextStatementV1'
          : payload.statement?.content?.kind === 'digest'
            ? 'DigestStatementV1'
            : 'FileStatementV1'
      ensure(app.profiles.includes(profile), 'FORBIDDEN')
      return enqueueRequest(payload, source, binding)
    }
    const request = await validateAuthenticationPayload(command, source, app)
    const payload: AuthenticationPayload = { requestCbor: command.payload! }
    const encrypted = await hpkeSeal(meta.hpkePublic, canonical(payload), [
      'dmsg/external-request/1',
      meta.subjectId,
      meta.deviceId,
      command.operationId
    ])
    const record: PendingRequest = {
      id: command.operationId,
      kind: 'authentication',
      source,
      bridge: binding,
      digest: binding.operationDigest,
      payload: encrypted,
      expiresAt: Number(request.expires_at_ms),
      createdAt: Date.now(),
      state: 'awaiting_user'
    }
    const tx = db.db.transaction('requests', 'readwrite')
    const old = (await tx.store.get(record.id)) as PendingRequest | undefined
    if (old) {
      ensure(
        old.digest === record.digest && old.bridge?.accountId === binding.accountId,
        'IDEMPOTENCY_CONFLICT'
      )
      await tx.store.put({ ...old, source })
      await tx.done
      return { ...old, source }
    }
    const all = (await tx.store.getAll()) as PendingRequest[]
    const pending = all.filter(
      (r) =>
        ['awaiting_user', 'authorized', 'execution_unknown'].includes(r.state) &&
        (r.expiresAt > Date.now() || r.state !== 'awaiting_user')
    )
    ensure(
      all.length < 10000 &&
        pending.length < 20 &&
        pending.filter((r) => r.source.origin === source.origin).length < 5,
      'QUOTA_EXCEEDED'
    )
    await tx.store.add(record)
    await tx.done
    return record
  } finally {
    db.db.close()
  }
}

/** Key possession and same app/origin/account are required to rebind a new document. */
export async function bindBrowserOperation(command: BrowserCommand, source: SourceBinding) {
  const db = await requestDatabase()
  try {
    const meta = await db.meta()
    const tx = db.db.transaction('requests', 'readwrite')
    const record = (await tx.store.get(command.operationId)) as PendingRequest | undefined
    ensure(
      record?.bridge &&
        record.bridge.appId === command.appId &&
        record.bridge.publicKey === command.publicKey &&
        record.source.origin === source.origin &&
        record.bridge.accountId === meta?.account?.id &&
        meta?.registered &&
        meta.account.homeUser === config.canisters.user,
      'FORBIDDEN'
    )
    const next = { ...record, source }
    await tx.store.put(next)
    await tx.done
    return next
  } finally {
    db.db.close()
  }
}
