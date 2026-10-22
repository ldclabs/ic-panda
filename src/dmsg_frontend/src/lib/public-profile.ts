import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import {
  idlFactory as handleIdl,
  type _SERVICE as HandleService,
  type CertifiedBatch as CandidBatch
} from '$declarations/dmsg_handle/dmsg_handle.did.js'
import {
  idlFactory as userIdl,
  type _SERVICE as UserService
} from '$declarations/dmsg_user/dmsg_user.did.js'
import {
  accountBytes,
  accountHome,
  profileTarget,
  verifyHandle,
  verifyPrincipalDocument,
  type PrincipalDocument
} from 'dmsg-sdk/principal'
import {
  idlFactory as directoryIdl,
  type _SERVICE as DirectoryService
} from '$declarations/dmsg_directory/dmsg_directory.did.js'
import { DmsgError } from 'dmsg-sdk/errors'
import { verifyCertifiedPath, type CertifiedBatch } from 'dmsg-sdk/ic'
import { decodeCanonical, equal } from 'dmsg-sdk/codec'
import { DMSG_HANDLE_CANISTER_ID, IS_LOCAL } from './constants'

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
  document: PrincipalDocument | null
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

function ok<T>(answer: { Ok: T } | { Err: unknown }): T {
  if ('Ok' in answer) return answer.Ok
  throw new ProfileError('unavailable')
}

// A session never accepts an older directory document after seeing a newer one.
const seen = new Map<string, { updatedAt: number; document: string }>()

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
    const routeProof =
      'handle' in target
        ? batchView(ok(await handle.resolve_handle_certified([target.handle])))
        : undefined
    const account =
      'account' in target
        ? target.account
        : await verifyHandle(
            routeProof!,
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
    const userConfig = await user.user_config()
    const directory = Actor.createActor<DirectoryService>(directoryIdl, {
      agent,
      canisterId: userConfig.directory_canister
    })
    const directoryConfig = await directory.directory_config()
    if (
      directoryConfig.principal_origin !== userConfig.principal_origin ||
      Object.keys(directoryConfig.environment)[0] !==
        Object.keys(config.environment)[0] ||
      directoryConfig.issuer_namespace !== config.issuer_namespace ||
      !directoryConfig.user_homes.some((h) => equal(h.toUint8Array(), home))
    )
      throw new ProfileError('verification_failed')
    const proof = batchView(
      ok(await directory.principal_document(accountBytes(account)))
    )
    let document: PrincipalDocument | null
    try {
      document = await verifyPrincipalDocument(
        proof,
        account,
        userConfig.directory_canister.toUint8Array(),
        userConfig.principal_origin,
        rootKey,
        BigInt(Date.now())
      )
    } catch {
      throw new ProfileError('verification_failed')
    }
    if (!document && !seen.has(account)) {
      const id = accountBytes(account)
      const proof = batchView(ok(await user.security_snapshot_batch([id])))
      let exists: boolean
      try {
        const found = await verifyCertifiedPath(
          proof,
          id,
          [id],
          home,
          rootKey,
          BigInt(Date.now())
        )
        exists = found.value !== null
        if (found.value) {
          const snapshot = decodeCanonical<{
            schema: number
            account_id: Uint8Array
            home_user: Uint8Array
          }>(found.value)
          if (
            snapshot.schema !== 4 ||
            !equal(snapshot.account_id, id) ||
            !equal(snapshot.home_user, home)
          )
            throw new Error('Account binding')
        }
      } catch {
        throw new ProfileError('verification_failed')
      }
      if (!exists) throw new ProfileError('not_found')
    }
    // Recheck routing freshness after the directory request has returned.
    if (routeProof)
      await verifyHandle(
        routeProof,
        name!,
        registry.toUint8Array(),
        rootKey,
        BigInt(Date.now())
      )
    signal.throwIfAborted()
    const known = seen.get(account)
    const encoded = JSON.stringify(document)
    if (
      known &&
      (!document ||
        document.updated_at < known.updatedAt ||
        (document.updated_at === known.updatedAt && encoded !== known.document))
    )
      throw new ProfileError('verification_failed')
    if (document)
      seen.set(account, { updatedAt: document.updated_at, document: encoded })
    return { account, handle: name, document }
  } catch (error) {
    if (error instanceof ProfileError) throw error
    if (error instanceof DmsgError)
      throw new ProfileError('verification_failed')
    throw new ProfileError('unavailable')
  }
}
