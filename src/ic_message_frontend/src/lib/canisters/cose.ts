import {
  idlFactory,
  type CreateSettingInput,
  type CreateSettingOutput,
  type ECDHInput,
  type ECDHOutput,
  type SettingInfo,
  type SettingPath,
  type _SERVICE
} from '$declarations/ic_cose_canister/ic_cose_canister.did.js'
import { unwrapResult } from '$lib/types/result'
import { Principal } from '@dfinity/principal'
import {
  DerivedPublicKey,
  EncryptedVetKey,
  TransportSecretKey,
  VetKey
} from '@dfinity/vetkeys'
import { createActor } from './actors'

export {
  type ECDHInput,
  type ECDHOutput,
  type SettingInfo,
  type SettingPath
} from '$declarations/ic_cose_canister/ic_cose_canister.did.js'
export { type DerivedPublicKey, type VetKey } from '@dfinity/vetkeys'

export class CoseAPI {
  readonly canisterId: Principal
  private actor: _SERVICE

  constructor(canister: Principal) {
    this.canisterId = canister
    this.actor = createActor<_SERVICE>({
      canisterId: canister,
      idlFactory: idlFactory
    })
  }

  async ecdh_cose_encrypted_key(
    path: SettingPath,
    ecdh: ECDHInput
  ): Promise<ECDHOutput> {
    const res = await this.actor.ecdh_cose_encrypted_key(path, ecdh)
    return unwrapResult(res, 'call ecdh_cose_encrypted_key failed')
  }

  async vetkey(path: SettingPath): Promise<[VetKey, DerivedPublicKey]> {
    const tsk = TransportSecretKey.random()
    const tpk = tsk.publicKeyBytes()
    const [pk, ek] = await Promise.all([
      this.actor.vetkd_public_key(path),
      this.actor.vetkd_encrypted_key(path, tpk)
    ])
    const dpk = DerivedPublicKey.deserialize(
      unwrapResult(pk, 'call vetkd_public_key failed') as Uint8Array
    )
    const evk = new EncryptedVetKey(
      unwrapResult(ek, 'call vetkd_encrypted_key failed') as Uint8Array
    )
    const vk = evk.decryptAndVerify(tsk, dpk, path.key as Uint8Array)
    return [vk, dpk]
  }

  async setting_get(path: SettingPath): Promise<SettingInfo> {
    const res = await this.actor.setting_get(path)
    return unwrapResult(res, 'call setting_get failed')
  }

  async setting_create(
    path: SettingPath,
    input: CreateSettingInput
  ): Promise<CreateSettingOutput> {
    const res = await this.actor.setting_create(path, input)
    return unwrapResult(res, 'call setting_create failed')
  }
}
