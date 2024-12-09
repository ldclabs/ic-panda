const src = globalThis.location?.href || ''

export const APP_VERSION = '2.8.0'
export const IS_LOCAL = src.includes('localhost') || src.includes('127.0.0.1')
export const ENV = IS_LOCAL ? 'local' : 'ic'
export const APP_ORIGIN = IS_LOCAL
  ? 'http://c63a7-6yaaa-aaaap-ab3gq-cai.localhost:4943'
  : 'https://panda.fans'

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const LUCKYPOOL_CANISTER_ID = 'a7cug-2qaaa-aaaap-ab3la-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local
export const TOKEN_LEDGER_CANISTER_ID = 'druyg-tyaaa-aaaaq-aactq-cai' // ic & local
export const CKDOGE_MINTER_CANISTER_ID = 'bw4dl-smaaa-aaaaa-qaacq-cai' // local
export const CKDOGE_CHAIN_CANISTER_ID = 'be2us-64aaa-aaaaa-qaabq-cai' // local
export const CKDOGE_LEDGER_CANISTER_ID = 'b77ix-eeaaa-aaaaa-qaada-cai' // local

export const MESSAGE_CANISTER_ID = IS_LOCAL
  ? 'ajuq4-ruaaa-aaaaa-qaaga-cai'
  : 'nscli-qiaaa-aaaaj-qa4pa-cai' // ic & local
export const MASTER_KEY_ID = IS_LOCAL ? 'ICPanda_Messages_Master_Key' : 'PANDA'

export const X_AUTH_KEY = 'ICPanda:XAuth'
export const X_AUTH_ENDPIONT = 'https://auth.panda.fans/idp/twitter/authorize'
export const X_AUTH_CHALLENGE_ENDPIONT = 'https://auth.panda.fans/challenge'
