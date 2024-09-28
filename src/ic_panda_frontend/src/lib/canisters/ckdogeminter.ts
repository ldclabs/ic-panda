import {
  idlFactory,
  type BurnInput,
  type BurnOutput,
  type MintOutput,
  type MintedUtxo,
  type State,
  type _SERVICE
} from '$declarations/ck-doge-minter/ck-doge-minter.did.js'
import { CKDOGE_MINTER_CANISTER_ID } from '$lib/constants'
import { asyncFactory } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export class CKDogeMinterAPI {
  readonly principal: Principal
  readonly canisterId: Principal
  private actor: _SERVICE
  private _state = writable<State | null>(null)

  static async with(identity: Identity): Promise<CKDogeMinterAPI> {
    const actor = createActor<_SERVICE>({
      canisterId: CKDOGE_MINTER_CANISTER_ID,
      idlFactory: idlFactory
    })

    const api = new CKDogeMinterAPI(identity.getPrincipal(), actor)
    await api.refreshState()
    return api
  }

  constructor(principal: Principal, actor: _SERVICE) {
    this.principal = principal
    this.canisterId = Principal.fromText(CKDOGE_MINTER_CANISTER_ID)
    this.actor = actor
  }

  get stateStore(): Readable<State | null> {
    return readonly(this._state)
  }

  async apiVersion(): Promise<number> {
    return this.actor.api_version()
  }

  async refreshState(): Promise<void> {
    const state = await this.actor.get_state()
    this._state.set(unwrapResult(state, 'call state failed'))
  }

  async getAddress(): Promise<string> {
    const res = await this.actor.get_address()
    return unwrapResult(res, 'call get_address failed')
  }

  async mintCKDoge(): Promise<MintOutput> {
    const res = await this.actor.mint_ckdoge()
    return unwrapResult(res, 'call mint_ckdoge failed')
  }

  async burnCKDoge(input: BurnInput): Promise<BurnOutput> {
    const res = await this.actor.burn_ckdoge(input)
    return unwrapResult(res, 'call burn_ckdoge failed')
  }

  async list_minted_utxos(): Promise<Array<MintedUtxo>> {
    const res = await this.actor.list_minted_utxos([])
    return unwrapResult(res, 'call list_minted_utxos failed')
  }
}

const ckDogeMinterAPIStore = asyncFactory((identity) =>
  CKDogeMinterAPI.with(identity)
)

export const ckDogeMinterAPIAsync = async () =>
  (await ckDogeMinterAPIStore).async()
