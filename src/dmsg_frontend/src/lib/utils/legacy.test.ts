import { afterEach, describe, expect, it, vi } from 'vitest'
import { AnonymousIdentity, HttpAgent } from '@icp-sdk/core/agent'
import { AuthAgent } from './auth'

// These tests exercise the transport boundary, including update-method recovery.
// They do not call any deployed canister.
describe('legacy archive transport', () => {
  afterEach(() => vi.restoreAllMocks())

  it('rejects business writes before they reach the network', async () => {
    const submit = vi
      .spyOn(HttpAgent.prototype, 'call')
      .mockResolvedValue({} as never)
    const agent = new AuthAgent({
      identity: new AnonymousIdentity(),
      shouldFetchRootKey: false
    })
    for (const methodName of [
      'add_message',
      'upload_file_token',
      'update_my_setting',
      'register_username',
      'transfer_username',
      'update_profile',
      'setting_create',
      'setting_update_payload',
      'icrc1_transfer',
      'icrc2_approve',
      'add_delegator',
      'unknown_future_write'
    ]) {
      await expect(
        agent.call('aaaaa-aa', { methodName, arg: new Uint8Array() })
      ).rejects.toThrow('read-only')
    }
    expect(submit).not.toHaveBeenCalled()
  })

  it('preserves existing authentication, decryption and download update methods', async () => {
    const result = { requestId: new Uint8Array([1]) }
    const submit = vi
      .spyOn(HttpAgent.prototype, 'call')
      .mockResolvedValue(result as never)
    const agent = new AuthAgent({
      identity: new AnonymousIdentity(),
      shouldFetchRootKey: false
    })
    for (const methodName of [
      'my_iv',
      'sign_in',
      'ecdh_cose_encrypted_key',
      'vetkd_public_key',
      'vetkd_encrypted_key',
      'download_files_token'
    ]) {
      const options = { methodName, arg: new Uint8Array([1, 2]) }
      await expect(agent.call('aaaaa-aa', options)).resolves.toBe(result)
      expect(submit).toHaveBeenLastCalledWith('aaaaa-aa', options)
    }
  })
})
