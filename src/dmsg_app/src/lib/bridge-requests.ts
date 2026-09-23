import { xidBytes } from './protocol/identity'
import { browserOperationDigest, hex, unbase64, type BrowserCommand } from '@dmsg/sdk/browser'
import {
  decodeCanonical,
  equalBytes,
  sha256,
  validateAuthentication,
  validateActionAdmission,
  validateShape,
  type AppAction,
  type AuthenticationRequest
} from '@dmsg/sdk'
import { services } from './services/ic'
import { registeredApplication } from './services/registration'
import { enqueueRequest, requestDatabase } from './requests'
import { parseRequest, type PendingRequest, type SourceBinding } from './protocol/requests'
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
  ensure(
    authentication || ['signDocument', 'signAction', 'checkout'].includes(command.method),
    'INVALID_INPUT'
  )
  const app = await registeredApplication(
    await services(),
    command.appId,
    source.origin,
    authentication
      ? 'Authenticate'
      : command.method === 'checkout'
        ? 'Checkout'
        : command.method === 'signAction'
          ? 'SignAction'
          : 'SignDocument'
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
    if (command.method === 'signDocument') {
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
    const action =
      command.method === 'signAction'
        ? (decodeCanonical(unbase64(command.payload!)) as unknown as AppAction)
        : null
    if (action) {
      validateActionAdmission(action, app, BigInt(Date.now()))
      ensure(equalBytes(action.signing_account, xidBytes(meta.account.id)), 'ACCOUNT_MISMATCH')
      ensure(
        action.origin === source.origin &&
          action.app_id === command.appId &&
          hex(action.operation_id) === command.operationId,
        'INTEGRITY_FAILED'
      )
    }
    const checkout =
      command.method === 'checkout'
        ? (decodeCanonical(
            unbase64(command.payload!)
          ) as unknown as import('@dmsg/sdk').CheckoutRequest)
        : null
    if (checkout) {
      validateShape('CheckoutRequest', checkout)
      ensure(
        equalBytes(checkout.approving_account, xidBytes(meta.account.id)),
        'ACCOUNT_MISMATCH'
      )
      ensure(
        checkout.offer.app_id === command.appId &&
          hex(checkout.offer.operation_id) === command.operationId &&
          checkout.offer.accept_by_ms > BigInt(Date.now()) &&
          app.product_ids.includes(checkout.offer.product_id),
        'INTEGRITY_FAILED'
      )
      ensure(checkout.offer.allowed_settlement_methods.includes(checkout.method), 'FORBIDDEN')
    }
    const auth =
      action || checkout ? null : await validateAuthenticationPayload(command, source, app)
    const payload = checkout
      ? { checkoutCbor: command.payload! }
      : action
        ? {
            protocol: 'dmsg-extension/4',
            method: 'signature.request',
            requestId: command.operationId,
            accountId: meta.account.id,
            nonce: command.operationId,
            expiresAt: action.expires_at_ms.toString(),
            statement: {
              issuer: meta.account.issuer,
              content: { kind: 'app_action', actionCbor: command.payload! }
            }
          }
        : { requestCbor: command.payload! }
    const actionDigest = action ? parseRequest(payload, source, Date.now(), true).digest : null
    const encrypted = await hpkeSeal(meta.hpkePublic, canonical(payload), [
      'dmsg/external-request/1',
      meta.subjectId,
      meta.deviceId,
      command.operationId
    ])
    const record: PendingRequest = {
      id: command.operationId,
      kind: checkout ? 'checkout' : action ? 'action' : 'authentication',
      source,
      bridge: binding,
      digest: actionDigest ?? binding.operationDigest,
      payload: encrypted,
      expiresAt: Number(
        action?.expires_at_ms ?? checkout?.offer.accept_by_ms ?? auth!.expires_at_ms
      ),
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
