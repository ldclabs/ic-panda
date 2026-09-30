import { describe, expect, it } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import {
  agentExecutionKind,
  agentId,
  delegationId,
  eventHash,
  eventText,
  executionApprovalMessage,
  grantEvent,
  isAgentId,
  jcs,
  type Json
} from '../src/lib/protocol/agent'
import { accountApprovalMessage } from '../src/lib/protocol/account'
import { b64, digest, hex } from '../src/lib/protocol/codec'
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
const home = Principal.fromUint8Array(new Uint8Array([9, 1])).toUint8Array()
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

  it('approves the exact execution kind the user home verifies (Rust vector)', () => {
    const event =
      '{"actor":"did:agent:AQ","created_at":1,"nonce":1,"payload":{},"protocol":"agent-delegation/1.0","type":"delegation.revoke"}'
    const deviceId = new Uint8Array(32).fill(3)
    const requestId = digest('dmsg/execution-request/v2', [account, 4n, deviceId, 5n])
    expect(hex(requestId)).toBe('3314ea786e3936efdc446d44c256d3806f9eda70a222abb76597c0a7cd73d5c9')
    const message = executionApprovalMessage({
      home,
      account,
      deviceId,
      securityEpoch: 4n,
      sequence: 5n,
      requestId,
      expiresAt: 1790000000000n,
      kind: agentExecutionKind(
        2,
        event,
        `https://id.dmsg.test/${xidText(account)}`,
        `chrome-extension://${'a'.repeat(32)}`
      ),
      maxCycles: 100000000000n
    })
    expect(hex(message)).toBe('19dbc3fca708066f83258d4eb2c0cedfab5a45f7298c223af2f73840588f32bc')
  })

  it('approves a controller registration exactly as Rust (Rust vector)', () => {
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
          supersedes: [1]
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
    expect(hex(accountApprovalMessage(Principal.fromUint8Array(home), request))).toBe(
      'e41be524bc2a4fa8893ae6ac85723da8f743cc2fc2aa3c8dc215bd6c5e2a542a'
    )
  })
})
