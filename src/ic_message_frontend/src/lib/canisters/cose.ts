import {
  idlFactory,
  type ECDHInput,
  type ECDHOutput,
  type SettingInfo,
  type SettingPath,
  type _SERVICE
} from '$declarations/ic_cose_canister/ic_cose_canister.did.js'
import { unwrapResult } from '$lib/types/result'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export {
  type ECDHInput,
  type ECDHOutput,
  type SettingInfo,
  type SettingPath
} from '$declarations/ic_cose_canister/ic_cose_canister.did.js'

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

  async setting_get(path: SettingPath): Promise<SettingInfo> {
    const res = await this.actor.setting_get(path)
    return unwrapResult(res, 'call setting_get failed')
  }
}
