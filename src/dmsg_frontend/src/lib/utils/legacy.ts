/** This client is an archive. Keep authentication and historical recovery usable.
 * Some read/recovery operations are ICP updates, so blocking every update would
 * lock users out. Business writes are denied by default at the agent boundary.
 * The service-side freeze must also be enforced by the deployed canisters.
 */
export const LEGACY_READ_ONLY = true

const recoveryCalls = new Set([
  'my_iv',
  'sign_in',
  'ecdh_cose_encrypted_key',
  'vetkd_public_key',
  'vetkd_encrypted_key',
  'download_files_token'
])

export function assertLegacyCallAllowed(method: string): void {
  if (LEGACY_READ_ONLY && !recoveryCalls.has(method)) {
    throw new Error(
      'The legacy app is read-only. New messages, uploads, account changes, and payments are disabled.'
    )
  }
}
