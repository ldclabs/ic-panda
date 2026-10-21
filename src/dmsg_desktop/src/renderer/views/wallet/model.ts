/** Wallet values crossing the daemon bridge. Atomic amounts always stay strings. */
export interface Asset {
  asset: string
  key?: string
  symbol?: string
  decimals?: number
  chain_id?: number
  ledger?: string
  token?: string
  mint?: string
  error?: string
  source?: string
}
export interface Member {
  kind?: string
  id: string
  role: string
  status: string
  label?: string
  agent_ids: string[]
  principals?: string[]
}
export interface Proposal {
  id: number
  status: string
  proposer: string
  via_agent?: string
  intent: Record<string, any>
  requirement: { approvals: number; admin: boolean; delay_ms: number; window_amount?: string }
  votes: { member: string; decision: string; at: number }[]
  ready_at?: number
  expires_at: number
  created_at: number
  updated_at?: number
  signed: { plan: number; txid: string }[]
  plans: number
  failure?: string
  digest?: string
  outcome?: any
  broadcasts?: any[]
  plan_details?: any[]
}
export interface Wallet {
  wallet: string
  environment?: string
  name: string
  me?: Member
  members: Member[]
  policy: any
  addresses: any[]
  cosigners: any[]
  frozen: boolean
  audit_head?: string
  stats?: any
  code?: string
  code_acceptance?: any
  channel_hint?: string
}

export const assetKey = (a: Asset): string =>
  a.key ??
  { ICP: 'icp', BTC: 'btc', SOL: 'sol' }[a.asset] ??
  (a.asset === 'ICRC' ? `icrc:${a.ledger}` : a.asset === 'SPL' ? `sol:${a.mint}` : `evm:${a.chain_id}${a.token ? `:${a.token}` : ''}`)
export const assetInfo = (a: Asset, metadata: Asset[]) => metadata.find((m) => assetKey(m).toLowerCase() === assetKey(a).toLowerCase()) ?? a
export const assetIdentity = (a: Asset) =>
  [a.symbol || a.asset, a.chain_id ? `#${a.chain_id}` : '', a.ledger || a.token || a.mint || ''].filter(Boolean).join(' · ')
export const assetChain = (a: Asset) =>
  ['ICP', 'ICRC'].includes(a.asset) ? 'Icp' : a.asset === 'BTC' ? 'Bitcoin' : ['ETH', 'ERC20'].includes(a.asset) ? 'Evm' : 'Solana'

export function toAtomic(text: string, decimals: number): string {
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 38 || !/^\d+(?:\.\d+)?$/.test(text) || text.length > 80)
    throw new Error('amount')
  const [whole, fraction = ''] = text.split('.')
  if (fraction.length > decimals) throw new Error('precision')
  const value = BigInt(whole + fraction.padEnd(decimals, '0'))
  if (value > (1n << 128n) - 1n) throw new Error('overflow')
  return value.toString()
}

export function decimal(value: string, decimals: number): string {
  if (!/^\d+$/.test(value) || !Number.isInteger(decimals) || decimals < 0 || decimals > 38) return value
  if (!decimals) return BigInt(value).toString()
  const n = value.padStart(decimals + 1, '0')
  const fraction = n.slice(-decimals).replace(/0+$/, '')
  return `${BigInt(n.slice(0, -decimals))}${fraction ? `.${fraction}` : ''}`
}

/** Cycles reserves in trillions, rounded to two decimal places for display. */
export function cyclesText(value: string): string {
  const hundredths = (BigInt(value) + 5_000_000_000n) / 10_000_000_000n
  return `${hundredths / 100n}.${(hundredths % 100n).toString().padStart(2, '0')}`
}

export const amountText = (value: string, asset: Asset, atomicLabel: string) =>
  asset.decimals == null
    ? `${value} ${atomicLabel} · ${assetIdentity(asset)}`
    : `${decimal(String(value), asset.decimals)} ${asset.symbol || asset.asset}`
export const feeAsset = (a: Asset): Asset =>
  a.asset === 'ERC20'
    ? { asset: 'ETH', symbol: [1, 8453].includes(a.chain_id!) ? 'ETH' : `Native #${a.chain_id}`, decimals: 18, chain_id: a.chain_id }
    : a.asset === 'SPL'
      ? { asset: 'SOL', decimals: 9 }
      : a
export const finalStates = new Set(['Settled', 'Failed', 'Rejected', 'Vetoed', 'Cancelled', 'Expired', 'Stale'])
export const membershipIntent = (kind: string) => ['add_member', 'update_member', 'remove_member'].includes(kind)
export const freezeAllows = (kind: string) => kind === 'unfreeze' || membershipIntent(kind)

export function stateAt(p: Proposal, now: number): string {
  if (!['Open', 'Delayed', 'Ready'].includes(p.status)) return p.status
  if (now >= p.expires_at) return 'Expired'
  return p.status === 'Delayed' && p.ready_at && now >= p.ready_at ? 'Ready' : p.status
}

/** Match the wallet's active voter and admin-only counting rules. */
export function approvalCount(p: Proposal, members: Member[]): number {
  return p.votes.filter(
    (v) =>
      v.decision === 'Approve' &&
      members.some(
        (m) =>
          m.id === v.member && m.status === 'Active' && (p.requirement.admin ? m.role === 'Admin' : ['Admin', 'Approver'].includes(m.role))
      )
  ).length
}

export function actions(p: Proposal, w: Wallet, now: number, plan = p.plans) {
  const me = w.me
  const active = me?.status === 'Active'
  const state = stateAt(p, now)
  const decider = active && ['Admin', 'Approver'].includes(me!.role)
  const eligible = decider && (!p.requirement.admin || me?.role === 'Admin')
  const voted = p.votes.some((v) => v.member === me?.id)
  const permitted = !w.frozen || freezeAllows(p.intent.kind)
  return {
    state,
    approve: state === 'Open' && eligible && !voted && permitted,
    reject: state === 'Open' && eligible && !voted,
    veto: state === 'Delayed' && decider,
    execute: active && me?.role !== 'Viewer' && ['Ready', 'Executing'].includes(state) && permitted,
    executeCollected:
      active &&
      me?.role !== 'Viewer' &&
      permitted &&
      (state === 'Ready' || (['Signed', 'Broadcast'].includes(state) && plan > 0 && !p.signed.some((s) => s.plan === plan))),
    rebroadcast: active && me?.role !== 'Viewer' && ['Signed', 'Broadcast'].includes(state),
    resign: active && me?.role !== 'Viewer' && ['Signed', 'Broadcast'].includes(state) && permitted,
    cancel: active && p.proposer === me?.id && ['Open', 'Delayed', 'Ready'].includes(state),
    voted
  }
}

/** An epoch protects all page responses from account/wallet changes and later reloads. */
export class ViewScope {
  private epoch = 0
  next() {
    return ++this.epoch
  }
  current(epoch: number) {
    return epoch === this.epoch
  }
  clear() {
    ++this.epoch
  }
}

const fields: Record<string, string[]> = {
  transfer: ['from', 'asset', 'outputs', 'max_fee', 'memo'],
  batch: ['from', 'asset', 'outputs', 'max_fee'],
  canister_call: ['canister', 'method', 'arg', 'arg_text', 'cycles', 'consent'],
  add_address: ['chain', 'control', 'control_details', 'label', 'cosign'],
  set_cosign: ['address', 'cosign'],
  add_cosigner: ['member', 'curve', 'public_key', 'envelope', 'hardware', 'label'],
  remove_cosigner: ['cosigner'],
  add_member: ['member'],
  update_member: ['member'],
  remove_member: ['member'],
  set_policy: ['policy'],
  book_add: ['destination', 'label'],
  book_remove: ['destination'],
  unfreeze: [],
  refuel: ['from', 'icp_e8s'],
  accept_upgrade: ['version', 'module_hash'],
  lock_code: [],
  cancel_evm_nonce: ['address', 'chain_id', 'max_fee'],
  release_evm_nonce: ['address', 'chain_id'],
  init_sol_nonce: ['address', 'max_fee']
}
export const knownIntent = (intent: any): boolean => {
  if (!intent || !Object.hasOwn(fields, intent.kind)) return false
  const optional = new Set(['memo', 'arg_text', 'consent', 'control_details'])
  if (fields[intent.kind].some((key) => !optional.has(key) && !Object.hasOwn(intent, key))) return false
  if (['set_cosign', 'add_address'].includes(intent.kind)) {
    const p = intent.cosign
    if (intent.kind === 'set_cosign' && (!Number.isSafeInteger(intent.address) || intent.address < 0)) return false
    if (p !== null && (!p || !Array.isArray(p.cosigners) || !Number.isInteger(p.threshold) || p.threshold < 1 || p.threshold > p.cosigners.length || p.cosigners.length > 16 ||
      p.cosigners.some((id: any) => !Number.isSafeInteger(id) || id < 0) || new Set(p.cosigners).size !== p.cosigners.length)) return false
  }
  if (['transfer', 'batch'].includes(intent.kind))
    return (
      !!intent.asset?.asset &&
      Array.isArray(intent.outputs) &&
      intent.outputs.length > 0 &&
      intent.outputs.every((o: any) => typeof o.to === 'string' && /^\d+$/.test(o.amount))
    )
  if (intent.kind === 'set_policy')
    return (
      !!intent.policy &&
      typeof intent.policy === 'object' &&
      typeof intent.policy.approver_threshold === 'number' &&
      Array.isArray(intent.policy.spendable_assets)
    )
  return true
}
export const intentFields = (intent: any) => fields[intent.kind] ?? []

/** Match the canister: general calls can spend from any protected ICP address. */
export function cosignAddresses(intent: any, wallet?: Wallet): any[] {
  if (!wallet) return []
  const from = ['transfer', 'batch', 'refuel'].includes(intent.kind) ? intent.from
    : ['cancel_evm_nonce', 'init_sol_nonce'].includes(intent.kind) ? intent.address : undefined
  return wallet.addresses.filter((a) => a.cosign && (a.id === from || (intent.kind === 'canister_call' && a.chain === 'Icp')))
}

export function sameCosign(a: any, b: any): boolean {
  if (!a || !b) return !a && !b
  return a.threshold === b.threshold && a.cosigners.length === b.cosigners.length && a.cosigners.every((id: number) => b.cosigners.includes(id))
}

export function cosignerReferences(wallet: Wallet, ids: number[]): any[] {
  return wallet.addresses.filter((a) => a.cosign?.cosigners.some((id: number) => ids.includes(id)) || ids.includes(a.control_details?.cosigner))
}

/** Native Taproot keys stay registered when their member leaves. */
export function removedMemberKeys(wallet: Wallet, member: string): number[] {
  return wallet.cosigners.filter((c) => c.member === member && !wallet.addresses.some((a) => a.control === 'taproot_2of2' && a.control_details?.cosigner === c.id)).map((c) => c.id)
}

export function unreachableCosigners(wallet: Wallet, hardware: boolean): any[] {
  return wallet.addresses.filter((a) => a.cosign && a.cosign.cosigners.filter((id: number) => wallet.cosigners.some((c) => c.id === id && (!hardware || c.hardware))).length < a.cosign.threshold)
}

/** Flatten every policy/member field, including empty collections and false values. */
export function leaves(value: any, path: string[] = []): { path: string[]; value: string }[] {
  if (value == null) return [{ path, value: '—' }]
  if (typeof value !== 'object') return [{ path, value: String(value) }]
  const entries = Object.entries(value)
  if (!entries.length) return [{ path, value: '—' }]
  return entries.flatMap(([key, v]) => leaves(v, [...path, key]))
}

export function previousIntent(intent: any, before?: Wallet): any {
  if (!before) return {}
  switch (intent.kind) {
    case 'set_policy':
      return { policy: before.policy }
    case 'update_member':
    case 'remove_member':
      return { member: before.members.find((m) => m.id === (typeof intent.member === 'string' ? intent.member : intent.member?.id)) }
    case 'set_cosign':
      return { cosign: before.addresses.find((a) => a.id === intent.address)?.cosign }
    case 'remove_cosigner':
      return { cosigner: before.cosigners.find((c) => c.id === intent.cosigner) }
    default:
      return {}
  }
}

export function policyDraft(policy: any) {
  return {
    ...JSON.parse(JSON.stringify(policy)),
    spendable_assets: policy.spendable_assets.map(assetKey),
    tiers: policy.tiers.map((t: any) => ({ ...t, asset: assetKey(t.asset) })),
    daily_limits: policy.daily_limits.map((l: any) => ({ ...l, asset: assetKey(l.asset) })),
    version: policy.version + 1
  }
}

export { validPrincipal } from '../../../shared/principal.ts'
export const utf8Length = (text: string) => new TextEncoder().encode(text).length
export const walletRoute = (wallet: string, proposal?: number) =>
  `/wallet/${encodeURIComponent(wallet)}${proposal == null ? '' : `/${proposal}`}`

/** Known explorer routes only; unknown networks keep the copyable transaction ID. */
export function explorerUrl(asset: Asset, txid: string, environment?: string): string | null {
  if (environment !== 'production') return null
  if (asset.asset === 'BTC' && /^[a-f0-9]{64}$/i.test(txid)) return `https://mempool.space/tx/${txid}`
  if (['ETH', 'ERC20'].includes(asset.asset) && /^0x[a-f0-9]{64}$/i.test(txid)) {
    const domain = asset.chain_id === 1 ? 'etherscan.io' : asset.chain_id === 8453 ? 'basescan.org' : null
    return domain ? `https://${domain}/tx/${txid}` : null
  }
  if (['SOL', 'SPL'].includes(asset.asset) && /^[1-9A-HJ-NP-Za-km-z]{80,90}$/.test(txid)) return `https://explorer.solana.com/tx/${txid}`
  return null
}
