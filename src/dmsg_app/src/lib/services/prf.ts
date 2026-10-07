import { currentWorkspace } from '../db'
import { prfSalt } from '../crypto/primitives'
import { b64, random, unb64, utf8 } from '../protocol/codec'
import { ensure } from '../errors'
import type { WorkspaceMeta } from '../models'

/** Platform-authenticator unlock through the WebAuthn PRF extension: the
 * secret stays in the authenticator, which releases a stable 32-byte output
 * for this workspace's salt after user verification. */
export const prfSupported = () =>
  typeof navigator !== 'undefined' && !!navigator.credentials && !!window.PublicKeyCredential
async function salt() {
  const name = await currentWorkspace()
  ensure(name, 'RECOVERY_INCOMPLETE')
  return prfSalt(name)
}
function output(credential: Credential | null) {
  const results = (credential as PublicKeyCredential | null)?.getClientExtensionResults() as
    | { prf?: { results?: { first?: ArrayBuffer } } }
    | undefined
  const first = results?.prf?.results?.first
  ensure(first && first.byteLength === 32, 'UNSUPPORTED_PROTOCOL', '此认证器不支持 PRF 输出。')
  return new Uint8Array(first)
}
/** Create a platform credential and evaluate its PRF once, for `enablePrf`. */
export async function createPrfCredential(meta: WorkspaceMeta) {
  ensure(prfSupported(), 'UNSUPPORTED_PROTOCOL', '此浏览器不支持平台认证器。')
  const created = (await navigator.credentials.create({
    publicKey: {
      challenge: random(32),
      rp: { name: 'dMsg' },
      user: { id: utf8(meta.deviceId), name: meta.deviceId.slice(0, 16), displayName: 'dMsg 设备' },
      pubKeyCredParams: [
        { type: 'public-key', alg: -8 },
        { type: 'public-key', alg: -7 },
        { type: 'public-key', alg: -257 }
      ],
      authenticatorSelection: {
        authenticatorAttachment: 'platform',
        residentKey: 'required',
        userVerification: 'required'
      },
      extensions: { prf: {} } as AuthenticationExtensionsClientInputs
    }
  })) as PublicKeyCredential | null
  ensure(created, 'UNSUPPORTED_PROTOCOL')
  const credentialId = b64(new Uint8Array(created.rawId))
  return { credentialId, output: await evaluatePrf(credentialId) }
}
/** One user verification; the PRF output never leaves page memory. */
export async function evaluatePrf(credentialId: string) {
  ensure(prfSupported(), 'UNSUPPORTED_PROTOCOL', '此浏览器不支持平台认证器。')
  const assertion = await navigator.credentials.get({
    publicKey: {
      challenge: random(32),
      allowCredentials: [{ type: 'public-key', id: unb64(credentialId) }],
      userVerification: 'required',
      extensions: { prf: { eval: { first: await salt() } } } as AuthenticationExtensionsClientInputs
    }
  })
  return output(assertion)
}
