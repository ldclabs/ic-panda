import type { Label } from '../i18n/messages/en'
import { isManaged } from './vault/vault.ts'

export type NearbyMode = 'receive' | 'send-files' | 'send-vault'

export interface NearbyFileRef {
  file_id: string
  version_id?: string | null
  name: string
  size?: number
}

export interface NearbyProgress {
  total_items: number
  completed_items: number
  total_bytes: number
  bytes: number
  current_name?: string | null
}

export interface NearbyPeerIdentity {
  environment: string
  account: string
  device: string
  handle?: string | null
  status: string
}

export interface NearbyOfferedFile {
  index: number
  name: string
  mime: string
  size: number
}

export interface NearbyPendingPeer {
  peer_id: string
  sas: string
  qr_bound: boolean
  transport: string
  kind: 'files' | 'secret_package'
  files: NearbyOfferedFile[]
  package_size?: number | null
  alias?: string | null
}

export interface NearbyReceivedFile {
  index: number
  file_id: string
  version_id: string
  name: string
  size: number
}

export interface NearbyError {
  code: string
  message: string
}

export interface NearbyReceiveView {
  id: number
  state: 'waiting' | 'running' | 'done' | 'failed' | 'cancelled'
  alias: string
  discoverable: boolean
  advert: string
  pk_hint: string
  port: number
  addrs: string[]
  offer: string
  qr?: string
  expires_at: number
  relay_ready: boolean
  transport?: string | null
  mdns_error?: string | null
  pending_peers: NearbyPendingPeer[]
  verification_codes?: string[]
  active_peer?: string | null
  progress: NearbyProgress
  outcome?: {
    kind: 'files' | 'secret_package'
    files: NearbyReceivedFile[]
    package_preview?: any
    identity?: NearbyPeerIdentity | null
  } | null
  error?: NearbyError | null
}

export interface NearbySendView {
  id: number
  state: 'connecting' | 'awaiting_sas' | 'awaiting_accept' | 'running' | 'done' | 'failed' | 'cancelled'
  kind: 'files' | 'secret_package'
  transport?: string | null
  sas?: string | null
  transcript?: string | null
  qr_bound: boolean
  peer_label: string
  awaiting_sas_confirm?: boolean
  share_job_id?: string | null
  progress: NearbyProgress
  outcome?: {
    kind: 'files' | 'secret_package'
    sent_items: number
    bytes: number
    package_digest?: string | null
    identity?: NearbyPeerIdentity | null
  } | null
  error?: NearbyError | null
}

export interface NearbyStatus {
  receive: NearbyReceiveView | null
  send: NearbySendView | null
}

export interface DiscoveredPeer {
  advert: string
  alias: string
  pk_hint: string
  host: string
  port: number
  transport: string
}

export interface NearbyVaultSelection {
  item_id: string
  revision: string
  field_ids: string[]
  include_notes: boolean
  attachments: { file_id: string; version_id: string }[]
}

export type PillTone = 'neutral' | 'forest' | 'gem' | 'warn' | 'danger' | 'ok'

const OFFER_PATTERN = /^dmsg-nearby\/1:[A-Za-z0-9_-]{16,}$/

/** Format a 6-digit visual SAS code as two 3-digit groups (`123 456`). */
export function formatSas(sas: string | null | undefined): string {
  const clean = (sas ?? '').trim()
  if (/^\d{6}$/.test(clean)) return `${clean.slice(0, 3)} ${clean.slice(3)}`
  return clean
}

/** Basic client-side syntax check for pasted `dmsg-nearby/1:` offers. */
export function isValidNearbyOffer(text: string): boolean {
  return OFFER_PATTERN.test(text.trim())
}

/** Whether a receive session is actively listening or transferring. */
export function isReceiveActive(view: NearbyReceiveView | null | undefined): boolean {
  return view?.state === 'waiting' || view?.state === 'running'
}

/** Whether a send session is currently in flight. */
export function isSendActive(view: NearbySendView | null | undefined): boolean {
  return (
    view?.state === 'connecting' ||
    view?.state === 'awaiting_sas' ||
    view?.state === 'awaiting_accept' ||
    view?.state === 'running'
  )
}

/** Map wire transport token to its localized label key. */
export function transportLabelKey(transport: string | null | undefined): Label | null {
  if (transport === 'lan') return 'nearbyTransportLan'
  if (transport === 'awdl') return 'nearbyTransportAwdl'
  if (transport === 'relay') return 'nearbyTransportRelay'
  return null
}

/** Map post-confirmation peer identity verification level to its label key. */
export function identityStatusLabelKey(status: string | null | undefined): Label {
  if (status === 'same_account') return 'nearbyIdentitySameAccount'
  if (status === 'verified') return 'nearbyIdentityVerified'
  return 'nearbyIdentitySignatureOnly'
}

export function receiveStateBadge(state: string | null | undefined): { label: Label; tone: PillTone } {
  switch (state) {
    case 'waiting':
      return { label: 'nearbyStateWaiting', tone: 'neutral' }
    case 'running':
      return { label: 'nearbyStateRunning', tone: 'forest' }
    case 'done':
      return { label: 'nearbyStateDone', tone: 'ok' }
    case 'failed':
      return { label: 'nearbyStateFailed', tone: 'danger' }
    case 'cancelled':
      return { label: 'nearbyStateCancelled', tone: 'warn' }
    default:
      return { label: 'nearbyStateWaiting', tone: 'neutral' }
  }
}

export function sendStateBadge(state: string | null | undefined): { label: Label; tone: PillTone } {
  switch (state) {
    case 'connecting':
      return { label: 'nearbyStateConnecting', tone: 'neutral' }
    case 'awaiting_sas':
      return { label: 'nearbyStateAwaitingSas', tone: 'warn' }
    case 'awaiting_accept':
      return { label: 'nearbyStateAwaitingAccept', tone: 'neutral' }
    case 'running':
      return { label: 'nearbyStateRunning', tone: 'forest' }
    case 'done':
      return { label: 'nearbyStateDone', tone: 'ok' }
    case 'failed':
      return { label: 'nearbyStateFailed', tone: 'danger' }
    case 'cancelled':
      return { label: 'nearbyStateCancelled', tone: 'warn' }
    default:
      return { label: 'nearbyStateConnecting', tone: 'neutral' }
  }
}

/** Whether a Vault entry is ineligible for nearby snapshot sharing (matching `Sharing.svelte`). */
export function vaultEntryBlocked(entry: any, conflict = false): boolean {
  if (!entry || !entry.item) return true
  return !!entry.deleted || conflict || !!entry.conflict || isManaged(entry.item)
}

/** Whether an attached file's `files/show` payload is locally readable and shareable. */
export function attachmentAvailable(detail: any): boolean {
  const file = detail?.file
  return Boolean(file?.current && file.local && file.trashed_at == null)
}

/** Build the exact `vault_item` payload for `nearby/send_start`, or null if invalid/empty. */
export function buildVaultSendSelection(
  entry: any,
  conflict: boolean,
  selectedFieldIds: string[],
  includeNotes: boolean,
  selectedFileIds: string[],
  fileDetails: Record<string, any>
): NearbyVaultSelection | null {
  if (vaultEntryBlocked(entry, conflict)) return null
  const populated = ((entry.item.fields ?? []) as { id: string; value?: string }[])
    .filter((f) => Boolean(f.value) && selectedFieldIds.includes(f.id))
    .map((f) => f.id)
  const notes = Boolean(includeNotes && entry.item.notes)
  const attachments: { file_id: string; version_id: string }[] = []
  for (const id of (entry.item.files ?? []) as string[]) {
    if (!selectedFileIds.includes(id)) continue
    const detail = fileDetails[id]
    if (!attachmentAvailable(detail)) return null
    attachments.push({ file_id: id, version_id: detail.file.current })
  }
  if (!populated.length && !notes && !attachments.length) return null
  return {
    item_id: entry.id,
    revision: entry.revision,
    field_ids: populated,
    include_notes: notes,
    attachments
  }
}

/** Preserve existing per-peer checkbox selections across `nearby/changed` updates. */
export function reconcilePeerSelections(
  current: Record<string, number[]>,
  peers: NearbyPendingPeer[]
): Record<string, number[]> {
  const next: Record<string, number[]> = {}
  for (const peer of peers) {
    if (peer.kind !== 'files') continue
    const valid = new Set(peer.files.map((f) => f.index))
    if (Object.hasOwn(current, peer.peer_id)) {
      next[peer.peer_id] = current[peer.peer_id].filter((i) => valid.has(i))
    } else {
      next[peer.peer_id] = peer.files.map((f) => f.index)
    }
  }
  return next
}

/** Build JSON-RPC params for `nearby/send_start`, or null if target or payload is incomplete. */
export function buildSendStartParams(options: {
  mode: 'send-files' | 'send-vault'
  offerText: string
  selectedPeer: DiscoveredPeer | null
  files: NearbyFileRef[]
  vaultSelection: NearbyVaultSelection | null
  shareIdentity: boolean
}): Record<string, unknown> | null {
  const trimmedOffer = options.offerText.trim()
  const hasOffer = trimmedOffer.length > 0
  if (hasOffer && !isValidNearbyOffer(trimmedOffer)) return null
  if (!hasOffer && !options.selectedPeer) return null

  const params: Record<string, unknown> = {
    share_identity: options.shareIdentity
  }

  if (hasOffer) {
    params.offer = trimmedOffer
  } else if (options.selectedPeer) {
    params.host = options.selectedPeer.host
    params.port = options.selectedPeer.port
    params.peer_alias = options.selectedPeer.alias
    params.advert = options.selectedPeer.advert
    params.transport = options.selectedPeer.transport
  }

  if (options.mode === 'send-files') {
    if (!options.files.length) return null
    params.files = options.files.map((f) => ({
      file_id: f.file_id,
      ...(f.version_id ? { version_id: f.version_id } : {})
    }))
  } else {
    if (!options.vaultSelection) return null
    params.vault_item = options.vaultSelection
  }

  return params
}

/** Merge `nearby/changed` receive snapshots without losing `mdns_error` from `receive_start`. */
export function mergeReceiveView(
  next: NearbyReceiveView | null,
  mdnsError: { id: number; error: string } | null
): NearbyReceiveView | null {
  if (!next) return null
  if (!next.mdns_error && mdnsError && mdnsError.id === next.id) {
    return { ...next, mdns_error: mdnsError.error }
  }
  return next
}

/** Progress bar inputs: indeterminate until `total_bytes > 0`. */
export function progressFraction(progress: NearbyProgress | null | undefined): { value: number | null; max: number } {
  const total = progress?.total_bytes ?? 0
  if (total <= 0) return { value: null, max: 100 }
  return { value: Math.max(0, Math.min(progress?.bytes ?? 0, total)), max: total }
}
