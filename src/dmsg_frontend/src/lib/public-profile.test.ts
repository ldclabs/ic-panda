// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import { readProfile } from './public-profile'
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
const read = (input = f.account) =>
  readProfile(input, new AbortController().signal)
beforeEach(async () => {
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
    handle: 'alice',
    document: f.document
  })
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
