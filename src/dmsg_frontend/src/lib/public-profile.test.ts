// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import { readProfile } from './public-profile'
import { API_ORIGIN, DMSG_HANDLE_CANISTER_ID } from './constants'
// Load the SDK's synthetic signing authority at runtime; its test sources
// are outside this application's TypeScript rootDir.
const fixtureUrl = new URL(
  '../../../../packages/dmsg-sdk/tests/profile-fixture.ts',
  import.meta.url
)
const f = await import(/* @vite-ignore */ fixtureUrl.href)
import { handlePath, accountText } from 'dmsg-sdk/profile'
import { canonical, b64, unb64 } from 'dmsg-sdk/codec'
import type { CertifiedBatch } from 'dmsg-sdk/ic'

vi.mock('@icp-sdk/core/agent', async (original) => ({
  ...(await original<typeof import('@icp-sdk/core/agent')>()),
  Actor: { createActor: vi.fn() },
  HttpAgent: { create: vi.fn() }
}))
const candidBatch = (batch: CertifiedBatch) => ({
  ...batch,
  schema: Number(batch.schema),
  canister: Principal.fromUint8Array(batch.canister),
  entries: batch.entries.map((e) => ({
    ...e,
    value: e.value === null ? [] : [e.value]
  }))
})
const device = {
  ...f.device,
  added_at: 1n,
  next_sequence: 1n,
  added_by: [],
  revoked_at: [],
  input: {
    ...f.device.input,
    role: { Administrator: null },
    capabilities: [{ ContentSign: null }]
  }
}
const registry = {
  get_handle_config: vi.fn(),
  resolve_handle_certified: vi.fn(),
  get_legacy_reservation: vi.fn()
}
const user = { security_snapshot_batch: vi.fn(), get_device_bundle: vi.fn() }
const network = vi.fn()
const read = (input = f.account) =>
  readProfile(input, new AbortController().signal)
const response = (record: unknown = f.record) =>
  new Response(JSON.stringify({ ok: true, data: record }), {
    headers: { 'Content-Type': 'application/json' }
  })

beforeEach(async () => {
  vi.spyOn(Date, 'now').mockReturnValue(Number(f.now))
  vi.mocked(HttpAgent.create).mockResolvedValue({ rootKey: f.rootKey } as never)
  vi.mocked(Actor.createActor).mockImplementation(
    (_idl, { canisterId }) =>
      (String(canisterId) === Principal.fromUint8Array(f.home).toText()
        ? user
        : registry) as never
  )
  registry.get_handle_config.mockResolvedValue({
    environment: { Local: null },
    issuer_namespace: f.namespace,
    user_homes: [Principal.fromUint8Array(f.home)]
  })
  registry.get_legacy_reservation.mockResolvedValue({ Ok: [] })
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: candidBatch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode(f.handle),
        canonical({ handle: f.handle, version: 1, owner_account: f.accountId }),
        handlePath(f.handle)
      )
    )
  })
  user.security_snapshot_batch.mockResolvedValue({
    Ok: candidBatch(await f.securityBatch())
  })
  user.get_device_bundle.mockResolvedValue({ Ok: [{}, [[f.deviceId, device]]] })
  network.mockImplementation(() => Promise.resolve(response()))
  vi.stubGlobal('fetch', network)
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it('both root entrances read and verify the same account without legacy authentication', async () => {
  const direct = await read()
  expect(direct.account).toBe(f.account)
  expect(direct.published?.profile.display_name).toBe(f.profile.display_name)
  expect(registry.resolve_handle_certified).not.toHaveBeenCalled()
  const named = await read('ALICE')
  expect(named.handle).toBe('alice')
  expect(named.published).toEqual(direct.published)
  expect(registry.resolve_handle_certified).toHaveBeenCalledWith(['alice'])
  expect(registry.get_legacy_reservation).not.toHaveBeenCalled()
  expect(network).toHaveBeenCalledWith(
    `${API_ORIGIN}/public/accounts/${f.account}/profile`,
    expect.objectContaining({
      credentials: 'omit',
      redirect: 'error',
      cache: 'no-store'
    })
  )
})

it('invalid IDs do not fall back to handles or make requests', async () => {
  await expect(read('a'.repeat(20))).rejects.toMatchObject({
    reason: 'invalid_target'
  })
  expect(Actor.createActor).not.toHaveBeenCalled()
  expect(network).not.toHaveBeenCalled()
})

it('a proven missing handle stops before reading a profile', async () => {
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: candidBatch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode('alice'),
        null
      )
    )
  })
  await expect(read('alice')).rejects.toMatchObject({ reason: 'not_found' })
  expect(network).not.toHaveBeenCalled()
})

it('distinguishes a reserved legacy name without treating it as a new account', async () => {
  registry.resolve_handle_certified.mockResolvedValue({
    Ok: candidBatch(
      await f.certified(
        Principal.fromText(DMSG_HANDLE_CANISTER_ID).toUint8Array(),
        new TextEncoder().encode('alice'),
        null
      )
    )
  })
  registry.get_legacy_reservation.mockResolvedValue({
    Ok: [{ handle: 'alice', quarantined: false }]
  })
  await expect(read('ALICE')).rejects.toMatchObject({
    reason: 'legacy_reserved'
  })
  expect(registry.get_legacy_reservation).toHaveBeenCalledWith('alice')
  expect(user.security_snapshot_batch).not.toHaveBeenCalled()
  expect(network).not.toHaveBeenCalled()
})

it('malicious projection, signature and oversized responses never become displayed content', async () => {
  network.mockImplementationOnce(() =>
    Promise.resolve(
      response({ ...f.record, payload: { ...f.profile, bio: 'unverified' } })
    )
  )
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
  const encoded = unb64(f.record.signed.cose_sign1)
  encoded[encoded.length - 1]! ^= 1
  network.mockImplementationOnce(() =>
    Promise.resolve(
      response({ ...f.record, signed: { cose_sign1: b64(encoded) } })
    )
  )
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
  network.mockImplementationOnce(() =>
    Promise.resolve(new Response(' '.repeat(131073)))
  )
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
})

it('network failures and cancellation cannot produce a successful result', async () => {
  network.mockRejectedValueOnce(new TypeError('offline'))
  await expect(read()).rejects.toMatchObject({ reason: 'unavailable' })
  const controller = new AbortController()
  controller.abort()
  await expect(readProfile(f.account, controller.signal)).rejects.toMatchObject(
    { reason: 'unavailable' }
  )
})

it('rejects a changed device map even if the profile signature is valid', async () => {
  user.get_device_bundle.mockResolvedValueOnce({
    Ok: [{}, [[f.deviceId, { ...device, next_sequence: 2n }]]]
  })
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
  expect(network).not.toHaveBeenCalled()
})

it('distinguishes an unpublished profile from a proven missing account', async () => {
  const id = f.accountId.slice()
  id[11] = 99
  const account = accountText(id)
  const snapshot = {
    ...f.snapshot,
    account_id: id,
    issuer: f.namespace + account
  }
  user.security_snapshot_batch.mockResolvedValueOnce({
    Ok: candidBatch(await f.certified(f.home, id, canonical(snapshot)))
  })
  network.mockImplementationOnce(() =>
    Promise.resolve(new Response(null, { status: 404 }))
  )
  expect(await read(account)).toMatchObject({
    account,
    published: null,
    avatar: null
  })
  user.security_snapshot_batch.mockResolvedValueOnce({
    Ok: candidBatch(await f.certified(f.home, id, null))
  })
  await expect(read(account)).rejects.toMatchObject({ reason: 'not_found' })
})

it('remembers verified versions and rejects rollback or disappearance', async () => {
  const next = f.signedRecord({
    ...f.command,
    payload: { ...f.profile, version: 2, prev_hash: f.record.hash }
  })
  network.mockImplementationOnce(() => Promise.resolve(response(next)))
  expect((await read()).published?.profile.version).toBe(2)
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
  network.mockImplementationOnce(() =>
    Promise.resolve(new Response(null, { status: 404 }))
  )
  await expect(read()).rejects.toMatchObject({ reason: 'verification_failed' })
})
