import { describe, expect, it } from 'vitest'
import {
  seal,
  open,
  hpkeSeal,
  hpkeOpen,
  hpkePublic,
  recoverySeeds,
  passwordKey,
  defaultKdf
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
  it('recovers future roots using only the offline recovery seed', async () => {
    const seeds = recoverySeeds(random(), 'local', 'subject', 1),
      publicKey = await hpkePublic(seeds.hpke),
      root = random()
    const envelope = await hpkeSeal(publicKey, root, ['root', 2])
    expect(await hpkeOpen(seeds.hpke, envelope, ['root', 2])).toEqual(root)
    await expect(hpkeOpen(random(), envelope, ['root', 2])).rejects.toThrow()
    await expect(hpkeOpen(seeds.hpke, envelope, ['root', 3])).rejects.toThrow()
    expect(seeds.signing).not.toEqual(seeds.hpke)
  })
  it('rejects imported KDF work factors before allocating memory', async () => {
    await expect(
      passwordKey('sample password', { ...defaultKdf(), memory: 1024 * 1024 })
    ).rejects.toThrow()
    await expect(
      passwordKey('sample password', { ...defaultKdf(), iterations: 1 })
    ).rejects.toThrow()
  })
})
