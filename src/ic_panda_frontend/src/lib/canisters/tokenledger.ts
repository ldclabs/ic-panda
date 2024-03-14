import { TOKEN_LEDGER_CANISTER_ID } from '$lib/constants'
import { createActor } from './actors'
import type { OptionIdentity } from '$lib/types/identity'
import { assertNonNullish, isNullish } from '@dfinity/utils'
import { Principal } from '@dfinity/principal'
import { TokenAmount } from '$lib/utils/token'
import {
  idlFactory,
  type _SERVICE
} from '$declarations/icrc1_ledger_canister/icrc1_ledger_canister.did.js'
import { unwrapResult } from '$lib/types/result'

let actor: _SERVICE | null = null

export const getTokenLedgerService = async ({
  identity
}: {
  identity: OptionIdentity
}): Promise<_SERVICE> => {
  assertNonNullish(identity, 'No internet identity.')

  if (isNullish(actor)) {
    actor = await createActor<_SERVICE>({
      canisterId: TOKEN_LEDGER_CANISTER_ID,
      idlFactory: idlFactory,
      identity
    })
  }

  return actor
}

export class TokenLedgerAPI {
  actor: _SERVICE

  constructor(actor: _SERVICE) {
    this.actor = actor
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

    return unwrapResult(res, 'icrc1_transfer failed')
  }
}
