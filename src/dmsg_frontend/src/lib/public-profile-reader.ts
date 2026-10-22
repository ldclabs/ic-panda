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
  verifyHandleRecord,
  verifyPrincipalDocument
} from 'dmsg-sdk/principal'
import {
  idlFactory as directoryIdl,
  type _SERVICE as DirectoryService
} from '$declarations/dmsg_directory/dmsg_directory.did.js'
import { DmsgError } from 'dmsg-sdk/errors'
import { verifyCertifiedPath, type CertifiedBatch } from 'dmsg-sdk/ic'
import { decodeCanonical, equal } from 'dmsg-sdk/codec'
import {
  DMSG_HANDLE_CANISTER_ID,
  DMSG_PROFILE_HINT,
  IS_LOCAL
} from './constants'

import {
  ProfileError,
  type ProfilePage,
  type ProfileReadOptions
} from './public-profile-types'

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

// In-memory only: query-verified configuration and independently certified proofs.
// Cached proofs are reverified against the current clock on every read.
const CACHE_MS = 30_000
class FreshCache<T> {
  private entries = new Map<string, { value: T; until: number }>()
  get(key: string): T | undefined {
    const entry = this.entries.get(key)
    if (entry && Date.now() < entry.until) return entry.value
    this.entries.delete(key)
    return undefined
  }
  put(key: string, value: T) {
    if (this.entries.get(key)?.value === value) return
    this.entries.delete(key)
    if (this.entries.size >= 64)
      this.entries.delete(this.entries.keys().next().value!)
    this.entries.set(key, { value, until: Date.now() + CACHE_MS })
  }
  delete(key: string) {
    this.entries.delete(key)
  }
}

type RegistryConfig = Awaited<ReturnType<HandleService['get_handle_config']>>
type UserConfig = Awaited<ReturnType<UserService['user_config']>>
type DirectoryConfig = Awaited<ReturnType<DirectoryService['directory_config']>>
type ProfileHint = { home: string; directory: string }

/** A hint can only save a request; a malformed one must not fail every read. */
function usableHint(hint: ProfileHint | null): ProfileHint | null {
  if (!hint) return null
  try {
    Principal.fromText(hint.home)
    Principal.fromText(hint.directory)
    return hint
  } catch {
    return null
  }
}

/** Each reader owns its short-lived cache and session rollback protection. */
export function createProfileReader(
  input: ProfileHint | null = IS_LOCAL ? null : DMSG_PROFILE_HINT
) {
  const hint = usableHint(input)
  const seen = new Map<string, { updatedAt: number; document: string }>()
  const registryConfigs = new FreshCache<RegistryConfig>()
  const userConfigs = new FreshCache<UserConfig>()
  const directoryConfigs = new FreshCache<DirectoryConfig>()
  const routes = new FreshCache<CertifiedBatch>()
  const documents = new FreshCache<CertifiedBatch>()

  /** Anonymous reads only; hints can start requests, never establish trust. */
  return async function readProfile(
    input: string,
    abort: AbortSignal,
    { refresh = false }: ProfileReadOptions = {}
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
        fetch: (url, init) =>
          fetch(url, { ...init, signal, credentials: 'omit' })
      })
      const root = agent.rootKey
      if (!root) throw new ProfileError('unavailable')
      const rootKey = Uint8Array.from(root)
      const registry = Principal.fromText(DMSG_HANDLE_CANISTER_ID)
      const handle = Actor.createActor<HandleService>(handleIdl, {
        agent,
        canisterId: registry
      })
      // Share a request within this read; cancellation never poisons another read.
      function query<T>(
        pending: Map<string, Promise<T>>,
        cache: FreshCache<T> | null,
        key: string,
        load: () => Promise<T>
      ): Promise<T> {
        const cached = !refresh && cache?.get(key)
        if (cached) return Promise.resolve(cached)
        let request = pending.get(key)
        if (!request) {
          request = Promise.resolve()
            .then(load)
            .then((value) => {
              signal.throwIfAborted()
              cache?.put(key, value)
              return value
            })
          // Speculative requests may be irrelevant after checking the registry.
          void request.catch(() => {})
          pending.set(key, request)
        }
        return request
      }
      const registryRequests = new Map<string, Promise<RegistryConfig>>()
      const userRequests = new Map<string, Promise<UserConfig>>()
      const directoryRequests = new Map<string, Promise<DirectoryConfig>>()
      const documentRequests = new Map<string, Promise<CertifiedBatch>>()
      const userActor = (id: Principal) =>
        Actor.createActor<UserService>(userIdl, { agent, canisterId: id })
      const directoryActor = (id: Principal) =>
        Actor.createActor<DirectoryService>(directoryIdl, {
          agent,
          canisterId: id
        })
      const userConfigFor = (id: Principal) =>
        query(userRequests, userConfigs, id.toText(), () =>
          userActor(id).user_config()
        )
      const directoryConfigFor = (id: Principal) =>
        query(directoryRequests, directoryConfigs, id.toText(), () =>
          directoryActor(id).directory_config()
        )
      const documentFor = (id: Principal, account: string) =>
        query(documentRequests, null, `${id}:${account}`, async () =>
          batchView(
            ok(
              await directoryActor(id).principal_document(accountBytes(account))
            )
          )
        )

      async function readProof<T>(
        cache: FreshCache<CertifiedBatch>,
        key: string,
        load: () => Promise<CertifiedBatch>,
        verify: (proof: CertifiedBatch) => Promise<T>
      ) {
        const cached = !refresh && cache.get(key)
        if (cached) {
          try {
            return { proof: cached, value: await verify(cached) }
          } catch (error) {
            cache.delete(key)
            // A previously verified certificate can expire before the cache TTL.
            if (!(error instanceof DmsgError) || error.code !== 'EXPIRED')
              throw error
          }
        }
        const proof = await load()
        return { proof, value: await verify(proof) }
      }

      const configRequest = query(
        registryRequests,
        registryConfigs,
        'registry',
        () => handle.get_handle_config()
      )
      const name = 'handle' in target ? target.handle : undefined
      const routeRequest = name
        ? readProof(
            routes,
            name,
            async () =>
              batchView(ok(await handle.resolve_handle_certified([name]))),
            (proof) =>
              verifyHandleRecord(
                proof,
                name,
                registry.toUint8Array(),
                rootKey,
                BigInt(Date.now())
              )
          )
        : Promise.resolve(null)
      if (hint) {
        // Current deployment hints collapse the cold path to one round for IDs,
        // two for handles. Changed homes/directories fall back to verified config.
        void userConfigFor(Principal.fromText(hint.home)).catch(() => {})
        const hintedDirectory = Principal.fromText(hint.directory)
        void directoryConfigFor(hintedDirectory).catch(() => {})
        void routeRequest
          .then((route) => {
            const account =
              'account' in target ? target.account : route?.value?.account
            if (
              account &&
              (refresh || !documents.get(`${hintedDirectory}:${account}`))
            )
              return documentFor(hintedDirectory, account)
            return undefined
          })
          .catch(() => {})
      }
      const [config, route] = await Promise.all([configRequest, routeRequest])
      const routeProof = route?.proof
      const resolved = route?.value
      const account = 'account' in target ? target.account : resolved?.account
      if (!account) {
        // Only after certified absence: a frozen legacy name is reserved, not
        // an active mapping to a new account. Never infer identity from it.
        const reserved =
          name && ok(await handle.get_legacy_reservation(name))[0]
        throw new ProfileError(reserved ? 'legacy_reserved' : 'not_found')
      }
      const home = accountHome(
        account,
        Object.keys(config.environment)[0]!,
        config.issuer_namespace,
        config.user_homes.map((h) => h.toUint8Array())
      )
      if (!home) throw new ProfileError('not_found')
      const user = userActor(Principal.fromUint8Array(home))
      const userConfig = await userConfigFor(Principal.fromUint8Array(home))
      const documentKey = `${userConfig.directory_canister}:${account}`
      // Config and document queries do not depend on each other's responses.
      const directoryRequest = directoryConfigFor(userConfig.directory_canister)
      const [directoryConfig, verified] = await Promise.all([
        directoryRequest,
        readProof(
          documents,
          documentKey,
          () => documentFor(userConfig.directory_canister, account),
          async (proof) => {
            await directoryRequest
            try {
              return await verifyPrincipalDocument(
                proof,
                account,
                userConfig.directory_canister.toUint8Array(),
                userConfig.principal_origin,
                rootKey,
                BigInt(Date.now())
              )
            } catch (error) {
              if (error instanceof DmsgError) throw error
              throw new ProfileError('verification_failed')
            }
          }
        )
      ])
      if (
        directoryConfig.principal_origin !== userConfig.principal_origin ||
        Object.keys(directoryConfig.environment)[0] !==
          Object.keys(config.environment)[0] ||
        directoryConfig.issuer_namespace !== config.issuer_namespace ||
        !directoryConfig.user_homes.some((h) => equal(h.toUint8Array(), home))
      )
        throw new ProfileError('verification_failed')
      const document = verified.value
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
        await verifyHandleRecord(
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
          (document.updated_at === known.updatedAt &&
            encoded !== known.document))
      )
        throw new ProfileError('verification_failed')
      if (document)
        seen.set(account, { updatedAt: document.updated_at, document: encoded })
      // Never cache absence or failed verification, and never extend a proof's
      // original 60-second freshness window when serving it from memory.
      if (document) documents.put(documentKey, verified.proof)
      if (routeProof && resolved) routes.put(name!, routeProof)
      // New registrations carry certified display casing. Older records fall
      // back to the URL spelling; routing always uses the lowercase name.
      return {
        account,
        handle: resolved?.displayHandle ?? (name ? input : undefined),
        document
      }
    } catch (error) {
      if (error instanceof ProfileError) throw error
      if (error instanceof DmsgError)
        throw new ProfileError('verification_failed')
      throw new ProfileError('unavailable')
    }
  }
}
