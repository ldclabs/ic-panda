import { beforeEach, describe, expect, it, vi } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'

const actor = vi.hoisted(() => ({
  setting_get: vi.fn(),
  setting_create: vi.fn()
}))
vi.mock('./actors', () => ({ createActor: () => actor }))
import { CoseAPI, type SettingPath } from './cose'

const path: SettingPath = {
  ns: 'archive',
  key: new Uint8Array([1]),
  subject: [],
  version: 0,
  user_owned: true
}

describe('historical COSE reads', () => {
  beforeEach(() => vi.resetAllMocks())

  it('reads a historical namespace without creating a replacement key', async () => {
    const existing = { version: 7, dek: [new Uint8Array([1, 2, 3])] }
    actor.setting_get
      .mockResolvedValueOnce({ Err: 'NotFound' })
      .mockResolvedValueOnce({ Ok: existing })
    const api = new CoseAPI(Principal.anonymous())
    await expect(api.setting_get_legacy(path)).resolves.toBe(existing)
    expect(actor.setting_get).toHaveBeenNthCalledWith(2, {
      ...path,
      user_owned: false
    })
    expect(actor.setting_create).not.toHaveBeenCalled()
  })

  it('surfaces a missing key instead of initializing an empty archive', async () => {
    actor.setting_get.mockResolvedValue({ Err: 'NotFound' })
    const api = new CoseAPI(Principal.anonymous())
    await expect(api.setting_get_legacy(path)).rejects.toThrow()
    expect(actor.setting_create).not.toHaveBeenCalled()
  })

  it('does not treat a service failure as a missing historical key', async () => {
    actor.setting_get.mockRejectedValue(new Error('Service unavailable'))
    const api = new CoseAPI(Principal.anonymous())
    await expect(api.setting_get_legacy(path)).rejects.toThrow(
      'Service unavailable'
    )
    expect(actor.setting_get).toHaveBeenCalledTimes(1)
    expect(actor.setting_create).not.toHaveBeenCalled()
  })
})
