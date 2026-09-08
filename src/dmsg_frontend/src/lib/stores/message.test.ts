import { webcrypto } from 'node:crypto'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Ed25519KeyIdentity } from '@icp-sdk/core/identity'
import { dynAgent, IdentityEx } from '$lib/utils/auth'
import { MasterKey, MyMessageState } from './message'
import { MessageAgent } from './message_agent'

vi.mock('./kvstore', () => ({
  getProfile: vi.fn(),
  getUser: vi.fn(),
  setProfile: vi.fn(),
  setUser: vi.fn()
}))

// Synthetic keys only: exercise archive unlock without a canister or real user.
describe('legacy master-key compatibility', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it.each(['Local', 'ECDH'] as const)(
    'reads a cached %s key without upgrading a COSE-enabled account',
    async (kind) => {
      vi.stubGlobal('crypto', webcrypto)
      const identity = new IdentityEx(
        Ed25519KeyIdentity.generate(),
        Date.now() + 600_000
      )
      const principal = identity.getPrincipal()
      const iv = new Uint8Array(32).fill(7)
      const original = await MasterKey.from(
        principal,
        kind,
        null,
        'sample-password',
        principal.toText(),
        new Uint8Array(32).fill(8),
        Date.now() + 600_000,
        iv
      )
      const agent = {
        principal,
        id: principal.toText(),
        hasCOSE: true,
        getMasterKeys: vi.fn().mockResolvedValue([original.toInfo()]),
        setMasterKeys: vi.fn(),
        coseAPI: { setting_try_get: vi.fn().mockResolvedValue(null) }
      }
      vi.spyOn(MessageAgent, 'create').mockResolvedValue(
        agent as unknown as MessageAgent
      )
      const fetchNewKey = vi.spyOn(
        MyMessageState.prototype,
        'fetchOrInitMasterKeyWithVetkey'
      )
      dynAgent.setIdentity(identity)
      const state = await MyMessageState.load()
      const restored = await state.masterKey(iv)
      expect(restored?.kind).toBe(kind)
      expect(restored?.toA256GCMKey().getSecretKey()).toEqual(
        original.toA256GCMKey().getSecretKey()
      )
      expect(state.isReady2()).toBe(true)
      expect(fetchNewKey).toHaveBeenCalledWith(iv, true)
      expect(agent.setMasterKeys).not.toHaveBeenCalled()
    }
  )

  it('does not initialize a missing remote vault key', async () => {
    const identity = new IdentityEx(
      Ed25519KeyIdentity.generate(),
      Date.now() + 600_000
    )
    const principal = identity.getPrincipal()
    const setting = vi.fn().mockResolvedValue(null)
    const agent = {
      principal,
      id: principal.toText(),
      hasCOSE: true,
      getMasterKeys: vi.fn().mockResolvedValue([]),
      setMasterKeys: vi.fn(),
      coseAPI: { setting_try_get: setting }
    }
    vi.spyOn(MessageAgent, 'create').mockResolvedValue(
      agent as unknown as MessageAgent
    )
    const initialize = vi.spyOn(
      MyMessageState.prototype,
      'initMasterKeyWithVetkey'
    )
    dynAgent.setIdentity(identity)
    const state = await MyMessageState.load()
    await expect(state.masterKey(new Uint8Array(32))).resolves.toBeNull()
    expect(setting).toHaveBeenCalledOnce()
    expect(initialize).not.toHaveBeenCalled()
    expect(agent.setMasterKeys).not.toHaveBeenCalled()
    expect(state.isReady2()).toBe(false)
  })
})
