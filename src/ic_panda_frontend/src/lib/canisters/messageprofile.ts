import {
  idlFactory,
  type ProfileInfo,
  type UpdateProfileInput,
  type _SERVICE
} from '$declarations/ic_message_profile/ic_message_profile.did.js'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export class ProfileAPI {
  readonly principal: Principal
  readonly canisterId: Principal
  private actor: _SERVICE

  static async with(
    identity: Identity,
    canister: Principal
  ): Promise<ProfileAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: canister,
      idlFactory: idlFactory,
      identity
    })

    const api = new ProfileAPI(identity.getPrincipal(), canister, actor)
    return api
  }

  constructor(principal: Principal, canister: Principal, actor: _SERVICE) {
    this.principal = principal
    this.canisterId = canister
    this.actor = actor
  }

  async get_profile(id: Principal): Promise<ProfileInfo> {
    const res = await this.actor.get_profile([id])
    return unwrapResult(res, 'call get_profile failed')
  }

  async update_profile(input: UpdateProfileInput): Promise<ProfileInfo> {
    const res = await this.actor.update_profile(input)
    return unwrapResult(res, 'call update_profile failed')
  }
}
