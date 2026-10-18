/**
 * How the approval window and the requests list present a request: what
 * kind of decision it is, whether approving it takes a hold, its title and
 * one-line summary, the way it came to this device and the steps of its
 * outcome. Pure, so the rules are tested apart from the windows.
 */
import type { Glyph } from './components/glyphs'
import type { Label } from './i18n/messages/en'

/** The kind of decision, for the chip at the top of the ticket. */
export function kindOf(request: any): Label {
  const d = request?.detail ?? {}
  switch (request?.type) {
    case 'connect':
      return d.method === 'checkout' ? 'kindPayment' : 'kindWeb'
    case 'attest':
      return 'kindSignature'
    case 'checkout':
    case 'letter':
      return 'kindPayment'
    case 'handle':
      return d.total ? 'kindPayment' : 'kindName'
    case 'delegation':
    case 'principal':
      return 'kindAgent'
    case 'account':
    case 'device_capabilities':
    case 'device_revoke':
      return 'kindSecurity'
    case 'wallet':
      return 'kindWallet'
    default:
      return 'kindOther'
  }
}

/** The icon of each kind of decision. */
export const kindIcon: Record<string, Glyph> = {
  kindWeb: 'website',
  kindPayment: 'coins',
  kindSignature: 'signature',
  kindName: 'user',
  kindAgent: 'agents',
  kindSecurity: 'security',
  kindWallet: 'wallet',
  kindOther: 'info'
}

/**
 * Requests that move money, revoke a device or change who controls the
 * account, as the daemon rates them (`risk`), are approved by holding the
 * button for a second, and where the device can ask, with Touch ID or the
 * login password too; the rest by a click once the approve delay has passed.
 */
export function holdToApprove(request: any): boolean {
  return request?.risk === 'high'
}

/** The statement a request signs: this device's own, or a page's document or action. */
export function statementOf(request: any): any {
  const d = request?.detail ?? {}
  return d.statement ?? d.content?.statement ?? (d.method === 'signAction' ? d.content : null)
}

/** An application action as its registered schema renders it, when it does. */
export function renderedAction(request: any): any {
  const d = request?.detail ?? {}
  return d.action ?? (d.method === 'signAction' ? d.content?.action : null) ?? null
}

/** A security change for people. */
export const changeLabels: Record<string, Label> = {
  bind_login: 'bindLogin',
  remove_login: 'removeLogin',
  policy: 'signingPolicy',
  recovery_delay: 'recoveryDelay',
  dispute_recovery: 'disputeRecovery'
}

/** A title: a label, and what it is about when the label alone does not say. */
export interface Title {
  label: Label
  detail?: string
}

/** What a request asks, for the ticket's title and the requests list. */
export function titleOf(request: any): Title {
  const d = request?.detail ?? {}
  switch (request?.type) {
    case 'connect':
    case 'attest': {
      if (d.method === 'checkout') return { label: 'payPlan' }
      if (d.method === 'authenticate') return { label: 'authenticateTitle' }
      const s = statementOf(request)
      if (s?.profile === 'app_action') return { label: 'appAction' }
      return { label: s?.content?.sha256 ? 'signFile' : 'newStatement' }
    }
    case 'checkout':
      return { label: 'payPlan' }
    case 'letter':
      return { label: 'payLetter' }
    case 'handle':
      return { label: 'names', detail: String(d.name ?? '') }
    case 'delegation':
      return { label: d.kind?.type === 'revoke' ? 'delegationRevoke' : 'delegationGrant' }
    case 'principal':
      return { label: 'principalChange' }
    case 'account':
      return changeLabels[d.change] ? { label: changeLabels[d.change] } : { label: 'kindSecurity', detail: String(d.change ?? '') }
    case 'device_capabilities':
      return { label: 'capabilities' }
    case 'device_revoke':
      return { label: 'revokeDevice' }
    case 'wallet':
      if (d.action === 'create') return { label: 'twCreateTitle' }
      if (d.action === 'join') return { label: 'twJoinTitle' }
      if (d.action === 'cosign') return { label: 'twCosignTitle' }
      return { label: d.decision === 'veto' ? 'twVetoTitle' : d.decision === 'reject' ? 'twRejectTitle' : 'twApproveTitle' }
    default:
      return { label: 'kindOther', detail: String(request?.type ?? '') }
  }
}

/** What a request is about, in one line. */
export function summaryOf(request: any): string {
  const d = request?.detail ?? {}
  const s = statementOf(request)
  if (s) {
    if (s.profile === 'app_action') return `${s.content?.app_id ?? ''} · ${s.content?.command ?? ''}`
    const file = d.file?.name ?? request?.file_check?.name
    const what = s.content?.text ?? s.content?.sha256 ?? ''
    return file ? `${file} · ${what}` : what
  }
  switch (request?.type) {
    case 'connect':
      return `${d.app_id ?? ''} · ${d.origin ?? ''}`
    case 'checkout':
      return `${d.sku ?? ''} · ${d.origin ?? ''}`
    case 'letter':
      return String(d.recipient ?? '')
    case 'handle':
      return `${d.action ?? ''} · ${d.name ?? ''}`
    case 'delegation':
      return String(d.kind?.subject ?? d.kind?.id ?? '')
    case 'principal':
      return String(d.change ?? '')
    case 'account':
      return String(d.principal ?? d.op_id ?? '')
    case 'device_capabilities':
    case 'device_revoke':
      return String(d.device_id ?? '').slice(0, 16)
    case 'wallet':
      return `${d.name ?? ''}${d.proposal ? ` · #${d.proposal.id} ${d.proposal.intent?.kind ?? ''}` : ''}`
    default:
      return ''
  }
}

/** One step of an outcome: a label or a request state's name, and how it went. */
export interface Step {
  label?: Label
  state?: string
  icon: Glyph
  tone: 'ok' | 'danger' | 'warn' | 'muted' | 'pending'
}

const ok = (label: Label): Step => ({ label, icon: 'check', tone: 'ok' })
const pending = (label: Label): Step => ({ label, icon: 'pending', tone: 'pending' })

/**
 * A decision's progress from the daemon's states, each step a real event
 * (PRD S04): approved; for a web request, the relay's authorization, the
 * result and the page taking it; otherwise the execution. `approving` is
 * the window's own approval still under way.
 */
export function stepsOf(request: any, approving = false): Step[] {
  const state = request?.state
  if (!state) return []
  if (state === 'awaiting_user') return approving ? [ok('stepApproved'), pending('stepRunning')] : []
  if (['rejected', 'expired', 'cancelled', 'elsewhere'].includes(state)) return [{ state, icon: 'close', tone: 'muted' }]
  const steps: Step[] = [ok('stepApproved')]
  const op = request?.type === 'connect' ? request?.detail?.state : null
  if (op) {
    const authorized = ['authorized', 'execution_unknown', 'signed', 'returned'].includes(op)
    steps.push(authorized ? ok('stepAuthorized') : state === 'failed' ? { label: 'stepAuthorized', icon: 'close', tone: 'danger' } : pending('stepAuthorized'))
    if (!authorized) return steps
    if (op === 'execution_unknown') steps.push({ state: 'unknown', icon: 'warning', tone: 'warn' })
    else if (state === 'failed') steps.push({ state, icon: 'warning', tone: 'danger' })
    else if (op === 'authorized') steps.push(pending('stepResult'))
    else {
      steps.push(ok('stepResult'))
      steps.push(op === 'returned' ? ok('stepReturned') : pending('stepReturned'))
    }
    return steps
  }
  if (state === 'executing') steps.push({ state, icon: 'pending', tone: 'pending' })
  else steps.push({ state, icon: state === 'done' ? 'check' : 'warning', tone: state === 'done' ? 'ok' : state === 'failed' ? 'danger' : 'warn' })
  return steps
}

/**
 * A label of an application's schema in `locale`: the exact tag, then its
 * language, then English, then any.
 */
export function pickLabel(labels: Record<string, string> | null | undefined, locale: string): string {
  if (!labels) return ''
  const language = locale.split('-')[0]
  const tags = Object.keys(labels)
  const tag = tags.find((t) => t === locale) ?? tags.find((t) => t.split('-')[0] === language) ?? tags.find((t) => t.split('-')[0] === 'en') ?? tags[0]
  return tag ? labels[tag] : ''
}

/** A value of an application's action, rendered by its schema, as text in `locale`. */
export function actionValue(value: any, locale: string): string {
  switch (value?.type) {
    case 'bool':
      return pickLabel(value.label, locale)
    case 'choice':
      return value.label ? pickLabel(value.label, locale) : String(value.text ?? '')
    case 'null':
      return '—'
    case 'artifact':
      return `${value.uri} · sha256 ${value.sha256}`
    case 'list':
      return (value.items ?? []).map((v: any) => actionValue(v, locale)).join(', ')
    case 'record':
      return (value.fields ?? []).map((f: any) => `${pickLabel(f.label, locale)}: ${actionValue(f.value, locale)}`).join('; ')
    default:
      return String(value?.text ?? '')
  }
}

/** One step on the way to this device; a relayed step reports what another party told it. */
export interface Hop {
  icon: Glyph
  /** What the step is, such as the website's host; empty when the role says it all. */
  name: string
  role: Label
  relayed?: boolean
}

const device: Hop = { icon: 'device', name: '', role: 'sourceDevice' }

/**
 * Where a request came from, from its `source`: a website through the paired
 * browser and connect.dmsg.net (relayed, so drawn dashed), an agent (by its
 * name among `agents`, a map from Agent ID to name, when this device hosts
 * it), the terminal or the main window.
 */
export function trailOf(request: any, agents: Record<string, string> = {}): Hop[] {
  const d = request?.detail ?? {}
  const source = String(request?.source ?? '')
  if (request?.type === 'connect' || source.startsWith('connect:')) {
    const origin = String(d.origin ?? source.slice('connect:'.length))
    return [
      { icon: 'website', name: origin.replace(/^https?:\/\//, ''), role: 'sourceWebsite' },
      { icon: 'browser', name: String(d.pairing_label ?? ''), role: 'sourceBrowser' },
      { icon: 'relay', name: String(d.relayed_by ?? 'connect.dmsg.net'), role: 'sourceRelay', relayed: true },
      { ...device, relayed: true }
    ]
  }
  if (source.startsWith('agent:')) {
    const did = source.slice('agent:'.length)
    return [{ icon: 'agents', name: agents[did] ?? did, role: 'sourceAgent' }, device]
  }
  if (source === 'cli') return [{ icon: 'terminal', name: 'dmsg', role: 'sourceTerminal' }, device]
  if (source === 'gui') return [{ icon: 'window', name: 'dMsg', role: 'sourceWindow' }, device]
  return [{ icon: 'window', name: source, role: 'sourceOther' }, device]
}

/** Why a waiting request did not run, or why a signature cannot be made now, for people. */
export const reasonLabels: Record<string, Label> = {
  AUTH_REQUIRED: 'reasonAuthRequired',
  UNAVAILABLE: 'reasonUnavailable',
  POLICY_STALE: 'reasonPolicyStale',
  PRESENCE_DECLINED: 'reasonPresenceDeclined',
  PRESENCE_REQUIRED: 'reasonPresenceRequired',
  PRESENCE_UNAVAILABLE: 'reasonPresenceUnavailable',
  FORMAL_APPROVE: 'reasonFormalApprove',
  FROZEN: 'reasonFrozen',
  PURPOSE: 'reasonPurpose',
  QUOTA: 'reasonQuota',
  QUOTA_EXCEEDED: 'reasonQuota',
  FORBIDDEN: 'reasonForbidden',
  LOCKED: 'reasonFrozen',
  INTEGRITY_FAILED: 'reasonIntegrity',
  EXPIRED: 'reasonExpired',
  RESULT_EXPIRED: 'reasonResultExpired'
}

/** The signing purposes of the account's policy, for people. */
export const purposeLabels: Record<string, Label> = {
  Statement: 'purposeStatement',
  FileAttestation: 'purposeFileAttestation',
  AppAction: 'purposeAppAction'
}

/** A month as `YYYYMM`, in the interface language. */
export function monthText(month: number, locale: string): string {
  const year = Math.floor(month / 100)
  const index = (month % 100) - 1
  if (!year || index < 0 || index > 11) return String(month)
  return new Intl.DateTimeFormat(locale, { year: 'numeric', month: 'short', timeZone: 'UTC' }).format(Date.UTC(year, index, 1))
}
