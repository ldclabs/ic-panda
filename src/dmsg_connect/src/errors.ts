/** The stable codes of dmsg-connect/1 §4.7, plus the SDK's transport codes. */
export type ConnectCode =
  | 'INVALID_INPUT'
  | 'UNSUPPORTED_PROTOCOL'
  | 'UNSUPPORTED_BROWSER'
  | 'FORBIDDEN'
  | 'APP_NOT_REGISTERED'
  | 'APP_PAUSED'
  | 'POLICY_STALE'
  | 'INTEGRITY_FAILED'
  | 'ACCOUNT_MISMATCH'
  | 'NOT_PAIRED'
  | 'NO_APPROVING_DEVICE'
  | 'LOCKED'
  | 'IDEMPOTENCY_CONFLICT'
  | 'NOT_FOUND'
  | 'NOT_CANCELLABLE'
  | 'QUOTA_EXCEEDED'
  | 'EXPIRED'
  | 'DEVICE_UNAVAILABLE'
  | 'DEVICE_REVOKED'
  | 'UNAVAILABLE'

export class ConnectError extends Error {
  readonly code: ConnectCode
  constructor(code: ConnectCode, message: string = code) {
    super(message)
    this.name = 'ConnectError'
    this.code = code
  }
}

export function fail(code: ConnectCode, message?: string): never {
  throw new ConnectError(code, message)
}

export function ensure(condition: unknown, code: ConnectCode, message?: string): asserts condition {
  if (!condition) fail(code, message)
}
