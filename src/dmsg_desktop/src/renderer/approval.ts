/**
 * How the approval window presents a request: what kind of decision it is,
 * whether approving it takes a hold, and the way it came to this device.
 * Pure, so the rules are tested apart from the window.
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

/** Account changes approved by holding: who can log in and recover, and how formal signatures are governed. */
const HELD_CHANGES = ['bind_login', 'remove_login', 'policy', 'recovery_delay']

/**
 * Requests that move money, revoke a device or change who controls the
 * account are approved by holding the button for a second; the rest by a
 * click once the approve delay has passed.
 */
export function holdToApprove(request: any): boolean {
  const d = request?.detail ?? {}
  switch (request?.type) {
    case 'checkout':
    case 'letter':
    case 'device_revoke':
      return true
    case 'handle':
      return !!d.total
    case 'connect':
      return d.method === 'checkout'
    case 'account':
      return HELD_CHANGES.includes(d.change)
    case 'wallet':
      return walletMovesFunds(d)
    default:
      return false
  }
}

/** Wallet requests that pay, sign a transfer or approve one are held. */
function walletMovesFunds(d: any): boolean {
  if (d.action === 'create' || d.action === 'cosign') return true
  const kind = d.proposal?.intent?.kind
  return d.action === 'decide' && d.decision === 'approve' && ['transfer', 'batch', 'canister_call', 'refuel'].includes(kind)
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
 * browser and connect.dmsg.net (relayed, so drawn dashed), an agent, the
 * terminal or the main window.
 */
export function trailOf(request: any): Hop[] {
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
  if (source.startsWith('agent:')) return [{ icon: 'agents', name: source.slice('agent:'.length), role: 'sourceAgent' }, device]
  if (source === 'cli') return [{ icon: 'terminal', name: 'dmsg', role: 'sourceTerminal' }, device]
  if (source === 'gui') return [{ icon: 'window', name: 'dMsg', role: 'sourceWindow' }, device]
  return [{ icon: 'window', name: source, role: 'sourceOther' }, device]
}
