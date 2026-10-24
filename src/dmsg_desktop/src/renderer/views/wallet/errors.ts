import { CallError, errorText } from '../../rpc'
import { failureLabel } from '../../failure'
/** Wallet forms keep the daemon's actionable field/reason, without losing its error code. */
export function walletError(error: unknown): string {
  if (error instanceof CallError) {
    const message = errorText(error)
    if (failureLabel(error.code)) return message
    return error.message && error.message !== error.code && !message.includes(error.message) ? `${message} · ${error.message}` : message
  }
  return error instanceof Error ? error.message : String(error)
}
