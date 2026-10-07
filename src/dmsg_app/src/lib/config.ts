import data from '../../dmsg.config.json'
// An empty JSON list infers as never[]; the pinned homes are canister IDs.
export const config = Object.freeze({
  ...data,
  canisters: { ...data.canisters, userHomes: data.canisters.userHomes as string[] }
})
export const isExtension = () => typeof chrome !== 'undefined' && !!chrome.runtime?.id
export const MAX_FILE = 100 * 1024 * 1024
export const CHUNK_SIZE = 1024 * 1024
export const MAX_CIPHER_CHUNK = CHUNK_SIZE + 64
export const MAX_CIPHER_UPLOAD = MAX_FILE + 128 * 64 + 64 * 1024
export const AUTO_LOCK_MS = 15 * 60 * 1000
export const MAX_OBJECT_BYTES = 200000
export const MAX_FORMAL_OBJECT_BYTES = 512 * 1024
export const MAX_SECURITY_EVIDENCE_BYTES = 256 * 1024
/** PRF unlocks are refused after this long without a login unlock. */
export const LOGIN_UNLOCK_INTERVAL_MS = 7 * 24 * 60 * 60 * 1000
