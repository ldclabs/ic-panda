import {
  idlFactory,
  type Delegator,
  type NameAccount,
  type SignInResponse,
  type SignedDelegation,
  type _SERVICE
} from '$declarations/ic_name_identity/ic_name_identity.did.js'
import { NAME_IDENTITY_CANISTER_ID } from '$lib/constants'
import { unwrapResult } from '$lib/types/result'
import { AuthAgent } from '$lib/utils/auth'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export type {
  Delegator,
  NameAccount
} from '$declarations/ic_name_identity/ic_name_identity.did.js'

const canisterId = Principal.fromText(NAME_IDENTITY_CANISTER_ID)
export class NameIdentityAPI {
  readonly canisterId: Principal
  private actor: _SERVICE

  constructor(agent: AuthAgent) {
    this.canisterId = canisterId
    this.actor = createActor<_SERVICE>({
      canisterId: canisterId,
      idlFactory: idlFactory,
      agent
    })
  }

  async activate_name(username: string): Promise<Delegator[]> {
    const res = await this.actor.activate_name(username)
    return unwrapResult(res, 'call activate_name failed')
  }

  async add_delegator(
    username: string,
    user: Principal,
    role: -1 | 0 | 1
  ): Promise<Delegator[]> {
    const res = await this.actor.add_delegator(username, user, role)
    return unwrapResult(res, 'call add_delegator failed')
  }

  async remove_delegator(username: string, user: Principal): Promise<null> {
    const res = await this.actor.remove_delegator(username, user)
    return unwrapResult(res, 'call remove_delegator failed')
  }

  async get_delegators(username: string): Promise<Delegator[]> {
    const res = await this.actor.get_delegators(username)
    return unwrapResult(res, 'call get_delegators failed')
  }

  async get_my_accounts(): Promise<NameAccount[]> {
    const res = await this.actor.get_my_accounts()
    return unwrapResult(res, 'call get_my_accounts failed')
  }

  async get_principal(username: string): Promise<Principal> {
    const res = await this.actor.get_principal(username)
    return unwrapResult(res, 'call get_principal failed')
  }

  async leave_delegation(username: string): Promise<null> {
    const res = await this.actor.leave_delegation(username)
    return unwrapResult(res, 'call leave_delegation failed')
  }

  async sign_in(
    username: string,
    pubkey: Uint8Array,
    sig: Uint8Array
  ): Promise<SignInResponse> {
    const res = await this.actor.sign_in(username, pubkey, sig)
    return unwrapResult(res, 'call sign_in failed')
  }

  async get_delegation(
    seed: Uint8Array,
    pubkey: Uint8Array,
    expiration_ms: bigint
  ): Promise<SignedDelegation> {
    const res = await this.actor.get_delegation(seed, pubkey, expiration_ms)
    return unwrapResult(res, 'call get_delegation failed')
  }
}
