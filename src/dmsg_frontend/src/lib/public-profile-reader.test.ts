// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import { createProfileReader } from './public-profile-reader'
import { DMSG_HANDLE_CANISTER_ID } from './constants'
import { handlePath, accountText } from 'dmsg-sdk/principal'
import { canonical } from 'dmsg-sdk/codec'
import type { CertifiedBatch } from 'dmsg-sdk/ic'
const fixtureUrl = new URL(
  '../../../../packages/dmsg-sdk/tests/principal-fixture.ts',
  import.meta.url
)
const f = await import(/* @vite-ignore */ fixtureUrl.href)
vi.mock('@icp-sdk/core/agent', async (original) => ({
  ...(await original<typeof import('@icp-sdk/core/agent')>()),
  Actor: { createActor: vi.fn() },
  HttpAgent: { create: vi.fn() }
}))
const batch = (b: CertifiedBatch) => ({
  ...b,
  schema: Number(b.schema),
  canister: Principal.fromUint8Array(b.canister),
  entries: b.entries.map((e) => ({
    ...e,
    value: e.value === null ? [] : [e.value]
  }))
})
const registry = {
  get_handle_config: vi.fn(),
  resolve_handle_certified: vi.fn(),
  get_legacy_reservation: vi.fn()
}
const user = { user_config: vi.fn(), security_snapshot_batch: vi.fn() }
const directory = { directory_config: vi.fn(), principal_document: vi.fn() }
let readProfile: ReturnType<typeof createProfileReader>
const read = (input = f.account, refresh = true) =>
  readProfile(input, new AbortController().signal, { refresh })
beforeEach(async () => {
  readProfile = createProfileReader(null)
  vi.spyOn(Date, 'now').mockReturnValue(Number(f.now))
  vi.mocked(HttpAgent.create).mockResolvedValue({ rootKey: f.rootKey } as never)
  vi.mocked(Actor.createActor).mockImplementation(
    (_idl, { canisterId }) =>
      (String(canisterId) === Principal.fromUint8Array(f.home).toText()
        ? user
        : String(canisterId) === Principal.fromUint8Array(f.directory).toText()
          ? directory
          : registry) as never
  )
  registry.get_handle_config.mockResolvedValue({
    environment: { Local: null },
    issuer_namespace: f.namespace,
    user_homes: [Principal.fromUint8Array(f.home)]
  })
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: batch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode(f.handle),
        canonical({ handle: f.handle, version: 1, owner_account: f.accountId }),
        handlePath(f.handle)
      )
    )
  })
  registry.get_legacy_reservation.mockResolvedValue({ Ok: [] })
  user.user_config.mockResolvedValue({
    directory_canister: Principal.fromUint8Array(f.directory),
    principal_origin: f.principalOrigin
  })
  directory.directory_config.mockResolvedValue({
    principal_origin: f.principalOrigin,
    environment: { Local: null },
    issuer_namespace: f.namespace,
    user_homes: [Principal.fromUint8Array(f.home)]
  })
  directory.principal_document.mockResolvedValue({
    Ok: batch(await f.documentBatch())
  })
  vi.stubGlobal('fetch', vi.fn())
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})
it('account and handle render the exact same certified PrincipalDocument, without relay profile reads', async () => {
  expect((await read()).document).toEqual(f.document)
  expect(registry.resolve_handle_certified).not.toHaveBeenCalled()
  expect(await read('ALICE')).toMatchObject({
    account: f.account,
    handle: 'ALICE',
    document: f.document
  })
  expect(registry.resolve_handle_certified).toHaveBeenCalledWith(['alice'])
  expect(fetch).not.toHaveBeenCalled()
})
it('requires the directory config to match the registry and home', async () => {
  directory.directory_config.mockResolvedValueOnce({
    principal_origin: f.principalOrigin,
    environment: { Production: null },
    issuer_namespace: f.namespace,
    user_homes: [Principal.fromUint8Array(f.home)]
  })
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
  directory.directory_config.mockResolvedValueOnce({
    principal_origin: f.principalOrigin,
    environment: { Local: null },
    issuer_namespace: f.namespace,
    user_homes: []
  })
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
})

it('uses certified registered casing even when the URL has another spelling', async () => {
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: batch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode(f.handle),
        canonical({
          handle: f.handle,
          display_handle: 'Alice',
          version: 1,
          owner_account: f.accountId
        }),
        handlePath(f.handle)
      )
    )
  })
  expect((await read('ALICE')).handle).toBe('Alice')
})

it('invalid IDs make no requests', async () => {
  await expect(read('a'.repeat(20))).rejects.toMatchObject({
    reason: 'invalid_target'
  })
  expect(Actor.createActor).not.toHaveBeenCalled()
})
it('reserved names remain separate from principal identities', async () => {
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: batch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode('alice'),
        null
      )
    )
  })
  await expect(read('alice')).rejects.toMatchObject({ reason: 'not_found' })
  registry.get_legacy_reservation.mockResolvedValue({
    Ok: [{ handle: 'alice' }]
  })
  await expect(read('alice')).rejects.toMatchObject({
    reason: 'legacy_reserved'
  })
  expect(directory.principal_document).not.toHaveBeenCalled()
})
it('rejects forged document bytes and network failures', async () => {
  const proof = await f.documentBatch()
  proof.entries[0].value[4] ^= 1
  directory.principal_document.mockResolvedValueOnce({ Ok: batch(proof) })
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
  directory.principal_document.mockRejectedValueOnce(new TypeError('offline'))
  await expect(read()).rejects.toMatchObject({ reason: 'unavailable' })
})
it('distinguishes a real unpublished account from an unallocated ID', async () => {
  const id = f.accountId.slice()
  id[11] = 99
  const account = accountText(id)
  directory.principal_document.mockResolvedValue({
    Ok: batch(await f.certified(f.directory, id, null))
  })
  user.security_snapshot_batch.mockResolvedValue({
    Ok: batch(
      await f.certified(
        f.home,
        id,
        canonical({ schema: 4, account_id: id, home_user: f.home })
      )
    )
  })
  expect(await read(account)).toMatchObject({ account, document: null })
  user.security_snapshot_batch.mockResolvedValue({
    Ok: batch(await f.certified(f.home, id, null))
  })
  await expect(read(account)).rejects.toMatchObject({ reason: 'not_found' })
})

it('refuses rollback after seeing a newer certified document', async () => {
  const next = {
    ...f.document,
    updated_at: Number(f.now) + 1,
    name: 'New name'
  }
  directory.principal_document.mockResolvedValueOnce({
    Ok: batch(await f.documentBatch(next))
  })
  expect((await read()).document?.name).toBe('New name')
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
})

const hint = () => ({
  home: Principal.fromUint8Array(f.home).toText(),
  directory: Principal.fromUint8Array(f.directory).toText()
})
const deferred = <T>() => {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

it('starts all ID reads together using hints, but waits for the verified registry binding', async () => {
  readProfile = createProfileReader(hint())
  const config = await registry.get_handle_config()
  registry.get_handle_config.mockClear()
  const waiting = deferred<typeof config>()
  registry.get_handle_config.mockReturnValueOnce(waiting.promise)
  let finished = false
  const result = read(f.account, false).then((value) => {
    finished = true
    return value
  })
  await vi.waitFor(() => {
    expect(user.user_config).toHaveBeenCalledTimes(1)
    expect(directory.directory_config).toHaveBeenCalledTimes(1)
    expect(directory.principal_document).toHaveBeenCalledTimes(1)
  })
  expect(finished).toBe(false)
  waiting.resolve(config)
  expect((await result).document).toEqual(f.document)
  expect(user.user_config).toHaveBeenCalledTimes(1)
  expect(directory.directory_config).toHaveBeenCalledTimes(1)
  expect(directory.principal_document).toHaveBeenCalledTimes(1)
})

it('starts handle resolution before registry config returns', async () => {
  const config = await registry.get_handle_config()
  const waiting = deferred<typeof config>()
  registry.get_handle_config.mockReturnValueOnce(waiting.promise)
  const result = read('alice', false)
  await vi.waitFor(() =>
    expect(registry.resolve_handle_certified).toHaveBeenCalledTimes(1)
  )
  waiting.resolve(config)
  expect((await result).account).toBe(f.account)
})

it('falls back when deployment hints are obsolete or fail', async () => {
  readProfile = createProfileReader({
    home: 'aaaaa-aa',
    directory: '2vxsx-fae'
  })
  expect((await read('alice', false)).document).toEqual(f.document)
  expect(user.user_config).toHaveBeenCalledTimes(1)
  expect(directory.directory_config).toHaveBeenCalledTimes(1)
})

it('reopens verified profiles without network reads and respects explicit refresh', async () => {
  readProfile = createProfileReader(hint())
  await read('alice', false)
  vi.clearAllMocks()
  expect(await read('ALICE', false)).toMatchObject({
    handle: 'ALICE',
    document: f.document
  })
  for (const query of [
    registry.get_handle_config,
    registry.resolve_handle_certified,
    user.user_config,
    directory.directory_config,
    directory.principal_document
  ])
    expect(query).not.toHaveBeenCalled()
  // A caller cannot mutate the cached document by editing a previous result.
  const result = await read(f.account, false)
  result.document!.name = 'Tampered'
  expect((await read(f.account, false)).document!.name).toBe('Alice')
  await read('alice', true)
  expect(registry.resolve_handle_certified).toHaveBeenCalledTimes(1)
  expect(directory.principal_document).toHaveBeenCalledTimes(1)
})

it('expires configuration and document caches after thirty seconds', async () => {
  await read(f.account, false)
  vi.spyOn(Date, 'now').mockReturnValue(Number(f.now) + 30_001)
  await read(f.account, false)
  expect(registry.get_handle_config).toHaveBeenCalledTimes(2)
  expect(user.user_config).toHaveBeenCalledTimes(2)
  expect(directory.principal_document).toHaveBeenCalledTimes(2)
})

it('refetches cached certificates that expire before their cache TTL', async () => {
  vi.spyOn(Date, 'now').mockReturnValue(Number(f.now) + 59_000)
  await read(f.account, false)
  vi.spyOn(Date, 'now').mockReturnValue(Number(f.now) + 61_000)
  const next = {
    ...f.document,
    name: 'Updated',
    updated_at: Number(f.now) + 61_000
  }
  directory.principal_document.mockResolvedValue({
    Ok: batch(
      await f.certified(
        f.directory,
        f.accountId,
        f.documentBytes(next),
        [new TextEncoder().encode('documents'), f.accountId],
        f.now + 61_000n
      )
    )
  })
  expect((await read(f.account, false)).document!.name).toBe('Updated')
  expect(directory.principal_document).toHaveBeenCalledTimes(2)
  expect(registry.get_handle_config).toHaveBeenCalledTimes(1)
})

it('does not cache failed reads or unverified document bytes', async () => {
  directory.principal_document.mockRejectedValueOnce(new TypeError('offline'))
  await expect(read(f.account, false)).rejects.toMatchObject({
    reason: 'unavailable'
  })
  expect((await read(f.account, false)).document).toEqual(f.document)
  expect(directory.principal_document).toHaveBeenCalledTimes(2)
})

it('refreshes a transferred handle without displaying the former owner’s cached document', async () => {
  readProfile = createProfileReader(hint())
  await read('alice', false)
  const id = f.accountId.slice()
  id[11] = 44
  const account = accountText(id)
  const now = f.now + 30_001n
  vi.spyOn(Date, 'now').mockReturnValue(Number(now))
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: batch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode('alice'),
        canonical({ handle: 'alice', version: 2, owner_account: id }),
        handlePath('alice'),
        now
      )
    )
  })
  const next = {
    ...f.document,
    id: `${f.principalOrigin}/${account}`,
    name: 'New owner',
    updated_at: Number(now)
  }
  directory.principal_document.mockResolvedValue({
    Ok: batch(
      await f.certified(
        f.directory,
        id,
        f.documentBytes(next),
        [new TextEncoder().encode('documents'), id],
        now
      )
    )
  })
  expect(await read('alice', false)).toMatchObject({ account, document: next })
  expect(directory.principal_document).toHaveBeenLastCalledWith(id)
})

it('checks document freshness after slower parallel configuration reads finish', async () => {
  readProfile = createProfileReader(hint())
  const config = await directory.directory_config()
  const waiting = deferred<typeof config>()
  directory.directory_config.mockReturnValueOnce(waiting.promise)
  const result = read(f.account, false)
  const rejected = expect(result).rejects.toMatchObject({
    reason: 'verification_failed'
  })
  await vi.waitFor(() =>
    expect(directory.principal_document).toHaveBeenCalledTimes(1)
  )
  vi.spyOn(Date, 'now').mockReturnValue(Number(f.now) + 60_001)
  waiting.resolve(config)
  await rejected
})
