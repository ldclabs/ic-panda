import {
  idlFactory,
  type BlockRef,
  type CreateTxInput,
  type CreateTxOutput,
  type SendTxInput,
  type SendTxOutput,
  type State,
  type UnspentTx,
  type UtxosOutput,
  type _SERVICE
} from '$declarations/ck-doge-canister/ck-doge-canister.did.js'
import { CKDOGE_CHAIN_CANISTER_ID } from '$lib/constants'
import { asyncFactory } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export {
  type BlockRef,
  type CreateTxInput,
  type CreateTxOutput,
  type SendTxInput,
  type SendTxOutput,
  type State,
  type UnspentTx,
  type Utxo,
  type UtxosOutput
} from '$declarations/ck-doge-canister/ck-doge-canister.did.js'

export class CKDogeCanisterAPI {
  readonly principal: Principal
  readonly canisterId: Principal
  private actor: _SERVICE
  private _state = writable<State | null>(null)

  static async with(identity: Identity): Promise<CKDogeCanisterAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: CKDOGE_CHAIN_CANISTER_ID,
      idlFactory: idlFactory,
      identity
    })

    const api = new CKDogeCanisterAPI(identity.getPrincipal(), actor)
    await api.refreshState()
    return api
  }

  constructor(principal: Principal, actor: _SERVICE) {
    this.principal = principal
    this.canisterId = Principal.fromText(CKDOGE_CHAIN_CANISTER_ID)
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
    this._state.set(unwrapResult(state, 'call get_state failed'))
  }

  async getAddress(): Promise<string> {
    const res = await this.actor.get_address()
    return unwrapResult(res, 'call get_address failed')
  }

  async getBalance(addr: string): Promise<bigint> {
    const res = await this.actor.get_balance(addr)
    return unwrapResult(res, 'call get_balance failed')
  }

  async getTip(addr: string): Promise<BlockRef> {
    const res = await this.actor.get_tip()
    return unwrapResult(res, 'call get_tip failed')
  }

  async getUtx(txid: string): Promise<UnspentTx> {
    const res = await this.actor.get_utx(txid)
    return unwrapResult(res, 'call get_utx failed')
  }

  async listUtxos(addr: string): Promise<UtxosOutput> {
    const res = await this.actor.list_utxos(addr, 100, false)
    return unwrapResult(res, 'call list_utxos failed')
  }

  async signAndSendTx(input: SendTxInput): Promise<SendTxOutput> {
    const res = await this.actor.sign_and_send_tx(input)
    return unwrapResult(res, 'call sign_and_send_tx failed')
  }

  async createTx(input: CreateTxInput): Promise<CreateTxOutput> {
    const res = await this.actor.create_tx(input)
    return unwrapResult(res, 'call create_tx failed')
  }
}

const ckDogeCanisterAPIStore = asyncFactory((identity) =>
  CKDogeCanisterAPI.with(identity)
)

export const ckDogeCanisterAPIAsync = async () =>
  (await ckDogeCanisterAPIStore).async()
