import { currentWorkspace, WorkspaceDB } from './db'
import { config, isExtension } from './config'
import { ensure } from './errors'
import { canonical } from './protocol/codec'
import { hpkeSeal } from './crypto/primitives'
import {
  parseRequest,
  sameSource,
  type PendingRequest,
  type BrowserBinding,
  type SourceBinding
} from './protocol/requests'

export async function requestDatabase() {
  const name = await currentWorkspace()
  ensure(name, 'LOCKED', '请先在扩展中建立工作台。')
  return WorkspaceDB.open(name)
}
export async function enqueueRequest(
  input: unknown,
  source: SourceBinding,
  bridge: BrowserBinding
) {
  const { request, digest, expiresAt } = parseRequest(input, source),
    db = await requestDatabase()
  try {
    const meta = await db.meta()
    ensure(
      meta &&
        meta.registered &&
        meta.account?.id === request.accountId &&
        meta.account.issuer === request.statement.issuer &&
        meta.account.homeUser === config.canisters.user,
      'FORBIDDEN',
      '请先绑定与请求签署者一致的链上账户。'
    )
    const payload = await hpkeSeal(meta.hpkePublic, canonical(request), [
      'dmsg/external-request/1',
      meta.subjectId,
      meta.deviceId,
      request.requestId
    ])
    const record: PendingRequest = {
      id: request.requestId,
      kind: 'document',
      bridge,
      source,
      digest,
      expiresAt,
      payload,
      createdAt: Date.now(),
      state: 'awaiting_user'
    }
    const tx = db.db.transaction('requests', 'readwrite'),
      old = (await tx.store.get(record.id)) as PendingRequest | undefined
    if (old) {
      ensure(
        old.digest === digest &&
          old.bridge?.operationDigest === bridge.operationDigest &&
          old.bridge.accountId === bridge.accountId &&
          old.source.origin === source.origin,
        'IDEMPOTENCY_CONFLICT'
      )
      await tx.store.put({ ...old, source })
      await tx.done
      return { ...old, source }
    }
    await admitRequest(tx.store, (await tx.store.getAll()) as PendingRequest[], source)
    await tx.store.add(record)
    await tx.done
    return record
  } finally {
    db.db.close()
  }
}
/** Shared admission: expire stale prompts, bound terminal history and unanswered prompts. */
export async function admitRequest(
  store: {
    put(value: PendingRequest): Promise<unknown>
    delete(id: string): Promise<void>
  },
  records: PendingRequest[],
  source: SourceBinding
) {
  const now = Date.now()
  for (const previous of records) {
    if (previous.state === 'awaiting_user' && previous.expiresAt <= now) {
      previous.state = 'expired'
      await store.put(previous)
    }
  }
  const terminal = records
    .filter((r) => !['awaiting_user', 'authorized', 'execution_unknown'].includes(r.state))
    .sort((a, b) => b.createdAt - a.createdAt)
  for (const previous of terminal.slice(5000)) await store.delete(previous.id)
  const pending = records.filter((r) => r.state === 'awaiting_user' && r.expiresAt > now)
  ensure(
    pending.length < 20 && pending.filter((r) => r.source.origin === source.origin).length < 5,
    'QUOTA_EXCEEDED',
    '待确认请求过多。'
  )
  ensure(
    records.length - Math.max(0, terminal.length - 5000) < 10000,
    'QUOTA_EXCEEDED',
    '请求历史容量已满。'
  )
}
const observed = (request: PendingRequest, now: number): PendingRequest =>
  request.state === 'awaiting_user' && request.expiresAt <= now
    ? { ...request, state: 'expired' }
    : request
export async function listRequests(): Promise<PendingRequest[]> {
  if (!(await currentWorkspace())) return []
  const db = await requestDatabase()
  try {
    const now = Date.now()
    return ((await db.db.getAll('requests')) as PendingRequest[])
      .map((r) => observed(r, now))
      .sort((a, b) => b.createdAt - a.createdAt)
  } finally {
    db.db.close()
  }
}
export async function getRequest(id: string): Promise<PendingRequest | null> {
  const db = await requestDatabase()
  try {
    const request = (await db.db.get('requests', id)) as PendingRequest | undefined
    return request ? observed(request, Date.now()) : null
  } finally {
    db.db.close()
  }
}
/** Stores expiry for unanswered prompts and returns how many remain open. */
export async function expireRequests() {
  if (!(await currentWorkspace())) return 0
  const db = await requestDatabase()
  try {
    const tx = db.db.transaction('requests', 'readwrite'),
      now = Date.now()
    let open = 0
    for (const request of (await tx.store.getAll()) as PendingRequest[]) {
      if (request.state !== 'awaiting_user') continue
      if (request.expiresAt > now) open++
      else await tx.store.put({ ...request, state: 'expired' })
    }
    await tx.done
    return open
  } finally {
    db.db.close()
  }
}
export async function rejectRequest(
  id: string,
  reason: 'rejected' | 'cancelled' | 'expired' = 'rejected'
) {
  const db = await requestDatabase()
  try {
    const tx = db.db.transaction('requests', 'readwrite'),
      record = (await tx.store.get(id)) as PendingRequest | undefined
    ensure(record, 'NOT_FOUND')
    if (record.state === 'awaiting_user') await tx.store.put({ ...record, state: reason })
    await tx.done
  } finally {
    db.db.close()
  }
}
export function openApproval(id: string) {
  ensure(/^[0-9a-f]{64}$/.test(id), 'INVALID_INPUT')
  if (isExtension())
    return chrome.windows.create({
      url: chrome.runtime.getURL(`approve.html?id=${id}`),
      type: 'popup',
      width: 480,
      height: 760
    })
  window.open(`approve.html?id=${id}`, 'dmsg-approval', 'width=480,height=760')
}
export async function setRequestState(
  id: string,
  state: PendingRequest['state'],
  errorCode?: string
) {
  const db = await requestDatabase()
  try {
    const tx = db.db.transaction('requests', 'readwrite'),
      request = (await tx.store.get(id)) as PendingRequest | undefined
    ensure(request, 'NOT_FOUND')
    if (request.state === 'returned' && state === 'signed') state = 'returned'
    if (
      ['cancelled', 'rejected', 'expired', 'failed', 'result_expired'].includes(request.state)
    )
      ensure(state === request.state, 'FORBIDDEN')
    if (['signed', 'returned'].includes(request.state))
      ensure(['signed', 'returned', 'result_expired'].includes(state), 'FORBIDDEN')
    if (state === 'authorized')
      ensure(
        ['awaiting_user', 'authorized', 'execution_unknown'].includes(request.state),
        'FORBIDDEN'
      )
    await tx.store.put({ ...request, state, ...(errorCode ? { errorCode } : {}) })
    await tx.done
  } finally {
    db.db.close()
  }
}

export async function acknowledgeRequest(
  id: string,
  source: SourceBinding,
  digest: unknown,
  deliveredDigest: string | undefined
) {
  ensure(
    typeof digest === 'string' && /^[0-9a-f]{64}$/.test(digest) && deliveredDigest === digest,
    'FORBIDDEN'
  )
  const db = await requestDatabase()
  try {
    const tx = db.db.transaction('requests', 'readwrite')
    const request = (await tx.store.get(id)) as PendingRequest | undefined
    ensure(
      request &&
        sameSource(request.source, source) &&
        ['signed', 'returned'].includes(request.state),
      'FORBIDDEN'
    )
    await tx.store.put({ ...request, state: 'returned' })
    await tx.done
  } finally {
    db.db.close()
  }
}
export async function assertLiveSource(request: PendingRequest) {
  ensure(isExtension(), 'FORBIDDEN')
  const result = await chrome.runtime.sendMessage({
    type: 'dmsg-source-check',
    requestId: request.id
  })
  ensure(result?.ok === true, 'FORBIDDEN', '原请求页面已关闭、导航或断开，请重新发起。')
}
