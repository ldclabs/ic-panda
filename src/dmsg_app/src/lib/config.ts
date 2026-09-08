import data from '../../dmsg.config.json'
export const config = Object.freeze(data)
export const isExtension = () => typeof chrome !== 'undefined' && !!chrome.runtime?.id
export const MAX_FILE = 100 * 1024 * 1024
export const CHUNK_SIZE = 1024 * 1024
export const AUTO_LOCK_MS = 15 * 60 * 1000
