import type { Catalog, Label } from './i18n/messages/en.ts'

/** User-facing failures use codes, never raw transport URLs or server text. */
export function failureLabel(code: string): Label | undefined {
  if (code === 'DAEMON_DISCONNECTED') return 'daemonDisconnected'
  if (code === 'CACHE_MISSING') return 'loadingCachedDisplay'
  if (code === 'NETWORK_UNAVAILABLE') return 'networkUnavailable'
  if (code === 'EXECUTION_UNKNOWN') return 'operationUnknown'
  if (code === 'UNAVAILABLE') return 'serviceUnavailable'
  if (code === 'LOCAL_STORAGE_UNAVAILABLE') return 'localStorageUnavailable'
  if (code === 'PENDING') return 'reasonPending'
  if (code === 'INSUFFICIENT_FUNDS') return 'reasonInsufficientFunds'
  if (code === 'LOCKED') return 'locked'
  if (code === 'SYNC_BUSY') return 'syncBusy'
  if (['REQUEST_TOO_LARGE', 'RESPONSE_TOO_LARGE'].includes(code)) return 'syncSizeError'
  if (code === 'UNSUPPORTED_PROTOCOL') return 'syncUnsupported'
}

const temporary = new Set<string>()
const network = new Set<string>()

/** A language this window has shown: what it stored in that language still reads as what it is. */
export function rememberFailures({ messages }: Catalog) {
  for (const text of [messages.networkUnavailable, messages.operationUnknown, messages.passkeyRegistrationUnconfirmed, messages.serviceUnavailable, messages.networkReconnecting, messages.syncBusy]) temporary.add(text)
  network.add(messages.networkUnavailable)
}

/** Stored messages retain their original language when the UI locale changes. */
export const temporaryFailure = (message: string) => temporary.has(message)
export const networkFailure = (message: string) => network.has(message)
export const failureTone = (messages: string | string[]): 'warn' | 'danger' =>
  (Array.isArray(messages) ? messages : [messages]).filter(Boolean).every(temporaryFailure) ? 'warn' : 'danger'
