import {
  idlFactory,
  type _SERVICE,
  type Allowance
} from '$declarations/icrc1_ledger_canister/icrc1_ledger_canister.did.js'
import { unwrapResult } from '$lib/types/result'
import { dynAgent } from '$lib/utils/auth'
import { PANDAToken, type TokenInfo } from '$lib/utils/token'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export class TokenLedgerAPI {
  readonly canisterId: Principal
  readonly token: TokenInfo
  private actor: _SERVICE

  constructor(token: TokenInfo) {
    this.canisterId = Principal.fromText(token.canisterId)
    this.actor = createActor<_SERVICE>({
      canisterId: token.canisterId,
      idlFactory: idlFactory
    })
    this.token = token
  }

  async balance(): Promise<bigint> {
    return this.getBalanceOf(dynAgent.id.getPrincipal())
  }

  async getBalanceOf(owner: Principal): Promise<bigint> {
    return this.actor.icrc1_balance_of({ owner, subaccount: [] })
  }

  async allowance(spender: Principal): Promise<Allowance> {
    return this.actor.icrc2_allowance({
      account: { owner: dynAgent.id.getPrincipal(), subaccount: [] },
      spender: { owner: spender, subaccount: [] }
    })
  }

  async approve(spender: Principal, amount: bigint): Promise<bigint> {
    const res = await this.actor.icrc2_approve({
      fee: [],
      memo: [],
      from_subaccount: [],
      created_at_time: [BigInt(Date.now() * 1_000_000)],
      amount: amount,
      expected_allowance: [],
      expires_at: [],
      spender: { owner: spender, subaccount: [] }
    })

    return unwrapResult(res, 'call icrc2_approve failed')
  }

  async ensureAllowance(spender: Principal, amount: bigint): Promise<void> {
    const allowance = await this.allowance(spender)
    const expires_at = allowance.expires_at[0] || 0n
    if (
      allowance.allowance < amount ||
      (expires_at > 0 && expires_at < BigInt((Date.now() + 60000) * 1_000_000))
    ) {
      // Approve 10 times of the amount to avoid frequent approval
      await this.approve(spender, amount)
    }
  }

  async transfer(to: string, amount: bigint): Promise<bigint> {
    const principal = Principal.fromText(to)
    const res = await this.actor.icrc1_transfer({
      from_subaccount: [],
      to: { owner: principal, subaccount: [] },
      amount,
      fee: [],
      memo: [],
      created_at_time: [BigInt(Date.now() * 1_000_000)]
    })

    return unwrapResult(res, 'call icrc1_transfer failed')
  }
}

export const tokenLedgerAPI = new TokenLedgerAPI(PANDAToken)
