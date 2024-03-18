import { TOKEN_LEDGER_CANISTER_ID } from '$lib/constants'
import { createActor } from './actors'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { TokenAmount } from '$lib/utils/token'
import {
  idlFactory,
  type _SERVICE
} from '$declarations/icrc1_ledger_canister/icrc1_ledger_canister.did.js'
import { unwrapResult } from '$lib/types/result'
import { asyncFactory } from '$lib/stores/auth'

export class TokenLedgerAPI {
  principal: Principal
  actor: _SERVICE

  static async with(identity: Identity): Promise<TokenLedgerAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: TOKEN_LEDGER_CANISTER_ID,
      idlFactory: idlFactory,
      identity
    })

    return new TokenLedgerAPI(identity.getPrincipal(), actor)
  }

  constructor(principal: Principal, actor: _SERVICE) {
    this.principal = principal
    this.actor = actor
  }

  async balance(): Promise<bigint> {
    return this.getBalanceOf(this.principal)
  }

  async getBalanceOf(owner: Principal): Promise<bigint> {
    return this.actor.icrc1_balance_of({ owner, subaccount: [] })
  }

  async transfer(to: string, amount: TokenAmount): Promise<bigint> {
    const principal = Principal.fromText(to)
    const res = await this.actor.icrc1_transfer({
      from_subaccount: [],
      to: { owner: principal, subaccount: [] },
      amount: amount.toUlps(),
      fee: [],
      memo: [],
      created_at_time: [BigInt(Date.now() * 1_000_000)]
    })

    return unwrapResult(res, 'call icrc1_transfer failed')
  }
}

export const tokenLedgerAPIStore = asyncFactory((identity) =>
  TokenLedgerAPI.with(identity)
)
