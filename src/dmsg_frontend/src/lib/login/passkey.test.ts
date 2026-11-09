import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { encode, decode } from 'cborg'
import { b64, unb64 } from './protocol'
import {
  completePasskey,
  parsePasskeyRequest,
  supportsPasskey,
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

beforeEach(() => {
  vi.stubGlobal('isSecureContext', true)
  vi.stubGlobal('PublicKeyCredential', class {})
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('first-party Passkey transport', () => {
  it('refuses foreign redirects and expired challenges', () => {
    const value = request()
    expect(parsePasskeyRequest(b64(encode(value)))).toEqual(value)
    const reset = {
      ...value,
      challenge: { ...value.challenge, action: 'logout_all' }
    }
    expect(parsePasskeyRequest(b64(encode(reset)))).toEqual(reset)
    for (const changed of [
      { ...value, next_url: 'https://evil.example/' },
      { ...value, challenge: { ...value.challenge, expires_at: 1 } },
      { ...value, unexpected: true }
    ])
      expect(() => parsePasskeyRequest(b64(encode(changed)))).toThrow()
  })

  it('requires discoverable ES256 credentials on old and new WebAuthn clients', async () => {
    const value = request()
    value.challenge.action = 'register'
    const create = vi.fn().mockResolvedValue({
      type: 'public-key',
      rawId: new Uint8Array([3]).buffer,
      response: {
        clientDataJSON: new Uint8Array([4]).buffer,
        attestationObject: new Uint8Array([5]).buffer
      }
    })
    const get = vi.fn()
    const navigate = vi.fn()
    await completePasskey(value, { create, get }, navigate)
    const options = create.mock.calls[0]![0].publicKey
    expect(options).toMatchObject({
      rp: { id: 'dmsg.net' },
      user: { id: value.state, name: 'dMsg · 01010101', displayName: 'dMsg · 01010101' },
      pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
      authenticatorSelection: {
        residentKey: 'required',
        requireResidentKey: true,
        userVerification: 'required'
      },
      attestation: 'none'
    })
    expect(options.authenticatorSelection).not.toHaveProperty(
      'authenticatorAttachment'
    )
    expect(get).not.toHaveBeenCalled()
    const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
    expect(result.response.authenticator_data).toEqual(new Uint8Array([5]))
    expect(result.response.signature).toEqual(new Uint8Array())
    expect(result.error).toBeNull()
  })

  it('does not require a platform authenticator for external security keys', () => {
    const platformAvailable = vi.fn().mockResolvedValue(false)
    vi.stubGlobal('PublicKeyCredential', {
      isUserVerifyingPlatformAuthenticatorAvailable: platformAvailable
    })
    expect(supportsPasskey({ create: vi.fn(), get: vi.fn() })).toBe(true)
    expect(platformAvailable).not.toHaveBeenCalled()
  })

  it.each([undefined, {}, { create: vi.fn() }, { get: vi.fn() }])(
    'returns a supported callback when credentials methods are missing: %o',
    async (credentials) => {
      expect(supportsPasskey(credentials)).toBe(false)
      const navigate = vi.fn()
      await completePasskey(request(), credentials, navigate)
      const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
      expect(result.error).toBe('passkey_error')
      expect(result.response).toBeNull()
      expect(navigate).toHaveBeenCalledTimes(1)
    }
  )

  it.each(['isSecureContext', 'PublicKeyCredential'])(
    'rejects an unavailable %s before calling an authenticator',
    async (capability) => {
      vi.stubGlobal(
        capability,
        capability === 'isSecureContext' ? false : undefined
      )
      const credentials = { create: vi.fn(), get: vi.fn() }
      const navigate = vi.fn()
      expect(supportsPasskey(credentials)).toBe(false)
      await completePasskey(request(), credentials, navigate)
      expect(credentials.create).not.toHaveBeenCalled()
      expect(credentials.get).not.toHaveBeenCalled()
      const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
      expect(result.error).toBe('passkey_error')
      expect(result.response).toBeNull()
    }
  )

  it('requests verified discoverable authentication and preserves signed bytes', async () => {
    const value = request()
    const get = vi.fn().mockResolvedValue({
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

  it.each([
    ['NotAllowedError', 'passkey_error'],
    ['NotSupportedError', 'passkey_error'],
    ['SecurityError', 'passkey_error'],
    ['UnknownError', 'passkey_error'],
    ['AbortError', 'user_cancelled'],
    ['TimeoutError', 'timeout']
  ])('classifies %s as %s without browser diagnostics', async (name, error) => {
    const navigate = vi.fn()
    await completePasskey(
      request(),
      {
        get: vi
          .fn()
          .mockRejectedValue({ name, message: 'private browser details' }),
        create: vi.fn()
      },
      navigate
    )
    const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
    expect(result.error).toBe(error)
    expect(result.response).toBeNull()
    expect(Object.keys(result).sort()).toEqual([
      'error',
      'protocol',
      'response',
      'state'
    ])
  })

  it('does not claim cancellation when the browser returns no credential', async () => {
    const navigate = vi.fn()
    await completePasskey(
      request(),
      { get: vi.fn().mockResolvedValue(null), create: vi.fn() },
      navigate
    )
    const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
    expect(result.error).toBe('passkey_error')
    expect(result.response).toBeNull()
  })

  it('returns timeout without opening an authenticator after the challenge expires', async () => {
    const value = request()
    value.challenge.expires_at = Date.now()
    const credentials = { get: vi.fn(), create: vi.fn() }
    const navigate = vi.fn()
    await completePasskey(value, credentials, navigate)
    expect(credentials.get).not.toHaveBeenCalled()
    expect(credentials.create).not.toHaveBeenCalled()
    const result = decode(unb64(navigate.mock.calls[0]![0].split('#')[1]))
    expect(result.error).toBe('timeout')
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
