import {
  idlFactory,
  type ECDHInput,
  type ECDHOutput,
  type SettingInfo,
  type SettingPath,
  type _SERVICE
} from '$declarations/ic_cose_canister/ic_cose_canister.did.js'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export {
  type ECDHInput,
  type ECDHOutput,
  type SettingInfo,
  type SettingPath
} from '$declarations/ic_cose_canister/ic_cose_canister.did.js'

export class CoseAPI {
  readonly principal: Principal
  readonly canisterId: Principal
  private actor: _SERVICE

  static async with(identity: Identity, canister: Principal): Promise<CoseAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: canister,
      idlFactory: idlFactory,
      identity
    })

    const api = new CoseAPI(identity.getPrincipal(), canister, actor)
    return api
  }

  constructor(principal: Principal, canister: Principal, actor: _SERVICE) {
    this.principal = principal
    this.canisterId = canister
    this.actor = actor
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
