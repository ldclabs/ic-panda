import { ICP_LEDGER_CANISTER_ID } from '$lib/constants'
import { createActor } from './actors'
import type { OptionIdentity } from '$lib/types/identity'
import { assertNonNullish, isNullish } from '@dfinity/utils'
import {
  idlFactory,
  type _SERVICE
} from '$declarations/icp_ledger_canister/icp_ledger_canister.did'

let actor: _SERVICE | null = null

export const getService = async ({
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
