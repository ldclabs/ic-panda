import type { Catalog, Label } from './i18n/messages/en.ts'

/** User-facing failures use codes, never raw transport URLs or server text. */
const labels = new Map<string, Label>([
  ['DAEMON_DISCONNECTED', 'daemonDisconnected'],
  ['CACHE_MISSING', 'loadingCachedDisplay'],
  ['NETWORK_UNAVAILABLE', 'networkUnavailable'],
  ['EXECUTION_UNKNOWN', 'operationUnknown'],
  ['UNAVAILABLE', 'serviceUnavailable'],
  ['LOCAL_STORAGE_UNAVAILABLE', 'localStorageUnavailable'],
  ['PENDING', 'reasonPending'],
  ['MEMBERSHIP_STALE', 'membershipRefreshHint'],
  ['INSUFFICIENT_FUNDS', 'reasonInsufficientFunds'],
  ['LOCKED', 'locked'],
  ['SYNC_BUSY', 'syncBusy'],
  ['REQUEST_TOO_LARGE', 'syncSizeError'],
  ['RESPONSE_TOO_LARGE', 'syncSizeError'],
  ['UNSUPPORTED_PROTOCOL', 'syncUnsupported']
])

export const failureLabel = (code: string) => labels.get(code)

const temporary = new Set<string>()
const network = new Set<string>()
const failures = new Set<string>()
const errorPrefixes = new Set<string>()

export const isFailure = (message: string) => failures.has(message) || [...errorPrefixes].some((prefix) => message.startsWith(prefix))

/** A language this window has shown: what it stored in that language still reads as what it is. */
export function rememberFailures({ messages }: Catalog) {
  for (const text of [messages.networkUnavailable, messages.operationUnknown, messages.passkeyRegistrationUnconfirmed, messages.serviceUnavailable, messages.networkReconnecting, messages.syncBusy, messages.membershipRefreshHint]) temporary.add(text)
  network.add(messages.networkUnavailable)
  for (const label of labels.values()) if (label !== 'loadingCachedDisplay') failures.add(messages[label])
  for (const text of [messages.appearanceFailed, messages.copyFailed, messages.languageFailed, messages.shortcutLockFailed]) failures.add(text)
  errorPrefixes.add(`${messages.error}: `)
}

/** Stored messages retain their original language when the UI locale changes. */
export const temporaryFailure = (message: string) => temporary.has(message)
export const networkFailure = (message: string) => network.has(message)
export const failureTone = (messages: string | string[]): 'warn' | 'danger' =>
  (Array.isArray(messages) ? messages : [messages]).filter(Boolean).every(temporaryFailure) ? 'warn' : 'danger'
