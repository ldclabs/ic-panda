import en, { type Label } from './i18n/messages/en.ts'
import zh from './i18n/messages/zh.ts'
import ru from './i18n/messages/ru.ts'
import ar from './i18n/messages/ar.ts'
import fr from './i18n/messages/fr.ts'
import es from './i18n/messages/es.ts'

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
}

const temporary = new Set([en, zh, ru, ar, fr, es].flatMap(({ messages }) =>
  [messages.networkUnavailable, messages.operationUnknown, messages.serviceUnavailable, messages.networkReconnecting]))

/** Stored messages retain their original language when the UI locale changes. */
export const temporaryFailure = (message: string) => temporary.has(message)
export const networkFailure = (message: string) => [en, zh, ru, ar, fr, es].some(({ messages }) => messages.networkUnavailable === message)
export const failureTone = (messages: string | string[]): 'warn' | 'danger' =>
  (Array.isArray(messages) ? messages : [messages]).filter(Boolean).every(temporaryFailure) ? 'warn' : 'danger'
