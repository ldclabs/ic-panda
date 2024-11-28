import {
  idlFactory,
  type _SERVICE,
  type Allowance
} from '$declarations/icrc1_ledger_canister/icrc1_ledger_canister.did.js'
import { agent } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
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

  static async fromID(canisterId: Principal): Promise<TokenLedgerAPI> {
    const actor = createActor<_SERVICE>({
      canisterId,
      idlFactory: idlFactory
    })
    const metadata = await actor.icrc1_metadata()
    const token: TokenInfo = {
      name: 'Internet Computer',
      symbol: 'ICP',
      decimals: 8,
      fee: 10000n,
      one: 100000000n,
      logo: '',
      canisterId: canisterId.toText()
    }

    for (const [key, value] of metadata) {
      switch (key) {
        case 'icrc1:name':
          token.name = (value as { 'Text': string }).Text
          continue
        case 'icrc1:symbol':
          token.symbol = (value as { 'Text': string }).Text
          continue
        case 'icrc1:decimals':
          const decimals = (value as { 'Nat': bigint }).Nat
          token.decimals = Number(decimals)
          token.one = 10n ** decimals
          continue
        case 'icrc1:fee':
          token.fee = (value as { 'Nat': bigint }).Nat
          continue
        case 'icrc1:logo':
          token.logo = (value as { 'Text': string }).Text
          continue
      }
    }

    return new TokenLedgerAPI(token)
  }

  async balance(): Promise<bigint> {
    return this.getBalanceOf(agent.id.getPrincipal())
  }

  async getBalanceOf(owner: Principal): Promise<bigint> {
    return this.actor.icrc1_balance_of({ owner, subaccount: [] })
  }

  async allowance(spender: Principal): Promise<Allowance> {
    return this.actor.icrc2_allowance({
      account: { owner: agent.id.getPrincipal(), subaccount: [] },
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

export const pandaLedgerAPI = new TokenLedgerAPI(PANDAToken)
