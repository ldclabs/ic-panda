const src = globalThis.location?.href || ''

export const APP_VERSION = '2.12.1'
export const IS_LOCAL = src.includes('localhost') || src.includes('127.0.0.1')
export const ENV = IS_LOCAL ? 'local' : 'ic'
export const APP_ORIGIN = IS_LOCAL
  ? 'http://2fvu6-tqaaa-aaaap-akksa-cai.localhost:4943'
  : 'https://dmsg.net'

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const NAME_IDENTITY_CANISTER_ID = '2rgax-kyaaa-aaaap-anvba-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local
export const TOKEN_LEDGER_CANISTER_ID = 'druyg-tyaaa-aaaaq-aactq-cai' // ic & local
export const DMSG_LEDGER_CANISTER_ID = 'ocqzv-tyaaa-aaaar-qal4a-cai' // ic & local

export const MESSAGE_CANISTER_ID = 'nscli-qiaaa-aaaaj-qa4pa-cai' // ic & local
export const MASTER_KEY_ID = IS_LOCAL ? 'ICPanda_Messages_Master_Key' : 'PANDA'

export const ICPSWAP_TOKENS_CANISTER_ID = 'moe7a-tiaaa-aaaag-qclfq-cai'
