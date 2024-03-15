import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
import { createActor } from './actors'
import type { OptionIdentity } from '$lib/types/identity'
import { isNullish } from '@dfinity/utils'
import {
  idlFactory,
  type Notification,
  type _SERVICE
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'
import { AnonymousIdentity } from '@dfinity/agent'

let actor: _SERVICE | null = null

export const getLuckyPoolService = async ({
  identity
}: {
  identity: OptionIdentity
}): Promise<_SERVICE> => {
  if (isNullish(actor)) {
    actor = await createActor<_SERVICE>({
      canisterId: LUCKYPOOL_CANISTER_ID,
      idlFactory: idlFactory,
      identity: identity || new AnonymousIdentity()
    })
  }

  return actor
}

export class LuckyPoolAPI {
  actor: _SERVICE

  constructor(actor: _SERVICE) {
    this.actor = actor
  }

  async api_version(): Promise<number> {
    return this.actor.api_version()
  }

  async notifications(): Promise<Notification[]> {
    return this.actor.notifications()
  }
}
