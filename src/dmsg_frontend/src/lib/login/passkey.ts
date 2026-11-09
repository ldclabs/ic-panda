// dmsg-passkey/1: this page only collects WebAuthn; the client submits it to dmsg_auth.
import { decode, encode } from 'cborg'
import { b64, unb64, callbackTarget } from './protocol'

export interface PasskeyRequest {
  protocol: 'dmsg-passkey/1'
  state: Uint8Array<ArrayBuffer>
  challenge: {
    challenge: Uint8Array<ArrayBuffer>
    expires_at: number
    action: 'register' | 'login' | 'logout_all'
  }
  next_url: string
  credential_id: Uint8Array<ArrayBuffer> | null
}

type PasskeyCredentials = Pick<CredentialsContainer, 'create' | 'get'>

// External security keys do not require a platform authenticator.
export function supportsPasskey(
  credentials: Partial<PasskeyCredentials> | undefined
): credentials is PasskeyCredentials {
  return (
    globalThis.isSecureContext === true &&
    typeof globalThis.PublicKeyCredential !== 'undefined' &&
    typeof credentials?.create === 'function' &&
    typeof credentials?.get === 'function'
  )
}

function check(value: unknown, name: string): asserts value {
  if (!value) throw new Error(name)
}

function keys(
  value: unknown,
  expected: string[]
): value is Record<string, unknown> {
  return (
    !!value &&
    typeof value === 'object' &&
    Object.keys(value).sort().join() === expected.sort().join()
  )
}

export function parsePasskeyRequest(
  fragment: string,
  now = Date.now()
): PasskeyRequest {
  check(fragment.length <= 16_384, 'size')
  const bytes = unb64(fragment)
  const value = decode(bytes, { strict: true, rejectDuplicateMapKeys: true })
  check(
    keys(value, [
      'protocol',
      'state',
      'challenge',
      'next_url',
      'credential_id'
    ]),
    'encoding'
  )
  const encoded = encode(value)
  check(
    encoded.length === bytes.length && encoded.every((b, i) => b === bytes[i]),
    'canonical'
  )
  const request = value as unknown as PasskeyRequest
  check(request.protocol === 'dmsg-passkey/1', 'protocol')
  check(
    request.state instanceof Uint8Array &&
      request.state.length === 32 &&
      request.state.some(Boolean),
    'state'
  )
  check(
    keys(request.challenge, ['challenge', 'expires_at', 'action']),
    'challenge'
  )
  check(
    request.challenge.challenge instanceof Uint8Array &&
      request.challenge.challenge.length === 32,
    'challenge bytes'
  )
  check(
    ['register', 'login', 'logout_all'].includes(request.challenge.action),
    'action'
  )
  check(
    Number.isSafeInteger(request.challenge.expires_at) &&
      request.challenge.expires_at > now &&
      request.challenge.expires_at <= now + 360_000,
    'expiry'
  )
  check(typeof request.next_url === 'string', 'next_url')
  callbackTarget(request.next_url)
  check(
    request.credential_id === null ||
      (request.credential_id instanceof Uint8Array &&
        request.credential_id.length > 0 &&
        request.credential_id.length <= 1024),
    'credential'
  )
  return request
}

export async function completePasskey(
  request: PasskeyRequest,
  credentials: Partial<PasskeyCredentials> | undefined,
  navigate: (url: string) => void
) {
  let response = null
  let error: string | null = null
  try {
    const timeout = Math.min(60_000, request.challenge.expires_at - Date.now())
    if (timeout <= 0) throw new DOMException('Expired', 'TimeoutError')
    if (!supportsPasskey(credentials))
      throw new DOMException('WebAuthn unavailable', 'NotSupportedError')
    const registration = request.challenge.action === 'register'
    const credential = (
      registration
        ? await credentials.create({
            publicKey: {
              challenge: request.challenge.challenge,
              rp: { id: 'dmsg.net', name: 'dMsg' },
              user: { id: request.state, name: 'dMsg', displayName: 'dMsg' },
              pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
              authenticatorSelection: {
                residentKey: 'required',
                requireResidentKey: true,
                userVerification: 'required'
              },
              attestation: 'none',
              timeout
            }
          })
        : await credentials.get({
            publicKey: {
              challenge: request.challenge.challenge,
              rpId: 'dmsg.net',
              userVerification: 'required',
              timeout,
              ...(request.credential_id
                ? {
                    allowCredentials: [
                      { type: 'public-key' as const, id: request.credential_id }
                    ]
                  }
                : {})
            }
          })
    ) as PublicKeyCredential | null
    if (!credential || credential.type !== 'public-key')
      throw new DOMException(
        'Authentication did not complete',
        'NotAllowedError'
      )
    const result = credential.response
    response = {
      credential_id: new Uint8Array(credential.rawId),
      client_data_json: new Uint8Array(result.clientDataJSON),
      authenticator_data: new Uint8Array(
        registration
          ? (result as AuthenticatorAttestationResponse).attestationObject
          : (result as AuthenticatorAssertionResponse).authenticatorData
      ),
      signature: registration
        ? new Uint8Array()
        : new Uint8Array((result as AuthenticatorAssertionResponse).signature)
    }
  } catch (cause) {
    const name = (cause as { name?: string })?.name
    // NotAllowedError also covers timeout, unavailable credentials and policy
    // restrictions. It cannot establish that the user cancelled.
    error =
      name === 'AbortError'
        ? 'user_cancelled'
        : name === 'TimeoutError'
          ? 'timeout'
          : 'passkey_error'
  }
  navigate(
    `${request.next_url}#${b64(encode({ protocol: 'dmsg-passkey/1', state: request.state, response, error }))}`
  )
}
