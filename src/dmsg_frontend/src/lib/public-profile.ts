import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import {
  idlFactory as handleIdl,
  type _SERVICE as HandleService,
  type CertifiedBatch as CandidBatch
} from '$declarations/dmsg_handle/dmsg_handle.did.js'
import {
  idlFactory as userIdl,
  type _SERVICE as UserService,
  type Device
} from '$declarations/dmsg_user/dmsg_user.did.js'
import {
  accountBytes,
  accountHome,
  profileTarget,
  verifyHandle,
  verifyProfileSecurity,
  verifyPublicProfile,
  type ProfileDevice,
  type VerifiedProfile
} from 'dmsg-sdk/profile'
import { DmsgError } from 'dmsg-sdk/errors'
import type { CertifiedBatch } from 'dmsg-sdk/ic'
import { API_ORIGIN, DMSG_HANDLE_CANISTER_ID, IS_LOCAL } from './constants'

export class ProfileError extends Error {
  constructor(
    readonly reason:
      | 'invalid_target'
      | 'not_found'
      | 'legacy_reserved'
      | 'unavailable'
      | 'verification_failed'
  ) {
    super(reason)
  }
}

export interface ProfilePage {
  account: string
  handle?: string | undefined
  published: VerifiedProfile | null
  avatar: string | null
}

function batchView(batch: CandidBatch): CertifiedBatch {
  return {
    schema: BigInt(batch.schema),
    canister: batch.canister.toUint8Array(),
    certificate: Uint8Array.from(batch.certificate),
    entries: batch.entries.map((e) => ({
      key: Uint8Array.from(e.key),
      value: e.value.length ? Uint8Array.from(e.value[0]!) : null,
      witness: Uint8Array.from(e.witness)
    }))
  }
}

function deviceView(d: Device): ProfileDevice {
  return {
    input: {
      device_id: Uint8Array.from(d.input.device_id),
      signing_pub: Uint8Array.from(d.input.signing_pub),
      hpke_pub: Uint8Array.from(d.input.hpke_pub),
      role: Object.keys(d.input.role)[0]!,
      capabilities: d.input.capabilities.map((c) => Object.keys(c)[0]!)
    },
    added_at: d.added_at,
    added_by: d.added_by[0] ? Uint8Array.from(d.added_by[0]) : null,
    revoked_at: d.revoked_at[0] ?? null,
    next_sequence: d.next_sequence
  }
}

function ok<T>(answer: { Ok: T } | { Err: unknown }): T {
  if ('Ok' in answer) return answer.Ok
  throw new ProfileError('unavailable')
}

// Remember only verified heads for this browser session; a relay cannot roll
// back a profile already displayed here or replace a version with other bytes.
const seen = new Map<string, { version: number; hash: string }>()

async function publicRecord(
  account: string,
  signal: AbortSignal
): Promise<unknown | null> {
  const response = await fetch(
    `${API_ORIGIN}/public/accounts/${account}/profile`,
    {
      signal,
      credentials: 'omit',
      redirect: 'error',
      cache: 'no-store',
      headers: { Accept: 'application/json' }
    }
  )
  if (response.status === 404) return null
  if (!response.ok || !response.body) throw new ProfileError('unavailable')
  const reader = response.body.getReader()
  const chunks: Uint8Array[] = []
  let size = 0
  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) break
      size += value.length
      if (size > 131072) throw new ProfileError('verification_failed')
      chunks.push(value)
    }
  } finally {
    await reader.cancel()
    reader.releaseLock()
  }
  const bytes = new Uint8Array(size)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.length
  }
  try {
    const body = JSON.parse(
      new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    )
    if (body.ok !== true || !body.data)
      throw new Error('Invalid profile response')
    return body.data
  } catch {
    throw new ProfileError('verification_failed')
  }
}

/** Anonymous reads only. No legacy auth, daemon, or response-supplied URLs. */
export async function readProfile(
  input: string,
  abort: AbortSignal
): Promise<ProfilePage> {
  let target: ReturnType<typeof profileTarget>
  try {
    target = profileTarget(input)
  } catch {
    throw new ProfileError('invalid_target')
  }
  const signal = AbortSignal.any([abort, AbortSignal.timeout(20000)])
  try {
    const agent = await HttpAgent.create({
      host: IS_LOCAL ? 'http://localhost:8000' : 'https://icp-api.io',
      shouldFetchRootKey: IS_LOCAL,
      verifyQuerySignatures: true,
      fetch: (url, init) => fetch(url, { ...init, signal, credentials: 'omit' })
    })
    const root = agent.rootKey
    if (!root) throw new ProfileError('unavailable')
    const rootKey = Uint8Array.from(root)
    const registry = Principal.fromText(DMSG_HANDLE_CANISTER_ID)
    const handle = Actor.createActor<HandleService>(handleIdl, {
      agent,
      canisterId: registry
    })
    const config = await handle.get_handle_config()
    const name = 'handle' in target ? target.handle : undefined
    const account =
      'account' in target
        ? target.account
        : await verifyHandle(
            batchView(
              ok(await handle.resolve_handle_certified([target.handle]))
            ),
            target.handle,
            registry.toUint8Array(),
            rootKey,
            BigInt(Date.now())
          )
    if (!account) {
      // Only after certified absence: a frozen legacy name is reserved, not
      // an active mapping to a new account. Never infer identity from it.
      const reserved = name && ok(await handle.get_legacy_reservation(name))[0]
      throw new ProfileError(reserved ? 'legacy_reserved' : 'not_found')
    }
    const home = accountHome(
      account,
      Object.keys(config.environment)[0]!,
      config.issuer_namespace,
      config.user_homes.map((h) => h.toUint8Array())
    )
    if (!home) throw new ProfileError('not_found')
    const user = Actor.createActor<UserService>(userIdl, {
      agent,
      canisterId: Principal.fromUint8Array(home)
    })
    const id = accountBytes(account)
    const batch = batchView(ok(await user.security_snapshot_batch([id])))
    const entries = batch.entries[0]?.value
      ? ok(await user.get_device_bundle(id))[1]
      : []
    const devices = new Map(
      entries.map(([key, d]) => [Uint8Array.from(key), deviceView(d)])
    )
    const security = await verifyProfileSecurity(
      batch,
      account,
      home,
      rootKey,
      devices,
      BigInt(Date.now())
    )
    if (!security) throw new ProfileError('not_found')
    const record = await publicRecord(account, signal)
    let published: VerifiedProfile | null
    try {
      published = record === null ? null : verifyPublicProfile(record, security)
    } catch {
      throw new ProfileError('verification_failed')
    }
    signal.throwIfAborted()
    const known = seen.get(account)
    if (
      known &&
      (!published ||
        published.profile.version < known.version ||
        (published.profile.version === known.version &&
          published.hash !== known.hash))
    )
      throw new ProfileError('verification_failed')
    if (published)
      seen.set(account, {
        version: published.profile.version,
        hash: published.hash
      })
    const upload = published?.profile.avatar_upload
    return {
      account,
      handle: name,
      published,
      avatar: upload
        ? `${API_ORIGIN}/public/accounts/${account}/avatar/${upload}`
        : null
    }
  } catch (error) {
    if (error instanceof ProfileError) throw error
    if (error instanceof DmsgError)
      throw new ProfileError('verification_failed')
    throw new ProfileError('unavailable')
  }
}
