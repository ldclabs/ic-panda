import {
  idlFactory,
  type AirdropClaimInput,
  type AirdropHarvestInput,
  type AirdropStateOutput,
  type CaptchaOutput,
  type ClaimPrizeInput,
  type LuckyDrawInput,
  type LuckyDrawLog,
  type Notification,
  type AddPrizeInput as _AddPrizeInput,
  type ClaimPrizeOutput as _ClaimPrizeOutput,
  type LuckyDrawOutput as _LuckyDrawOutput,
  type NameOutput as _NameOutput,
  type PrizeClaimLog as _PrizeClaimLog,
  type PrizeOutput as _PrizeOutput,
  type _SERVICE,
  type State as _State
} from '$declarations/ic_panda_luckypool/ic_panda_luckypool.did.js'
import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
import { anonymousIdentity, asyncFactory } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
import type { Identity } from '@dfinity/agent'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export type State = _State
export type AirdropState = AirdropStateOutput
export type Captcha = CaptchaOutput
export type LuckyDrawOutput = _LuckyDrawOutput
export type NameOutput = _NameOutput
export type AddPrizeInput = _AddPrizeInput
export type PrizeOutput = _PrizeOutput
export type PrizeClaimLog = _PrizeClaimLog
export type ClaimPrizeOutput = _ClaimPrizeOutput

export class LuckyPoolAPI {
  principal: Principal
  actor: _SERVICE
  private _state = writable<State | null>(null)
  private _airdropState = writable<AirdropState | null>(null)
  private _nameState = writable<NameOutput | null>(null)

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

  async defaultAirdropState(): Promise<AirdropState> {
    const airdropState = await this.actor.airdrop_state_of([
      anonymousIdentity.getPrincipal()
    ])
    return unwrapResult(airdropState, 'call airdrop_state_of failed')
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

  async claimPrize(input: ClaimPrizeInput): Promise<ClaimPrizeOutput> {
    const res = await this.actor.claim_prize(input)
    return unwrapResult(res, 'call claim_prize failed')
  }

  async createPrize(input: AddPrizeInput): Promise<PrizeOutput> {
    const res = await this.actor.add_prize(input)
    return unwrapResult(res, 'call add_prize failed')
  }

  async prizeInfo(code: string): Promise<PrizeOutput> {
    const res = await this.actor.prize_info(code, [this.principal])
    return unwrapResult(res, 'call prize_info failed')
  }

  async prizeClaimLogs(
    prev: bigint,
    take: bigint
  ): Promise<Array<PrizeClaimLog>> {
    const res = await this.actor.prize_claim_logs(
      this.principal,
      prev > 0n ? [prev] : [],
      take > 0n ? [take] : []
    )
    return res
  }

  async prizeIssueLogs(prev_ts: bigint): Promise<Array<PrizeOutput>> {
    const res = await this.actor.prize_issue_logs(
      this.principal,
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
    return this.actor.my_luckydraw_logs([], [20])
  }

  async nameOf(): Promise<NameOutput | null> {
    const res = await this.actor.name_of([])
    return unwrapResult(res, 'call name_of failed', true)
  }

  async nameLookup(name: string): Promise<NameOutput | null> {
    const res = await this.actor.name_lookup(name)
    return unwrapResult(res, 'call name_lookup failed', true)
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

const luckyPoolAPIStore = asyncFactory((identity) =>
  LuckyPoolAPI.with(identity)
)

export const luckyPoolAPIAsync = async () => (await luckyPoolAPIStore).async()
