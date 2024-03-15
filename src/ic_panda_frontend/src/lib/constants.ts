import { PUBLIC_APP_VERSION } from '$env/static/public'

export const APP_VERSION = PUBLIC_APP_VERSION
export const IS_LOCAL =
  document?.location.hostname.includes('localhost') ||
  document?.location.hostname.includes('127.0.0.1')

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const LUCKYPOOL_CANISTER_ID = 'a7cug-2qaaa-aaaap-ab3la-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local
export const TOKEN_LEDGER_CANISTER_ID = 'bkyz2-fmaaa-aaaaa-qaaaq-cai' // TODO: replace with the real canister id
