import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { complete, errorCode, takeFragment } from './flow'
import {
  callbackPort,
  callbackTarget,
  IOS_CALLBACK,
  encodeError,
  encodeResult,
  parseRequest,
  unb64,
  type LoginErrorCode
} from './protocol'

const vectors = JSON.parse(
  readFileSync(resolve('../../tests/vectors/client/login-v1.json'), 'utf8')
)
const hex = (bytes: Uint8Array) =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
const unhex = (text: string) =>
  Uint8Array.from(text.match(/../g) ?? [], (b) => parseInt(b, 16))

describe('dmsg-login/1 vectors', () => {
  it('reads every valid request', () => {
    for (const item of vectors.requests) {
      const request = parseRequest(item.fragment)
      expect({
        ...request,
        state: hex(request.state),
        session_pubkey: hex(request.session_pubkey)
      }).toEqual(item.fields)
      expect(hex(unb64(item.fragment))).toBe(item.cbor_hex)
    }
  })

  it('refuses every rejected request before anything else', () => {
    for (const item of vectors.rejected_requests)
      expect(() => parseRequest(item.fragment), item.name).toThrow()
    expect(() => parseRequest('')).toThrow()
    expect(() => parseRequest(vectors.requests[0].fragment + '=')).toThrow()
  })

  it('writes callbacks byte for byte', () => {
    for (const item of vectors.results)
      expect(
        encodeResult(unhex(item.fields.state), item.fields.delegation)
      ).toBe(item.fragment)
    for (const item of vectors.errors)
      expect(
        encodeError(
          unhex(item.fields.state),
          item.fields.error as LoginErrorCode
        )
      ).toBe(item.fragment)
  })

  it('accepts only loopback IP callbacks', () => {
    expect(callbackPort('http://127.0.0.1:8/auth/callback')).toBe(8)
    expect(callbackPort('http://[::1]:65535/auth/callback')).toBe(65535)
    for (const url of [
      'http://localhost:8/auth/callback',
      'http://127.0.0.1:65536/auth/callback',
      'http://127.0.0.1:8/auth/callback/'
    ])
      expect(() => callbackPort(url)).toThrow()
  })

  it('accepts only the exact associated iOS callback', () => {
    expect(callbackTarget(IOS_CALLBACK)).toBe('ios')
    for (const url of [
      IOS_CALLBACK + '/',
      IOS_CALLBACK + '?next=x',
      IOS_CALLBACK + '#x',
      'https://dmsg.net:443/_/ios/auth/callback',
      'dmsg://auth/callback',
      'https://dmsg.net.evil.example/_/ios/auth/callback'
    ]) {
      expect(() => callbackTarget(url)).toThrow()
    }
  })
})

describe('login flow', () => {
  const request = parseRequest(vectors.requests[0].fragment)

  it('clears the fragment when reading it', () => {
    const replaceState = vi.fn()
    expect(
      takeFragment({ hash: '#abc', pathname: '/_/login' }, { replaceState })
    ).toBe('abc')
    expect(replaceState).toHaveBeenCalledWith(null, '', '/_/login')
  })

  it('returns the delegation to the callback', async () => {
    const navigate = vi.fn()
    const delegation = vectors.results[0].fields.delegation
    await complete(request, async () => delegation, navigate)
    expect(navigate).toHaveBeenCalledWith(
      `${request.next_url}#${encodeResult(request.state, delegation)}`
    )
  })

  it('returns a classified error to the callback', async () => {
    const navigate = vi.fn()
    await complete(
      request,
      async () => {
        throw Object.assign(new Error('User rejected'), { code: 3001 })
      },
      navigate
    )
    expect(navigate).toHaveBeenCalledWith(
      `${request.next_url}#${encodeError(request.state, 'user_cancelled')}`
    )
    expect(errorCode(new Error('invalid derivation origin'))).toBe(
      'derivation_origin_not_allowed'
    )
    expect(
      errorCode(Object.assign(new Error('x'), { name: 'TimeoutError' }))
    ).toBe('timeout')
    expect(errorCode(new Error('boom'))).toBe('ii_error')
  })
})
