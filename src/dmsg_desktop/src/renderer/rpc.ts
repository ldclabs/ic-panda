/** Calls through `window.dmsg`, with the daemon's error code split off. */
export class CallError extends Error {
  readonly code: string
  constructor(message: string) {
    const [code, ...rest] = message.split(': ')
    super(rest.join(': ') || code)
    this.code = /^[A-Z_]+$/.test(code) ? code : 'UNAVAILABLE'
  }
}

export async function call<T = any>(method: string, params?: unknown): Promise<T> {
  try {
    return (await window.dmsg.call(method, params)) as T
  } catch (error) {
    throw new CallError(error instanceof Error ? error.message : String(error))
  }
}

export function code(error: unknown): string {
  return error instanceof CallError ? error.code : 'UNAVAILABLE'
}

export const when = (ms?: number | null) => (ms ? new Date(ms).toLocaleString() : '-')
