import { ICP_LEDGER_CANISTER_ID } from '$lib/constants'
import { createActor } from './actors'
import type { OptionIdentity } from '$lib/types/identity'
import { assertNonNullish, isNullish } from '@dfinity/utils'
import { Principal } from '@dfinity/principal'
import {
  idlFactory,
  type _SERVICE
} from '$declarations/icp_ledger_canister/icp_ledger_canister.did.js'

let actor: _SERVICE | null = null

export const getICPLedgerService = async ({
  identity
}: {
  identity: OptionIdentity
}): Promise<_SERVICE> => {
  assertNonNullish(identity, 'No internet identity.')

  if (isNullish(actor)) {
    actor = await createActor<_SERVICE>({
      canisterId: ICP_LEDGER_CANISTER_ID,
      idlFactory: idlFactory,
      identity
    })
  }

  return actor
}

export class ICPLedgerAPI {
  actor: _SERVICE

  constructor(actor: _SERVICE) {
    this.actor = actor
  }

  async getBalanceOf(owner: Principal): Promise<bigint> {
    return this.actor.icrc1_balance_of({ owner })
  }
}
