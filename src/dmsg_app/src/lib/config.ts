import data from '../../dmsg.config.json'
export const config = Object.freeze(data)
export const isExtension = () => typeof chrome !== 'undefined' && !!chrome.runtime?.id
export const MAX_FILE = 100 * 1024 * 1024
export const CHUNK_SIZE = 1024 * 1024
export const MAX_CIPHER_CHUNK = CHUNK_SIZE + 64
export const MAX_CIPHER_UPLOAD = MAX_FILE + 128 * 64 + 64 * 1024
export const AUTO_LOCK_MS = 15 * 60 * 1000
export const MAX_OBJECT_BYTES = 200000
export const MAX_FORMAL_OBJECT_BYTES = 512 * 1024
export const MAX_BACKUP_BYTES = 256 * 1024 * 1024
export const MAX_BACKUP_RECORDS = 10000
