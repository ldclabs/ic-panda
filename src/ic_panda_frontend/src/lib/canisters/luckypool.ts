import {
  idlFactory,
  type Airdrops108Output,
  type NameOutput,
  type Notification,
  type _SERVICE
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'
import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
import { unwrapOptionResult, unwrapResult } from '$lib/types/result'
import type { Principal } from '@icp-sdk/core/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export {
  type Airdrops108Output,
  type NameOutput
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'

export class LuckyPoolAPI {
  private actor: _SERVICE
  private _nameState = writable<NameOutput | null>(null)

  constructor() {
    this.actor = createActor<_SERVICE>({
      canisterId: LUCKYPOOL_CANISTER_ID,
      idlFactory: idlFactory
    })
  }

  get nameStateStore(): Readable<NameOutput | null> {
    return readonly(this._nameState)
  }

  async apiVersion(): Promise<number> {
    return this.actor.api_version()
  }

  async refreshNameState(): Promise<void> {
    const nameState = await this.nameOf()
    this._nameState.set(nameState)
  }

  async notifications(): Promise<Notification[]> {
    return this.actor.notifications()
  }

  async airdrops108Of(user: Principal): Promise<Airdrops108Output | null> {
    const res = await this.actor.airdrops108_of([user])
    return unwrapResult(res, 'call airdrops108_of failed')
  }

  async nameOf(): Promise<NameOutput | null> {
    const res = await this.actor.name_of([])
    return unwrapOptionResult(res, 'call name_of failed')
  }

  async nameLookup(name: string): Promise<NameOutput | null> {
    const res = await this.actor.name_lookup(name)
    return unwrapOptionResult(res, 'call name_lookup failed')
  }

  async registerName(name: string): Promise<NameOutput> {
    const res = await this.actor.register_name({ name, old_name: [] })
    const nameState: NameOutput = unwrapResult(res, 'call register_name failed')
    this._nameState.set(nameState)
    return nameState
  }

  async unregisterName(name: string): Promise<bigint> {
    const res = await this.actor.unregister_name({ name, old_name: [] })
    const refund: bigint = unwrapResult(res, 'call unregister_name failed')
    this._nameState.set(null)
    return refund
  }

  async updateName(name: string, old_name: string): Promise<NameOutput> {
    const res = await this.actor.update_name({ name, old_name: [old_name] })
    const nameState: NameOutput = unwrapResult(res, 'call update_name failed')
    this._nameState.set(nameState)
    return nameState
  }
}

export const luckyPoolAPI = new LuckyPoolAPI()
