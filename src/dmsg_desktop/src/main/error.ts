/** A failure with the daemon's error code, such as `FORBIDDEN` or `UNAVAILABLE`. */
export class DaemonError extends Error {
  readonly code: string
  constructor(code: string, message: string) {
    super(message)
    this.code = code
  }
}
