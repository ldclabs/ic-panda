import { readFileSync, writeFileSync } from 'node:fs'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ed25519 } from '@noble/curves/ed25519.js'
import { b64, canonical, decodeCanonical, hash, hex, unb64 } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'
import {
  signCloudCommand,
  signCloudHttp,
  readCloudCommand,
  verifyCloudCommand,
  verifyCloudHttp,
  type CloudAction
} from '../src/lib/protocol/cloud'
import { CloudClient, RelayError, verifyCloudProfile } from '../src/lib/services/relay'

const now = 1_700_000_000_000
const seed = new Uint8Array(32).fill(7),
  publicKey = ed25519.getPublicKey(seed)
const accountId = xidText(new Uint8Array(12).fill(1))
const context = {
  accountId,
  issuer: `https://dmsg.test/u/${accountId}`,
  deviceId: '07'.repeat(32),
  securityEpoch: 4,
  requestId: '09'.repeat(32),
  deadline: now + 45_000
}
const profile = {
  version: 1,
  prev_hash: null,
  display_name: 'Public interop vector',
  bio: 'Synthetic test account',
  avatar_upload: null,
  links: ['https://example.com/']
}
const sign = async (message: Uint8Array) => ed25519.sign(message, seed)
const client = () => new CloudClient({ origin: 'https://dmsg.test', environment: 'local' })
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(now)
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('dmsg-cloud/1 client wire protocol', () => {
  it('rejects lossy Unicode in both payload keys and values before signing', async () => {
    await expect(
      signCloudCommand(context, 'dmsg/profile/v1', { ...profile, bio: '\ud800' }, sign)
    ).rejects.toThrow()
    await expect(
      signCloudCommand(context, 'dmsg/workflow/v1', { ['\udfff']: 'x' }, sign)
    ).rejects.toThrow()
    expect(decodeCanonical(canonical({ text: '你好🌍' }))).toEqual({ text: '你好🌍' })
    await expect(
      signCloudCommand(context, 'dmsg/profile/v1', { ...profile, bio: '\ufefftext' }, sign)
    ).rejects.toThrow()
  })
  it('fixes independent-consumer command and HTTP vectors', async () => {
    const command = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
    const body = canonical(command),
      url = new URL(`/v1/accounts/${accountId}/profile`, 'https://dmsg.test')
    const pop = await signCloudHttp(context, url, 'POST', body, sign)
    const vector = {
      schema: 1,
      test_only: true,
      at: now,
      context,
      profile,
      public_key_hex: hex(publicKey),
      command,
      body_hex: hex(body),
      http_pop: pop,
      url: url.href,
      body_digest: hash(body)
    }
    const path = new URL('./fixtures/cloud-v1.json', import.meta.url)
    if (process.env.DMSG_WRITE_CLOUD_VECTOR === '1')
      writeFileSync(path, JSON.stringify(vector, null, 2) + '\n')
    expect(vector).toEqual(JSON.parse(readFileSync(path, 'utf8')))
    expect(verifyCloudCommand(command, context, publicKey, 'dmsg/profile/v1').payload).toEqual(
      profile
    )
    expect(verifyCloudHttp(pop, publicKey).body.body_digest).toBe(hash(body))
  })
  it('rejects another issuer, key, purpose and altered signature', async () => {
    const signed = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
    expect(() =>
      verifyCloudCommand(
        signed,
        { ...context, issuer: context.issuer + 'x' },
        publicKey,
        'dmsg/profile/v1'
      )
    ).toThrow()
    expect(() =>
      verifyCloudCommand(
        signed,
        context,
        ed25519.getPublicKey(new Uint8Array(32).fill(8)),
        'dmsg/profile/v1'
      )
    ).toThrow()
    expect(() => verifyCloudCommand(signed, context, publicKey, 'dmsg/cursor/v1')).toThrow()
    const raw = unb64(signed.cose_sign1)
    raw[raw.length - 1] ^= 1
    expect(() =>
      verifyCloudCommand({ cose_sign1: b64(raw) }, context, publicKey, 'dmsg/profile/v1')
    ).toThrow()
  })
  it('rejects unsupported wire versions, profiles and unprotected headers', async () => {
    const signed = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
    const raw = decodeCanonical<unknown[]>(unb64(signed.cose_sign1).subarray(1))
    const encode = (value: unknown) => ({
      cose_sign1: b64(Uint8Array.from([0xd2, ...canonical(value)]))
    })
    const body = decodeCanonical<Record<string, unknown>>(raw[2] as Uint8Array)
    expect(() =>
      readCloudCommand(
        encode([raw[0], {}, canonical({ ...body, protocol: 'dmsg-cloud/0.1' }), raw[3]])
      )
    ).toThrow()
    expect(() => readCloudCommand(encode([raw[0], { extra: true }, raw[2], raw[3]]))).toThrow()
    const headers = decodeCanonical<Map<number, unknown>>(raw[0] as Uint8Array)
    headers.set(1, -8)
    expect(() => readCloudCommand(encode([canonical(headers), {}, raw[2], raw[3]]))).toThrow()
    await expect(
      signCloudCommand(context, 'signHash' as CloudAction, {}, sign)
    ).rejects.toThrow()
  })
  it('checks exclusive deadlines while still verifying historical content', async () => {
    const signed = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
    vi.setSystemTime(context.deadline)
    await expect(
      client().post(`/v1/accounts/${accountId}/profile`, signed, context, sign)
    ).rejects.toThrow()
    expect(verifyCloudCommand(signed, context, publicKey, 'dmsg/profile/v1').payload).toEqual(
      profile
    )
  })
  it('binds HTTP to exact bytes and freezes the body before awaiting a PoP signer', async () => {
    const signed = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
    const original = signed.cose_sign1
    const fetcher = vi.fn(async (_url: URL, init: RequestInit) => {
      const body = init.body as Uint8Array
      expect(decodeCanonical(body)).toEqual({ cose_sign1: original })
      const pop = verifyCloudHttp(
        (init.headers as Record<string, string>)['x-dmsg-pop'],
        publicKey
      )
      expect(pop.body).toMatchObject({
        body_digest: hash(body),
        method: 'POST',
        request_id: context.requestId
      })
      return Response.json({ ok: true, data: { version: 1 } })
    })
    vi.stubGlobal('fetch', fetcher)
    await client().post(
      `/v1/accounts/${accountId}/profile`,
      signed,
      context,
      async (message) => {
        signed.cose_sign1 = 'changed'
        return sign(message)
      }
    )
    expect(fetcher).toHaveBeenCalledOnce()
  })
  it('does not retry errors and preserves retryability and operation correlation', async () => {
    const fetcher = vi.fn(async () =>
      Response.json(
        {
          ok: false,
          error: { code: 'VERSION_CONFLICT', message: 'stale head', retryable: false },
          request_id: 'trace'
        },
        { status: 409 }
      )
    )
    vi.stubGlobal('fetch', fetcher)
    await expect(
      client().get(`/v1/accounts/${accountId}/profile`, context, sign)
    ).rejects.toMatchObject({
      code: 'VERSION_CONFLICT',
      retryable: false,
      requestId: 'trace'
    } satisfies Partial<RelayError>)
    expect(fetcher).toHaveBeenCalledOnce()
  })
  it('rejects altered profile JSON even when an unchanged signature accompanies it', async () => {
    const signed = await signCloudCommand(context, 'dmsg/profile/v1', profile, sign)
    const value = {
      signed,
      payload: profile,
      version: 1,
      hash: hash(unb64(signed.cose_sign1))
    }
    expect(verifyCloudProfile(value, context, publicKey)).toEqual(profile)
    expect(() =>
      verifyCloudProfile(
        { ...value, payload: { ...profile, bio: 'forged' } },
        context,
        publicKey
      )
    ).toThrow()
    expect(() =>
      verifyCloudProfile({ ...value, hash: '00'.repeat(32) }, context, publicKey)
    ).toThrow()
  })
  it('keeps production requests closed and rejects unsupported readiness versions', async () => {
    const fetcher = vi.fn(async () =>
      Response.json({
        ok: true,
        data: {
          protocol: 'dmsg-cloud/0.1',
          ready: true,
          gates: [],
          paid_contacts_enabled: false
        }
      })
    )
    vi.stubGlobal('fetch', fetcher)
    await expect(client().readiness()).rejects.toThrow()
    fetcher.mockClear()
    await expect(
      new CloudClient({ origin: 'https://dmsg.test', environment: 'production' }).get(
        '/v1/plans',
        context,
        sign
      )
    ).rejects.toThrow()
    expect(fetcher).not.toHaveBeenCalled()
  })
})
