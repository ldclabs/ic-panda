import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { encode, decode } from 'cborg'
import { b64, unb64 } from './protocol'
import {
  completePasskey,
  parsePasskeyRequest,
  type PasskeyRequest
} from './passkey'

const request = (): PasskeyRequest => ({
  protocol: 'dmsg-passkey/1',
  state: new Uint8Array(32).fill(1),
  challenge: {
    challenge: new Uint8Array(32).fill(2),
    action: 'login',
    expires_at: Date.now() + 300_000
  },
  next_url: 'http://127.0.0.1:2345/auth/callback',
  credential_id: null
})

describe('first-party Passkey transport', () => {
  it('refuses foreign redirects and expired challenges', () => {
    const value = request()
    expect(parsePasskeyRequest(b64(encode(value)))).toEqual(value)
    const reset = { ...value, challenge: { ...value.challenge, action: 'logout_all' } }
    expect(parsePasskeyRequest(b64(encode(reset)))).toEqual(reset)
    for (const changed of [
      { ...value, next_url: 'https://evil.example/' },
      { ...value, challenge: { ...value.challenge, expires_at: 1 } },
      { ...value, unexpected: true }
    ])
      expect(() => parsePasskeyRequest(b64(encode(changed)))).toThrow()
  })

  it('requests verified discoverable authentication and preserves signed bytes', async () => {
    const value = request()
    const get = vi
      .fn()
      .mockResolvedValue({
        type: 'public-key',
        rawId: new Uint8Array([3]).buffer,
        response: {
          clientDataJSON: new Uint8Array([4]).buffer,
          authenticatorData: new Uint8Array([5]).buffer,
          signature: new Uint8Array([6]).buffer
        }
      })
    const navigate = vi.fn()
    await completePasskey(value, { get, create: vi.fn() }, navigate)
    expect(get.mock.calls[0]![0].publicKey).toMatchObject({
      rpId: 'dmsg.net',
      userVerification: 'required'
    })
    const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
    expect(result.state).toEqual(value.state)
    expect(result.response.signature).toEqual(new Uint8Array([6]))
    expect(result.error).toBeNull()
  })

  it('returns cancellation without assertion bytes', async () => {
    const navigate = vi.fn()
    await completePasskey(
      request(),
      {
        get: vi.fn().mockRejectedValue({ name: 'NotAllowedError' }),
        create: vi.fn()
      },
      navigate
    )
    const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
    expect(result.error).toBe('user_cancelled')
    expect(result.response).toBeNull()
  })
})

it('roundtrips the Rust-generated WebAuthn callback byte for byte', async () => {
  const fixture = JSON.parse(
    readFileSync(resolve('../../tests/vectors/client/passkey-v1.json'), 'utf8')
  )
  const value = parsePasskeyRequest(fixture.request, fixture.now_ms)
  const expected = decode(unb64(fixture.callback))
  const response = expected.response
  const get = vi.fn()
  // Buffer-backed decoded bytes need an exact ArrayBuffer without adjacent CBOR bytes.
  const buffer = (bytes: Uint8Array) => Uint8Array.from(bytes).buffer
  get.mockResolvedValue({
    type: 'public-key',
    rawId: buffer(response.credential_id),
    response: {
      clientDataJSON: buffer(response.client_data_json),
      authenticatorData: buffer(response.authenticator_data),
      signature: buffer(response.signature)
    }
  })
  vi.spyOn(Date, 'now').mockReturnValue(fixture.now_ms)
  const navigate = vi.fn()
  try {
    await completePasskey(value, { get, create: vi.fn() }, navigate)
  } finally {
    vi.restoreAllMocks()
  }
  expect(navigate.mock.calls[0]![0]).toBe(
    `${value.next_url}#${fixture.callback}`
  )
})
