import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE,
  AgentEventSignRequest,
  Error as CanisterError,
  ExecutionOutcome,
  ExecutionResult,
  PrincipalInfo
} from '../src/lib/canisters/generated/user'
import type { _SERVICE as CoseService } from '../src/lib/canisters/generated/cose'
import type { AccountClient } from '../src/lib/services/account'
import type { CloudSession } from '../src/lib/services/cloud-session'
import { AgentClient, type Credential } from '../src/lib/services/agent'
import { ed25519 } from '../src/lib/crypto/primitives'
import { encodeControl } from '../src/lib/protocol/account'
import { agentId, eventHash } from '../src/lib/protocol/agent'
import { xidText } from '../src/lib/protocol/identity'

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

function result(request: AgentEventSignRequest, outcome: ExecutionOutcome): ExecutionResult {
  return { request_id: request.approval.request_id, outcome, cycles_cost_upper_bound: 100n }
}

function completed(request: AgentEventSignRequest): ExecutionResult {
  const hash = eventHash(request.event)
  return result(request, {
    Completed: {
      AgentSignature: {
        event_hash: hash,
        signature: ed25519.sign(hash, seed),
        key: {
          account_id: rawAccount,
          algorithm: { Ed25519: null },
          key_generation: 1n,
          public_key_fingerprint: new Uint8Array(32),
          derivation_version: 2,
          public_key: publicKey,
          key_id: new Uint8Array(32),
          home_cose: home,
          environment: { Local: null },
          master_key_name: 'test_key_1',
          purpose: { AgentController: null }
        }
      }
    }
  })
}

function fixture() {
  const stored = new Map<string, string>()
  const principal: PrincipalInfo = {
    principal_id: `https://id.dmsg.test/${accountId}`,
    published_version: 2n,
    last_nonces: [],
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
          supersedes: [],
          retired_at: [],
          invalid_from: []
        }
      ]
    }
  }
  const user = {
    get_principal: vi.fn<_SERVICE['get_principal']>().mockResolvedValue({ Ok: principal }),
    sign_agent_event: vi
      .fn<_SERVICE['sign_agent_event']>()
      .mockRejectedValue(new Error('timeout')),
    get_execution: vi.fn<_SERVICE['get_execution']>(),
    reconcile_execution: vi.fn<_SERVICE['reconcile_execution']>()
  }
  const deviceSign = vi.fn((message: Uint8Array) => ed25519.sign(message, seed))
  const account = {
    home,
    user,
    meta: { account: { id: accountId }, deviceId: '03'.repeat(32) },
    crypto: {
      call: async (method: string, key: string | Uint8Array, value?: string) => {
        if (method === 'controlGet') return stored.get(key as string) ?? null
        if (method === 'controlPut') {
          stored.set(key as string, value!)
          return
        }
        if (method === 'deviceSign') return deviceSign(key as Uint8Array)
        throw new Error(`unexpected crypto call: ${method}`)
      }
    },
    refresh: async () => ({
      info: {
        security_epoch: 1n,
        sensitive_policy: { frozen: false, allowed_purposes: [{ AgentController: null }] }
      },
      device: { input: { capabilities: [{ FormalApprove: null }] }, next_sequence: 1n }
    })
  } as unknown as AccountClient
  const get = vi.fn<CloudSession['get']>()
  const cloud = { get } as unknown as CloudSession
  const reconnect = () => new AgentClient(account, {} as CoseService, cloud)
  const submit = vi.fn<typeof fetch>().mockImplementation(async (_url, init) => {
    const envelope = JSON.parse(init!.body as string)
    return Response.json({ id: envelope.event.payload.id, event_id: envelope.hash })
  })
  vi.stubGlobal('fetch', submit)
  return { client: reconnect(), reconnect, user, deviceSign, submit, get }
}

beforeEach(() => {
  vi.spyOn(Date, 'now').mockReturnValue(NOW)
  vi.stubGlobal('chrome', { runtime: { id: 'a'.repeat(32) } })
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('agent execution recovery', () => {
  it.each<ExecutionOutcome>([
    { Authorized: null },
    { Executing: null },
    { Unknown: { Unavailable: 'awaiting management response' } }
  ])(
    'retains a nonterminal result until the original signature completes: %j',
    async (outcome) => {
      const f = fixture()
      f.user.sign_agent_event.mockImplementation(async (request) => ({
        Ok: result(request, outcome)
      }))
      const job = await f.client.revoke(accountId, 1, delegation)
      expect(job.stage).toBe('unknown')
      const request = f.user.sign_agent_event.mock.calls[0][0]
      f.user.get_execution.mockResolvedValue({ Ok: result(request, outcome) })
      f.user.reconcile_execution.mockResolvedValue({ Ok: result(request, outcome) })

      const resumed = f.reconnect()
      expect((await resumed.resume(job.executionId)).stage).toBe('unknown')
      expect((await resumed.jobs())[0].stage).toBe('unknown')
      expect(f.submit).not.toHaveBeenCalled()

      f.user.get_execution.mockResolvedValue({ Ok: completed(request) })
      expect((await resumed.resume(job.executionId)).stage).toBe('accepted')
      expect((await resumed.job(job.executionId))?.error).toBeUndefined()
      expect(f.user.reconcile_execution).toHaveBeenCalledWith(
        rawAccount,
        request.approval.request_id
      )
      expect(f.user.sign_agent_event).toHaveBeenCalledTimes(1)
      expect(f.deviceSign).toHaveBeenCalledTimes(1)
      const sent = JSON.parse(f.submit.mock.calls[0][1]!.body as string)
      expect(sent.event).toEqual(JSON.parse(request.event))
    }
  )

  it.each<CanisterError>([{ Unavailable: 'COSE unavailable' }, { QuotaExceeded: null }])(
    'reconciles an authorized request after a dispatch error: %j',
    async (error) => {
      const f = fixture()
      f.user.sign_agent_event.mockResolvedValue({ Err: error })
      const job = await f.client.revoke(accountId, 1, delegation)
      expect(job).toMatchObject({ stage: 'unknown', error: Object.keys(error)[0] })
      const request = f.user.sign_agent_event.mock.calls[0][0]
      f.user.get_execution.mockResolvedValue({ Ok: result(request, { Authorized: null }) })
      f.user.reconcile_execution.mockResolvedValue({ Ok: completed(request) })

      expect((await f.reconnect().resume(job.executionId)).stage).toBe('accepted')
      expect(f.user.reconcile_execution).toHaveBeenCalledWith(
        rawAccount,
        request.approval.request_id
      )
      expect(f.user.sign_agent_event).toHaveBeenCalledTimes(1)
      expect(f.deviceSign).toHaveBeenCalledTimes(1)
    }
  )

  it('retries the exact journaled approval after a transport failure and absent result', async () => {
    const f = fixture()
    const job = await f.client.revoke(accountId, 1, delegation)
    expect(job).toMatchObject({ stage: 'unknown', error: 'EXECUTION_UNKNOWN' })
    f.user.get_execution.mockResolvedValue({ Err: { ResultExpired: null } })
    f.user.sign_agent_event.mockImplementation(async (request) => ({ Ok: completed(request) }))

    expect((await f.reconnect().resume(job.executionId)).stage).toBe('accepted')
    expect(f.user.sign_agent_event).toHaveBeenCalledTimes(2)
    for (const [request] of f.user.sign_agent_event.mock.calls)
      expect(encodeControl('sign_agent_event', [request])).toBe(job.request)
    expect(f.deviceSign).toHaveBeenCalledTimes(1)
  })

  it('keeps a pending journal when reconciliation is unavailable', async () => {
    const f = fixture()
    const job = await f.client.revoke(accountId, 1, delegation)
    f.user.get_execution.mockRejectedValue(new Error('offline'))
    await expect(f.reconnect().resume(job.executionId)).rejects.toThrow('offline')
    expect((await f.client.job(job.executionId))?.stage).toBe('unknown')
    expect(f.user.sign_agent_event).toHaveBeenCalledTimes(1)
  })

  it.each<ExecutionOutcome>([{ Failed: { Expired: null } }, { ResultExpired: null }])(
    'finishes a confirmed terminal failure without resubmission: %j',
    async (outcome) => {
      const f = fixture()
      f.user.sign_agent_event.mockImplementation(async (request) => ({
        Ok: result(request, outcome)
      }))
      const job = await f.client.revoke(accountId, 1, delegation)
      expect(job.stage).toBe('failed')
      expect(await f.reconnect().resume(job.executionId)).toEqual(job)
      expect(f.user.get_execution).not.toHaveBeenCalled()
      expect(f.submit).not.toHaveBeenCalled()
    }
  )

  it('ends an absent request only after its approval expires', async () => {
    const f = fixture()
    f.user.sign_agent_event.mockResolvedValue({ Err: { Forbidden: null } })
    const job = await f.client.revoke(accountId, 1, delegation)
    f.user.get_execution.mockResolvedValue({ Err: { ResultExpired: null } })
    vi.mocked(Date.now).mockReturnValue(NOW + 240_000)
    expect(await f.reconnect().resume(job.executionId)).toMatchObject({
      stage: 'failed',
      error: 'Expired'
    })
    expect(f.user.sign_agent_event).toHaveBeenCalledTimes(1)
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
})

it('keeps a signed event reconcilable after a transient failure outside the admission window', async () => {
  const f = fixture()
  f.user.sign_agent_event.mockImplementation(async (request) => ({ Ok: completed(request) }))
  f.submit.mockRejectedValueOnce(new Error('lost accepted response'))
  await expect(f.client.revoke(accountId, 1, delegation)).rejects.toThrow(
    'lost accepted response'
  )
  const [job] = await f.client.jobs()
  vi.mocked(Date.now).mockReturnValue(NOW + 301000)
  f.submit.mockResolvedValueOnce(
    Response.json({ error: { code: 'unavailable' } }, { status: 503 })
  )
  expect((await f.client.resume(job.executionId)).stage).toBe('signed')
  expect((await f.client.resume(job.executionId)).stage).toBe('accepted')
  expect(f.user.sign_agent_event).toHaveBeenCalledTimes(1)
})
