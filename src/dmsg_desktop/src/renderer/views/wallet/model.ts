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
  ledger?: string
  source_account?: string
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
  [a.symbol || a.asset, networkName(a), a.ledger || a.token || a.mint || ''].filter(Boolean).join(' · ')

/** Names are display hints. The chain ID / contract remains the asset identity. */
export function networkName(a: Asset): string {
  if (['ICP', 'ICRC'].includes(a.asset)) return 'Internet Computer'
  if (a.asset === 'BTC') return 'Bitcoin'
  if (['SOL', 'SPL'].includes(a.asset)) return 'Solana'
  const names: Record<number, string> = {
    1: 'Ethereum',
    8453: 'Base',
    10: 'Optimism',
    42161: 'Arbitrum One',
    137: 'Polygon',
    56: 'BNB Chain',
    11155111: 'Sepolia',
    84532: 'Base Sepolia'
  }
  return `${names[a.chain_id!] ?? 'EVM'} (${a.chain_id ?? '?'})`
}
export const assetChain = (a: Asset) =>
  ['ICP', 'ICRC'].includes(a.asset) ? 'Icp' : a.asset === 'BTC' ? 'Bitcoin' : ['ETH', 'ERC20'].includes(a.asset) ? 'Evm' : 'Solana'

export function assetFromKey(key: string, metadata: Asset[] = []): Asset {
  const found = metadata.find((a) => assetKey(a) === key)
  if (found) return found
  const [chain, id, token] = key.split(':')
  if (chain === 'icp') return { asset: 'ICP', decimals: 8 }
  if (chain === 'btc') return { asset: 'BTC', decimals: 8 }
  if (chain === 'icrc') return { asset: 'ICRC', ledger: id }
  if (chain === 'sol') return id ? { asset: 'SPL', mint: id } : { asset: 'SOL', decimals: 9 }
  return { asset: token ? 'ERC20' : 'ETH', chain_id: Number(id), token, ...(token ? {} : { decimals: 18 }) }
}

export function validPolicyDraft(policy: any, members: Member[]): boolean {
  const integer = (v: any, min: number, max: number) => Number.isSafeInteger(v) && v >= min && v <= max
  const money = (v: any) => typeof v === 'string' && /^\d+$/.test(v) && BigInt(v) <= (1n << 128n) - 1n
  const voters = members.filter((m) => ['Admin', 'Approver'].includes(m.role)).length
  if (
    !integer(policy.approver_threshold, 1, voters) ||
    !integer(policy.admin_threshold, 1, members.filter((m) => m.role === 'Admin').length)
  )
    return false
  if (!integer(policy.daily_signatures, 1, 1000) || !integer(policy.proposal_ttl_ms, 3600000, 90 * 86400000)) return false
  if (![policy.address_book_cooldown_ms, policy.unfreeze_delay_ms].every((v) => integer(v, 0, 30 * 86400000))) return false
  if (
    !policy.tiers.every(
      (r: any, i: number) =>
        money(r.up_to) &&
        integer(r.approvals, 1, voters) &&
        integer(r.delay_ms, 0, 30 * 86400000) &&
        policy.tiers
          .slice(0, i)
          .filter((p: any) => p.asset === r.asset)
          .every((p: any) => BigInt(p.up_to) < BigInt(r.up_to) && p.approvals <= r.approvals)
    )
  )
    return false
  if (!policy.daily_limits.every((r: any) => money(r.amount))) return false
  return (
    !policy.refuel ||
    (money(policy.refuel.min_cycles) &&
      money(policy.refuel.icp_e8s) &&
      BigInt(policy.refuel.icp_e8s) > 0n &&
      BigInt(policy.refuel.icp_e8s) <= (1n << 64n) - 1n &&
      integer(policy.refuel.max_per_day, 1, 24))
  )
}

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

/** Localized duration without asking people to count zeroes in milliseconds. */
export function durationText(ms: number, language = 'en'): string {
  const units = [
    [86400000, 'day'],
    [3600000, 'hour'],
    [60000, 'minute'],
    [1000, 'second'],
    [1, 'millisecond']
  ] as const
  const [scale, unit] = units.find(([scale]) => ms >= scale && ms % scale === 0) ?? units[ms === 0 ? 2 : 4]
  return new Intl.NumberFormat(language, { style: 'unit', unit, unitDisplay: 'short', maximumFractionDigits: 2 }).format(ms / scale)
}

export const batchLimit = (chain?: string) => (chain === 'Evm' ? 1 : 32)

/** The outcome says whether money moved; the proposal status alone does not. */
export function resultState(p: Proposal, now: number): string {
  if (p.outcome?.kind === 'call_unknown') return 'resultUnknown'
  if (p.status === 'Failed' && p.outcome?.kind === 'ledger' && p.outcome.blocks?.length) return 'partialPayment'
  if (p.status === 'Executing') return 'waitingResult'
  if (p.status === 'Settled') {
    if (p.outcome?.kind === 'ledger') return 'ledgerConfirmed'
    if (p.signed.length) return 'chainReported'
    if (p.outcome?.kind === 'replied') return 'callReplied'
    if (p.outcome?.kind === 'refueled') return 'refueled'
    return ['applied', 'cosigner_added', 'address_created'].includes(p.outcome?.kind) ? 'applied' : 'processFinished'
  }
  return stateAt(p, now)
}

export function queueKind(p: Proposal, wallet: Wallet, now: number): string {
  const result = resultState(p, now)
  if (['resultUnknown', 'partialPayment', 'waitingResult'].includes(result)) return 'attention'
  const state = stateAt(p, now)
  if (['Signed', 'Broadcast'].includes(state)) return 'confirmation'
  if (state === 'Ready') return cosignAddresses(p.intent, wallet).length ? 'cosigning' : 'executable'
  if (state === 'Delayed') return 'delay'
  if (state === 'Open') return actions(p, wallet, now).approve ? 'waiting' : 'votes'
  return 'history'
}

export function teamReadiness(wallet: Wallet): { key: string; count?: string; warn: boolean }[] {
  const active = wallet.members.filter((m) => m.status === 'Active')
  const approvers = active.filter((m) => ['Admin', 'Approver'].includes(m.role)).length
  const admins = active.filter((m) => m.role === 'Admin').length
  return [
    { key: 'readyVoters', count: `${approvers} / ${wallet.policy.approver_threshold}`, warn: approvers < wallet.policy.approver_threshold },
    { key: 'readyAdmins', count: `${admins} / ${wallet.policy.admin_threshold}`, warn: admins < wallet.policy.admin_threshold },
    { key: 'invitedMembers', count: String(wallet.members.length - active.length), warn: active.length < wallet.members.length },
    { key: wallet.policy.approver_threshold === 1 ? 'singleControl' : 'sharedControl', warn: wallet.policy.approver_threshold === 1 },
    {
      key: active.some((m) => m.role === 'Recovery') ? 'recoveryConfigured' : 'recoveryMissing',
      warn: !active.some((m) => m.role === 'Recovery')
    }
  ]
}

/** Preview only: editing a draft must not promise approval or reserve funds. */
export function policyExample(policy: any, asset: string, amount: string, reserved: string, executed: string, voters: number) {
  const total = BigInt(amount) + BigInt(reserved) + BigInt(executed)
  const tiers = policy.tiers.filter((r: any) => (typeof r.asset === 'string' ? r.asset : assetKey(r.asset)) === asset)
  const chosen = tiers.find((r: any) => BigInt(r.up_to) >= total)
  const top = tiers.at(-1)
  let approvals = chosen?.approvals ?? (top ? Math.max(top.approvals, Math.min(voters, top.approvals + 1)) : policy.approver_threshold)
  let delay = chosen?.delay_ms ?? top?.delay_ms ?? 0
  const all = policy.daily_limits.some(
    (r: any) => (typeof r.asset === 'string' ? r.asset : assetKey(r.asset)) === asset && total > BigInt(r.amount)
  )
  if (all) {
    approvals = Math.max(approvals, voters)
    delay = Math.max(delay, ...tiers.map((r: any) => r.delay_ms))
  }
  return { total: total.toString(), approvals, delay, all, book: chosen?.book_only ?? top?.book_only ?? false }
}

/** Describe an operation in a queue without leaking implementation fields. */
export function intentHeadline(intent: any, metadata: Asset[]): { kind: string; text: string } {
  if (intent.asset && intent.outputs?.length) {
    const asset = assetInfo(intent.asset, metadata)
    const total = intent.outputs.reduce((n: bigint, o: any) => n + BigInt(o.amount), 0n)
    return {
      kind: intent.kind,
      text: `${amountText(total.toString(), asset, 'atomic')} · ${networkName(asset)} → ${intent.outputs.length === 1 ? intent.outputs[0].to : `${intent.outputs.length}`}`
    }
  }
  const member = typeof intent.member === 'object' ? intent.member : null
  return {
    kind: intent.kind,
    text:
      intent.label ||
      member?.label ||
      member?.id ||
      (typeof intent.member === 'string' ? intent.member : '') ||
      (intent.version ? `v${intent.version}` : intent.address != null ? `#${intent.address}` : '')
  }
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

export function intentFeeAsset(intent: any, metadata: Asset[] = []): Asset {
  if (intent.asset) return feeAsset(assetInfo(intent.asset, metadata))
  return intent.kind === 'init_sol_nonce' ? { asset: 'SOL', decimals: 9 } : { asset: 'ETH', chain_id: intent.chain_id, decimals: 18 }
}

/** An advisory link to the known ledger browser, never a payment authority. */
export function ledgerLookupUrl(ledger?: string): string | null {
  return ledger && /^[a-z0-9-]+$/.test(ledger) ? `https://dashboard.internetcomputer.org/canister/${ledger}` : null
}
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
  const replacementAllowed =
    p.intent.kind !== 'init_sol_nonce' &&
    (!['SOL', 'SPL'].includes(p.intent.asset?.asset) || !!w.addresses.find((a) => a.id === p.intent.from)?.sol_nonce)
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
    resign: active && me?.role !== 'Viewer' && ['Signed', 'Broadcast'].includes(state) && permitted && replacementAllowed,
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
  cancel_evm_nonce: ['address', 'chain_id', 'proposal', 'nonce', 'max_fee'],
  release_evm_nonce: ['address', 'chain_id', 'proposal', 'nonce'],
  init_sol_nonce: ['address', 'max_fee']
}
export const knownIntent = (intent: any): boolean => {
  if (!intent || !Object.hasOwn(fields, intent.kind)) return false
  const optional = new Set(['memo', 'arg_text', 'consent', 'control_details'])
  if (fields[intent.kind].some((key) => !optional.has(key) && !Object.hasOwn(intent, key))) return false
  if (['set_cosign', 'add_address'].includes(intent.kind)) {
    const p = intent.cosign
    if (intent.kind === 'set_cosign' && (!Number.isSafeInteger(intent.address) || intent.address < 0)) return false
    if (
      p !== null &&
      (!p ||
        !Array.isArray(p.cosigners) ||
        !Number.isInteger(p.threshold) ||
        p.threshold < 1 ||
        p.threshold > p.cosigners.length ||
        p.cosigners.length > 16 ||
        p.cosigners.some((id: any) => !Number.isSafeInteger(id) || id < 0) ||
        new Set(p.cosigners).size !== p.cosigners.length)
    )
      return false
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
  const from = ['transfer', 'batch', 'refuel'].includes(intent.kind)
    ? intent.from
    : ['cancel_evm_nonce', 'init_sol_nonce'].includes(intent.kind)
      ? intent.address
      : undefined
  return wallet.addresses.filter((a) => a.cosign && (a.id === from || (intent.kind === 'canister_call' && a.chain === 'Icp')))
}

export function sameCosign(a: any, b: any): boolean {
  if (!a || !b) return !a && !b
  return (
    a.threshold === b.threshold && a.cosigners.length === b.cosigners.length && a.cosigners.every((id: number) => b.cosigners.includes(id))
  )
}

export function cosignerReferences(wallet: Wallet, ids: number[]): any[] {
  return wallet.addresses.filter(
    (a) => a.cosign?.cosigners.some((id: number) => ids.includes(id)) || ids.includes(a.control_details?.cosigner)
  )
}

/** Native Taproot keys stay registered when their member leaves. */
export function removedMemberKeys(wallet: Wallet, member: string): number[] {
  return wallet.cosigners
    .filter(
      (c) => c.member === member && !wallet.addresses.some((a) => a.control === 'taproot_2of2' && a.control_details?.cosigner === c.id)
    )
    .map((c) => c.id)
}

export function unreachableCosigners(wallet: Wallet, hardware: boolean): any[] {
  return wallet.addresses.filter(
    (a) =>
      a.cosign &&
      a.cosign.cosigners.filter((id: number) => wallet.cosigners.some((c) => c.id === id && (!hardware || c.hardware))).length <
        a.cosign.threshold
  )
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
