import {
  idlFactory,
  type AddPrizeInputV2,
  type AirdropClaimInput,
  type AirdropHarvestInput,
  type AirdropStateOutput,
  type Airdrops108Output,
  type CaptchaOutput,
  type ClaimPrizeInput,
  type ClaimPrizeOutput,
  type LuckyDrawInput,
  type LuckyDrawLog,
  type LuckyDrawOutput,
  type NameOutput,
  type Notification,
  type PrizeClaimLog,
  type PrizeOutput,
  type State,
  type _SERVICE
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'
import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
import { unwrapOptionResult, unwrapResult } from '$lib/types/result'
import { dynAgent } from '$lib/utils/auth'
import type { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export type AirdropState = AirdropStateOutput
export type Captcha = CaptchaOutput

export {
  type AddPrizeInputV2,
  type Airdrops108Output,
  type ClaimPrizeOutput,
  type LuckyDrawOutput,
  type NameOutput,
  type PrizeClaimLog,
  type PrizeOutput,
  type State
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'

export class LuckyPoolAPI {
  private actor: _SERVICE
  private _state = writable<State | null>(null)
  private _airdropState = writable<AirdropState | null>(null)
  private _nameState = writable<NameOutput | null>(null)

  constructor() {
    this.actor = createActor<_SERVICE>({
      canisterId: LUCKYPOOL_CANISTER_ID,
      idlFactory: idlFactory
    })
  }

  get stateStore(): Readable<State | null> {
    return readonly(this._state)
  }

  get airdropStateStore(): Readable<AirdropState | null> {
    return readonly(this._airdropState)
  }

  get nameStateStore(): Readable<NameOutput | null> {
    return readonly(this._nameState)
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

  async refreshNameState(): Promise<void> {
    const nameState = await this.nameOf()
    this._nameState.set(nameState)
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

  async airdrops108Of(user: Principal): Promise<Airdrops108Output | null> {
    const res = await this.actor.airdrops108_of([user])
    return unwrapResult(res, 'call airdrops108_of failed')
  }

  async claimPrize(input: ClaimPrizeInput): Promise<ClaimPrizeOutput> {
    const res = await this.actor.claim_prize(input)
    return unwrapResult(res, 'call claim_prize failed')
  }

  async createPrize(input: AddPrizeInputV2): Promise<PrizeOutput> {
    const res = await this.actor.add_prize(input)
    return unwrapResult(res, 'call add_prize failed')
  }

  async prizeInfo(code: string): Promise<PrizeOutput> {
    const res = await this.actor.prize_info(code, [dynAgent.id.getPrincipal()])
    return unwrapResult(res, 'call prize_info failed')
  }

  async prizeClaimLogs(
    prev: bigint,
    take: bigint
  ): Promise<Array<PrizeClaimLog>> {
    const res = await this.actor.prize_claim_logs(
      dynAgent.id.getPrincipal(),
      prev > 0n ? [prev] : [],
      take > 0n ? [take] : []
    )
    return res
  }

  async prizeIssueLogs(prev_ts: bigint): Promise<Array<PrizeOutput>> {
    const res = await this.actor.prize_issue_logs(
      dynAgent.id.getPrincipal(),
      prev_ts > 0n ? [prev_ts] : []
    )
    return res
  }

  async harvest(input: AirdropHarvestInput): Promise<AirdropState> {
    const res = await this.actor.harvest(input)
    return unwrapResult(res, 'call harvest failed')
  }

  async luckydraw(input: LuckyDrawInput): Promise<LuckyDrawOutput> {
    const res = await this.actor.luckydraw(input)
    return unwrapResult(res, 'call luckydraw failed')
  }

  async myLuckydrawLogs(): Promise<LuckyDrawLog[]> {
    return this.actor.my_luckydraw_logs([], [20n])
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
