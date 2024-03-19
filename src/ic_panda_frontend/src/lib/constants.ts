import { PUBLIC_APP_VERSION } from '$env/static/public'

export const APP_VERSION = PUBLIC_APP_VERSION
export const IS_LOCAL =
  document?.location.hostname.includes('localhost') ||
  document?.location.hostname.includes('127.0.0.1')

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const LUCKYPOOL_CANISTER_ID = 'j2o2p-baaaa-aaaap-acbaa-cai' // dev
export const ICP_LEDGER_CANISTER_ID = 'ck2fz-byaaa-aaaap-aca6q-cai' // dev
export const TOKEN_LEDGER_CANISTER_ID = 'ceyir-2iaaa-aaaap-aca7q-cai' // dev
