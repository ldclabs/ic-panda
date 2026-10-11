import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE,
  HostedController,
  PrincipalInfo
} from '../src/lib/canisters/generated/user'
import type { AccountClient } from '../src/lib/services/account'
import type { CloudSession } from '../src/lib/services/cloud-session'
import { AgentClient, covers, type Credential } from '../src/lib/services/agent'
import { ed25519 } from '../src/lib/crypto/primitives'
import { controllerPopMessage } from '../src/lib/protocol/account'
import { agentId, eventHash, eventText } from '../src/lib/protocol/agent'
import { b64 } from '../src/lib/protocol/codec'
import { xidBytes, xidText } from '../src/lib/protocol/identity'

vi.mock('../src/lib/config', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/lib/config')>()
  return {
    ...actual,
    config: {
      ...actual.config,
      principalOrigin: 'https://id.dmsg.test',
      agentOrigin: 'https://agents.dmsg.test'
    }
  }
})

const NOW = 1_790_000_000_000
const rawAccount = new Uint8Array(12).fill(7)
const accountId = xidText(rawAccount)
const home = Principal.fromUint8Array(new Uint8Array([9, 1]))
const seed = new Uint8Array(32).fill(8)
const publicKey = ed25519.getPublicKey(seed)
const delegation = `${accountId}.example`

function fixture() {
  const stored = new Map<string, string>()
  const principal: PrincipalInfo = {
    principal_id: `https://id.dmsg.test/${accountId}`,
    published_version: 2n,
    state: {
      principal_type: { Person: null },
      version: 2n,
      updated_at: BigInt(NOW - 1000),
      controllers: [
        {
          generation: 1,
          public_key: publicKey,
          name: [],
          valid_from: BigInt(NOW - 1000),
          delegation: { Unrestricted: null },
          retired_at: [],
          invalid_from: []
        }
      ]
    }
  }
  const user = {
    get_principal: vi.fn<_SERVICE['get_principal']>().mockResolvedValue({ Ok: principal })
  }
  const controllerKey = vi.fn(
    async (_account: string, _generation: number, action: string, message?: Uint8Array) => ({
      publicKey,
      signature: action === 'sign' ? ed25519.sign(message!, seed) : new Uint8Array()
    })
  )
  const mutate = vi.fn(async (_account: string, command: any) =>
    typeof command === 'function' ? command({}, new Uint8Array(32).fill(6)) : command
  )
  const account = {
    home,
    user,
    meta: { account: { id: accountId }, deviceId: '03'.repeat(32) },
    crypto: {
      call: async (method: string, ...args: any[]) => {
        if (method === 'controlGet') return stored.get(args[0]) ?? null
        if (method === 'controlPut') {
          stored.set(args[0], args[1])
          return
        }
        if (method === 'controllerKey') return controllerKey(args[0], args[1], args[2], args[3])
        throw new Error(`unexpected crypto call: ${method}`)
      }
    },
    mutate,
    refresh: async () => ({
      info: { security_epoch: 1n },
      device: { input: { capabilities: [{ FormalApprove: null }] }, next_sequence: 1n }
    })
  } as unknown as AccountClient
  const get = vi.fn<CloudSession['get']>()
  const cloud = { get } as unknown as CloudSession
  const reconnect = () => new AgentClient(account, cloud)
  const submit = vi.fn<typeof fetch>().mockImplementation(async (_url, init) => {
    const envelope = JSON.parse(init!.body as string)
    return Response.json({ id: envelope.event.payload.id, event_id: envelope.hash })
  })
  vi.stubGlobal('fetch', submit)
  return { client: reconnect(), reconnect, user, controllerKey, mutate, submit, get, stored }
}

beforeEach(() => {
  vi.spyOn(Date, 'now').mockReturnValue(NOW)
  vi.stubGlobal('chrome', { runtime: { id: 'a'.repeat(32) } })
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('self-held controller signing', () => {
  it('signs a revocation with the vault key, journals it and submits it once', async () => {
    const f = fixture()
    const job = await f.client.revoke(accountId, 1, delegation)
    expect(job.stage).toBe('accepted')
    expect(f.controllerKey).toHaveBeenCalledTimes(1)
    const sent = JSON.parse(f.submit.mock.calls[0][1]!.body as string)
    expect(sent).toEqual(job.envelope)
    expect(sent.event.actor).toBe(agentId(publicKey))
    expect(sent.event.nonce).toBe(NOW)
    const hash = eventHash(eventText(sent.event))
    expect(b64(hash)).toBe(sent.hash)
    expect(ed25519.verify(Buffer.from(sent.signature, 'base64'), hash, publicKey)).toBe(true)
    expect((await f.client.jobs())[0]).toEqual(job)
    // The next event of this generation uses a later nonce even within the same millisecond.
    const next = await f.client.revoke(accountId, 1, `${accountId}.other`)
    expect(next.envelope.event.nonce).toBe(NOW + 1)
  })

  it('keeps a signed envelope for resubmission after a transient failure', async () => {
    const f = fixture()
    f.submit.mockResolvedValueOnce(
      Response.json({ error: { code: 'unavailable' } }, { status: 503 })
    )
    const job = await f.client.revoke(accountId, 1, delegation)
    expect(job).toMatchObject({ stage: 'signed', error: 'unavailable' })
    expect((await f.reconnect().resume(job.id)).stage).toBe('accepted')
    expect(f.controllerKey).toHaveBeenCalledTimes(1)
    for (const [, init] of f.submit.mock.calls) expect(JSON.parse(init!.body as string)).toEqual(job.envelope)
  })

  it('records a definitive refusal without resubmitting', async () => {
    const f = fixture()
    f.submit.mockResolvedValueOnce(
      Response.json({ error: { code: 'permission_denied' } }, { status: 403 })
    )
    const job = await f.client.revoke(accountId, 1, delegation)
    expect(job).toMatchObject({ stage: 'failed', error: 'permission_denied' })
    expect(await f.reconnect().resume(job.id)).toEqual(job)
    expect(f.submit).toHaveBeenCalledTimes(1)
  })

  it('continues above the nonce another device holding the key already used', async () => {
    const f = fixture()
    f.submit.mockResolvedValueOnce(
      Response.json(
        { error: { code: 'nonce_not_greater', data: { max_nonce: NOW + 5000 } } },
        { status: 409 }
      )
    )
    const job = await f.client.revoke(accountId, 1, delegation)
    expect(job).toMatchObject({ stage: 'failed', error: 'nonce_not_greater' })
    const next = await f.client.revoke(accountId, 1, delegation)
    expect(next.stage).toBe('accepted')
    expect(next.envelope.event.nonce).toBe(NOW + 5001)
  })

  it('refuses a vault key that does not match the registered controller', async () => {
    const f = fixture()
    f.controllerKey.mockResolvedValueOnce({
      publicKey: ed25519.getPublicKey(new Uint8Array(32).fill(9)),
      signature: new Uint8Array(64)
    })
    await expect(f.client.revoke(accountId, 1, delegation)).rejects.toMatchObject({
      code: 'INTEGRITY_FAILED'
    })
    expect(f.submit).not.toHaveBeenCalled()
  })

  it('registers the next generation with a proof of possession over the approval request', async () => {
    const f = fixture()
    const generation = await f.client.register(accountId, {
      authority: { kind: 'restricted', scopes: ['message.draft'], audiences: ['https://dmsg.net'] },
      name: 'second'
    })
    expect(generation).toBe(2)
    const command = await f.mutate.mock.results[0].value
    expect(command.RegisterController).toMatchObject({
      generation: 2,
      public_key: publicKey,
      name: ['second']
    })
    expect(
      ed25519.verify(
        command.RegisterController.proof,
        controllerPopMessage(
          home,
          xidBytes(accountId),
          2,
          { Restricted: { scopes: ['message.draft'], audiences: ['https://dmsg.net'] } },
          new Uint8Array(32).fill(6)
        ),
        publicKey
      )
    ).toBe(true)
  })
})

describe('agent credential listing', () => {
  const credentials: Credential[] = Array.from({ length: 101 }, (_, n) => ({
    id: `${accountId}.${String(n).padStart(3, '0')}`,
    subject: agentId(publicKey),
    controller: agentId(publicKey),
    scopes: ['message.draft'],
    audiences: ['https://dmsg.net'],
    status: n < 100 ? 'revoked' : 'active',
    accepted_at: NOW
  }))

  it('includes active credentials beyond the first page', async () => {
    const f = fixture()
    f.get.mockResolvedValueOnce({
      result: credentials.slice(0, 100),
      next_cursor: credentials[99].id
    })
    f.get.mockResolvedValueOnce({ result: credentials.slice(100) })
    expect(await f.client.credentials(accountId)).toEqual(credentials)
    expect(f.get.mock.calls).toEqual([
      [`/v1/accounts/${accountId}/delegations?limit=100`],
      [`/v1/accounts/${accountId}/delegations?limit=100&cursor=${credentials[99].id}`]
    ])
  })

  it('does not present a partial list when a later page fails', async () => {
    const f = fixture()
    f.get.mockResolvedValueOnce({
      result: credentials.slice(0, 100),
      next_cursor: credentials[99].id
    })
    f.get.mockRejectedValueOnce(new Error('offline'))
    await expect(f.client.credentials(accountId)).rejects.toThrow('offline')
  })

  it('lets only a key whose ceiling covers a credential manage it', () => {
    const controller = (delegation: HostedController['delegation']): HostedController => ({
      generation: 1,
      public_key: publicKey,
      name: [],
      valid_from: BigInt(NOW),
      delegation,
      retired_at: [],
      invalid_from: []
    })
    const restricted = (scopes: string[], audiences: string[]) =>
      controller({ Restricted: { scopes, audiences } })
    const credential = credentials[100]
    const wider = restricted(['message.draft', 'inbox.screen'], ['https://dmsg.net'])
    expect(covers(controller({ Unrestricted: null }), credential)).toBe(true)
    expect(covers(wider, credential)).toBe(true)
    expect(covers(restricted(['inbox.screen'], ['https://dmsg.net']), credential)).toBe(false)
    expect(covers(restricted(['message.draft'], ['https://dmsg.app']), credential)).toBe(false)
  })
})
