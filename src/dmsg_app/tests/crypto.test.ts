import { describe, expect, it } from 'vitest'
import {
  seal,
  open,
  hpkeSeal,
  hpkeOpen,
  hpkePublic,
  unlockKey,
  prfKey,
  prfSalt,
  deviceRootContext
} from '../src/lib/crypto/primitives'
import { random, utf8, b64, unb64 } from '../src/lib/protocol/codec'

describe('content cryptography', () => {
  it('authenticates object context and detects modified COSE bytes', async () => {
    const key = random(),
      aad = ['dmsg/content/1', 'alice', 'v1'],
      plaintext = utf8('Private credential')
    const cipher = await seal(key, plaintext, aad)
    expect(await open(key, cipher, aad)).toEqual(plaintext)
    await expect(open(key, cipher, ['dmsg/content/1', 'bob', 'v1'])).rejects.toThrow()
    const modified = unb64(cipher)
    modified[modified.length - 1] ^= 1
    await expect(open(key, b64(modified), aad)).rejects.toThrow()
    expect(await seal(key, plaintext, aad)).not.toBe(cipher)
  })
  it('seals a root to a device key under a generation-bound context', async () => {
    const seed = random(),
      publicKey = await hpkePublic(seed),
      root = random()
    const context = deviceRootContext('local', 'account', 2, 'device')
    const envelope = await hpkeSeal(publicKey, root, context)
    expect(await hpkeOpen(seed, envelope, context)).toEqual(root)
    await expect(hpkeOpen(random(), envelope, context)).rejects.toThrow()
    await expect(
      hpkeOpen(seed, envelope, deviceRootContext('local', 'account', 3, 'device'))
    ).rejects.toThrow()
  })
  it('separates login, PRF and salt derivations per workspace', () => {
    const secret = random(),
      workspace = `dmsg:local:${'11'.repeat(32)}:${'22'.repeat(32)}`
    expect(unlockKey(secret, workspace)).toEqual(unlockKey(secret, workspace))
    expect(unlockKey(secret, workspace)).not.toEqual(unlockKey(secret, `${workspace}1`))
    expect(prfKey(secret, workspace)).not.toEqual(unlockKey(secret, workspace))
    expect(prfSalt(workspace)).not.toEqual(prfSalt(`${workspace}1`))
    expect(() => unlockKey(secret.subarray(0, 31), workspace)).toThrow()
  })
})
