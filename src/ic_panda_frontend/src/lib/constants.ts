const src = globalThis.location?.href || ''

export const APP_VERSION = '2.12.1'
export const IS_LOCAL = src.includes('localhost') || src.includes('127.0.0.1')
export const ENV = IS_LOCAL ? 'local' : 'ic'
export const APP_ORIGIN = IS_LOCAL
  ? 'http://c63a7-6yaaa-aaaap-ab3gq-cai.localhost:4943'
  : 'https://panda.fans'

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const LUCKYPOOL_CANISTER_ID = 'a7cug-2qaaa-aaaap-ab3la-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local
export const TOKEN_LEDGER_CANISTER_ID = 'druyg-tyaaa-aaaaq-aactq-cai' // ic & local

export const X_AUTH_KEY = 'ICPanda:XAuth'
export const X_AUTH_ENDPIONT = 'https://auth.panda.fans/idp/twitter/authorize'
export const X_AUTH_CHALLENGE_ENDPIONT = 'https://auth.panda.fans/challenge'

export const ICPSWAP_TOKENS_CANISTER_ID = 'moe7a-tiaaa-aaaag-qclfq-cai'
