import { describe, expect, it } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import {
  agentId,
  delegationId,
  eventHash,
  eventText,
  grantEvent,
  isAgentId,
  jcs,
  type Json
} from '../src/lib/protocol/agent'
import { accountApprovalMessage, controllerPopMessage } from '../src/lib/protocol/account'
import { b64, hex } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
import type { AccountMutation } from '../src/lib/canisters/generated/user'

// Agent Identity 1.0 vector "profile update" (agent-protocols 6a71313).
const vector = {
  event: {
    protocol: 'agent-profile/1.0',
    type: 'profile.update',
    actor: 'did:agent:6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw',
    created_at: 1779753600000,
    nonce: 1779753600000,
    payload: {
      id: 'did:agent:6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw',
      name: 'ResearchAgent',
      capabilities: ['research', 'code-review'],
      extra: {}
    }
  },
  jcs: '{"actor":"did:agent:6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw","created_at":1779753600000,"nonce":1779753600000,"payload":{"capabilities":["research","code-review"],"extra":{},"id":"did:agent:6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw","name":"ResearchAgent"},"protocol":"agent-profile/1.0","type":"profile.update"}',
  hash: 'pSqPTRievcRBpiy9Uk61jqkVdulqwvl-mp-FemLvsNs'
}
const home = Principal.fromUint8Array(new Uint8Array([9, 1]))
const account = new Uint8Array(12).fill(7)

describe('agent delegation events', () => {
  it('canonicalize and hash like the normative vector', () => {
    expect(jcs(vector.event as unknown as Json)).toBe(vector.jcs)
    expect(b64(eventHash(vector.jcs))).toBe(vector.hash)
    expect(jcs({ b: [1, 'x', null], a: { d: true, c: -0.5 } })).toBe(
      '{"a":{"c":-0.5,"d":true},"b":[1,"x",null]}'
    )
  })

  it('builds bounded grants under the account prefix', () => {
    const principalId = `https://id.dmsg.test/${xidText(account)}`
    const id = delegationId(xidText(account))
    expect(id).toMatch(new RegExp(`^${xidText(account)}\\.[A-Za-z0-9_-]{22}$`))
    const subject = agentId(new Uint8Array(32).fill(2))
    expect(isAgentId(subject)).toBe(true)
    const event = grantEvent(agentId(new Uint8Array(32).fill(1)), 5, {
      id,
      principalId,
      subject,
      scopes: ['message.draft'],
      audiences: ['https://dmsg.net'],
      days: 30
    }, 1000)
    expect(event.payload.expires_at).toBe(1000 + 30 * 86_400_000)
    expect(JSON.parse(eventText(event))).toEqual(event)
    for (const bad of [
      { scopes: ['*'] },
      { audiences: ['http://dmsg.net'] },
      { audiences: ['https://dmsg.net/path'] },
      { days: 367 },
      { subject: 'did:agent:short' }
    ])
      expect(() =>
        grantEvent(agentId(new Uint8Array(32).fill(1)), 5, {
          id,
          principalId,
          subject,
          scopes: ['message.draft'],
          audiences: ['https://dmsg.net'],
          days: 30,
          ...bad
        })
      ).toThrow()
  })

  it('approves a controller registration and its proof of possession exactly as Rust (Rust vectors)', () => {
    const request: AccountMutation = {
      account_id: account,
      expected_version: 10n,
      command: {
        RegisterController: {
          generation: 2,
          public_key: new Uint8Array(32).fill(5),
          name: ['dMsg hosted signer #2'],
          delegation: {
            Restricted: { scopes: ['message.draft'], audiences: ['https://dmsg.net'] }
          },
          proof: new Uint8Array(64).fill(8)
        }
      },
      approval: {
        device_id: new Uint8Array(32).fill(3),
        security_epoch: 4n,
        sequence: 5n,
        request_id: new Uint8Array(32).fill(6),
        expires_at: 1790000000000n,
        signature: new Uint8Array()
      }
    }
    expect(hex(accountApprovalMessage(home, request))).toBe(
      '8124d3a5f7974f6735155e84f25efc1ca0dcc0608a9d273bb0f83569c1d1957b'
    )
    expect(
      hex(
        controllerPopMessage(
          home,
          account,
          2,
          { Restricted: { scopes: ['message.draft'], audiences: ['https://dmsg.net'] } },
          new Uint8Array(32).fill(6)
        )
      )
    ).toBe('992ec6d7a7075d69c4c911aa4f218bbe884ca882d223d4e8f789d92c51089f31')
    expect(
      hex(
        controllerPopMessage(
          home,
          account,
          2,
          { Unrestricted: null },
          new Uint8Array(32).fill(6)
        )
      )
    ).not.toBe('992ec6d7a7075d69c4c911aa4f218bbe884ca882d223d4e8f789d92c51089f31')
  })
})
