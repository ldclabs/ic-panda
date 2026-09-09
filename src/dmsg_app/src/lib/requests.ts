import { currentWorkspace, WorkspaceDB } from './db'
import { config, isExtension } from './config'
import { ensure } from './errors'
import { canonical } from './protocol/codec'
import { hpkeSeal } from './crypto/primitives'
import {
  parseRequest,
  sameSource,
  type PendingRequest,
  type SourceBinding
} from './protocol/requests'

export async function requestDatabase() {
  const name = await currentWorkspace()
  ensure(name, 'LOCKED', '请先在扩展中建立工作台。')
  return WorkspaceDB.open(name)
}
export async function enqueueRequest(input: unknown, source: SourceBinding) {
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
      ensure(old.digest === digest && sameSource(old.source, source), 'IDEMPOTENCY_CONFLICT')
      await tx.done
      return old
    }
    const now = Date.now(),
      records = (await tx.store.getAll()) as PendingRequest[]
    for (const previous of records) {
      if (previous.state === 'awaiting_user' && previous.expiresAt <= now) {
        previous.state = 'expired'
        await tx.store.put(previous)
      }
    }
    const terminal = records
      .filter((r) => r.state !== 'awaiting_user')
      .sort((a, b) => b.createdAt - a.createdAt)
    for (const previous of terminal.slice(5000)) await tx.store.delete(previous.id)
    const pending = records.filter(
      (r) => r.state === 'awaiting_user' && r.expiresAt > Date.now()
    )
    ensure(
      pending.length < 20 &&
        pending.filter((r) => r.source.origin === source.origin).length < 5,
      'QUOTA_EXCEEDED',
      '待确认请求过多。'
    )
    ensure(
      records.length - Math.max(0, terminal.length - 5000) < 10000,
      'QUOTA_EXCEEDED',
      '请求历史容量已满。'
    )
    await tx.store.add(record)
    await tx.done
    return record
  } finally {
    db.db.close()
  }
}
export async function listRequests(): Promise<PendingRequest[]> {
  if (!(await currentWorkspace())) return []
  const db = await requestDatabase()
  try {
    return ((await db.db.getAll('requests')) as PendingRequest[])
      .map((r) =>
        r.state === 'awaiting_user' && r.expiresAt <= Date.now()
          ? { ...r, state: 'expired' as const }
          : r
      )
      .sort((a, b) => b.createdAt - a.createdAt)
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
export const externalEnabled = () => config.externalOrigins.length > 0
