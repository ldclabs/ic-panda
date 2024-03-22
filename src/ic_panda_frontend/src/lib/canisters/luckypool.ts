import {
  idlFactory,
  type AirdropClaimInput,
  type AirdropHarvestInput,
  type AirdropStateOutput,
  type CaptchaOutput,
  type LuckyDrawInput,
  type Notification,
  type LuckyDrawOutput as _LuckyDrawOutput,
  type _SERVICE,
  type State as _State
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'
import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
import { asyncFactory } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export type State = _State
export type AirdropState = AirdropStateOutput
export type Captcha = CaptchaOutput
export type LuckyDrawOutput = _LuckyDrawOutput

export class LuckyPoolAPI {
  principal: Principal
  actor: _SERVICE
  private _state = writable<State | null>(null)
  private _airdropState = writable<AirdropState | null>(null)

  static async with(identity: Identity): Promise<LuckyPoolAPI> {
    const actor = await createActor<_SERVICE>({
      canisterId: LUCKYPOOL_CANISTER_ID,
      idlFactory: idlFactory,
      identity
    })

    const api = new LuckyPoolAPI(identity.getPrincipal(), actor)
    await api.refreshAllState()
    return api
  }

  constructor(principal: Principal, actor: _SERVICE) {
    this.principal = principal
    this.actor = actor
  }

  get stateStore(): Readable<State | null> {
    return readonly(this._state)
  }

  get airdropStateStore(): Readable<AirdropState | null> {
    return readonly(this._airdropState)
  }

  async apiVersion(): Promise<number> {
    return this.actor.api_version()
  }

  async refreshAllState(): Promise<void> {
    const state = await this.actor.state()
    this._state.set(unwrapResult(state, 'call state failed'))

    const airdropState = await this.actor.airdrop_state_of([])
    this._airdropState.set(
      unwrapResult(airdropState, 'call airdrop_state_of failed')
    )
  }

  async notifications(): Promise<Notification[]> {
    return this.actor.notifications()
  }

  async captcha(): Promise<Captcha> {
    const res = await this.actor.captcha()
    return unwrapResult(res, 'call captcha failed')
  }

  async airdrop(input: AirdropClaimInput): Promise<AirdropState> {
    const res = await this.actor.airdrop(input)
    return unwrapResult(res, 'call airdrop failed')
  }

  async harvest(input: AirdropHarvestInput): Promise<AirdropState> {
    const res = await this.actor.harvest(input)
    return unwrapResult(res, 'call harvest failed')
  }

  async luckydraw(input: LuckyDrawInput): Promise<LuckyDrawOutput> {
    const res = await this.actor.luckydraw(input)
    return unwrapResult(res, 'call luckydraw failed')
  }
}

const luckyPoolAPIStore = asyncFactory((identity) =>
  LuckyPoolAPI.with(identity)
)

export const luckyPoolAPIAsync = async () => (await luckyPoolAPIStore).async()
