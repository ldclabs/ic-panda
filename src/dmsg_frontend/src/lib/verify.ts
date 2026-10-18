import {
  idlFactory,
  type _SERVICE
} from '$declarations/dmsg_handle/dmsg_handle.did.js'
import { createActor } from '$lib/canisters/actors'
import { DMSG_HANDLE_CANISTER_ID, IS_LOCAL } from '$lib/constants'
import { dynAgent } from '$lib/utils/auth'
import {
  parseAttestationBundle,
  verifyAttestationBundle,
  type AttestationBundle,
  type BundleReport
} from 'dmsg-sdk/bundle'

export type { BundleReport }

/** Why a bundle could not be checked at all. */
export class VerifyError extends Error {
  constructor(readonly reason: 'not_bundle' | 'unreachable') {
    super(reason)
  }
}

/** A bundle from a file's text; anything else is not one. */
export function readBundle(text: string): AttestationBundle {
  try {
    return parseAttestationBundle(JSON.parse(text))
  } catch {
    throw new VerifyError('not_bundle')
  }
}

/**
 * Verify a bundle in this browser against the deployment's user homes, read
 * from the handle registry, and the IC root key, with the original file's
 * bytes when given; the file never leaves the browser.
 */
export async function verifyBundle(
  bundle: AttestationBundle,
  file?: Uint8Array
): Promise<BundleReport> {
  let homes: Uint8Array[]
  try {
    if (IS_LOCAL) await dynAgent.fetchRootKey()
    const handle = createActor<_SERVICE>({
      canisterId: DMSG_HANDLE_CANISTER_ID,
      idlFactory,
      agent: dynAgent
    })
    const config = await handle.get_handle_config()
    homes = config.user_homes.map((home) => home.toUint8Array())
  } catch {
    throw new VerifyError('unreachable')
  }
  const rootKey = dynAgent.rootKey
  if (!rootKey) throw new VerifyError('unreachable')
  return verifyAttestationBundle(bundle, {
    homes,
    rootKey: Uint8Array.from(rootKey),
    file
  })
}
