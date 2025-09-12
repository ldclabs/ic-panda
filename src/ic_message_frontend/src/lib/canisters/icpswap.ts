import {
  idlFactory,
  type _SERVICE,
  type PublicTokenOverview
} from '$declarations/icpswap_tokens/icpswap_tokens.did.js'
import { ICPSWAP_TOKENS_CANISTER_ID } from '$lib/constants'
import { anonAgent } from '$lib/utils/auth'
import { Principal } from '@dfinity/principal'
import { createActor } from './actors'

export { type PublicTokenOverview } from '$declarations/icpswap_tokens/icpswap_tokens.did.js'

export class ICPSwapAPI {
  readonly canisterId: Principal
  private actor: _SERVICE

  constructor(canisterId: Principal) {
    this.canisterId = canisterId
    this.actor = createActor<_SERVICE>({
      canisterId: canisterId,
      idlFactory: idlFactory,
      agent: anonAgent
    })
  }

  // record {
  //   id = 20_066 : nat;
  //   volumeUSD1d = 0.23559134662449308 : float64;
  //   volumeUSD7d = 401.21375699473487 : float64;
  //   totalVolumeUSD = 396748.7304150212 : float64;
  //   name = "PANDA";
  //   volumeUSD = 401.21375699473487 : float64;
  //   feesUSD = 0.0007067740398734793 : float64;
  //   priceUSDChange = -3.508235689470593 : float64;
  //   address = "druyg-tyaaa-aaaaq-aactq-cai";
  //   txCount = 7_705 : int;
  //   priceUSD = 0.0010631241028960998 : float64;
  //   standard = "ICRC1";
  //   symbol = "PANDA";
  // }
  async getToken(ledger: string): Promise<PublicTokenOverview | null> {
    try {
      return await this.actor.getToken(ledger)
    } catch (e) {
      return null
    }
  }
}

export const icpSwapAPI = new ICPSwapAPI(
  Principal.fromText(ICPSWAP_TOKENS_CANISTER_ID)
)

// https://uvevg-iyaaa-aaaak-ac27q-cai.raw.ic0.app/
