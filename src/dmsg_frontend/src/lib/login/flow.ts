// The login page's single round trip: read the request once, ask Internet
// Identity for a delegation to the client's session key, navigate back.
import {
  encodeError,
  encodeResult,
  type LoginErrorCode,
  type LoginRequest
} from './protocol'

/** Read the fragment and remove it from the address bar and history. */
export function takeFragment(
  location: Pick<Location, 'hash' | 'pathname'>,
  history: Pick<History, 'replaceState'>
): string {
  const fragment = location.hash.slice(1)
  history.replaceState(null, '', location.pathname)
  return fragment
}

/** ICRC-25 code 3001 is the user's refusal; a closed window is too. */
export function errorCode(error: unknown): LoginErrorCode {
  const { code, name, message } = (error ?? {}) as {
    code?: unknown
    name?: unknown
    message?: unknown
  }
  const text = String(message ?? error).toLowerCase()
  if (code === 3001 || /closed|cancel|reject/.test(text))
    return 'user_cancelled'
  if (text.includes('derivation')) return 'derivation_origin_not_allowed'
  if (name === 'TimeoutError' || text.includes('timeout')) return 'timeout'
  return 'ii_error'
}

/** Ask for the delegation, then return to the client with it or with the error. */
export async function complete(
  request: LoginRequest,
  delegate: (request: LoginRequest) => Promise<string>,
  navigate: (url: string) => void
) {
  let fragment: string
  try {
    fragment = encodeResult(request.state, await delegate(request))
  } catch (error) {
    fragment = encodeError(request.state, errorCode(error))
  }
  navigate(`${request.next_url}#${fragment}`)
}
