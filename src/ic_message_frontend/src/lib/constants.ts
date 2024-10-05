const src = globalThis.location?.href || ''

export const APP_VERSION = '2.4.1'
export const IS_LOCAL = src.includes('localhost') || src.includes('127.0.0.1')
export const ENV = IS_LOCAL ? 'local' : 'ic'
export const APP_ORIGIN = IS_LOCAL
  ? 'http://2fvu6-tqaaa-aaaap-akksa-cai.localhost:4943'
  : 'https://dmsg.net'

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local
export const TOKEN_LEDGER_CANISTER_ID = 'druyg-tyaaa-aaaaq-aactq-cai' // ic & local

export const MESSAGE_CANISTER_ID = IS_LOCAL
  ? 'ajuq4-ruaaa-aaaaa-qaaga-cai'
  : 'nscli-qiaaa-aaaaj-qa4pa-cai' // ic & local
export const MASTER_KEY_ID = IS_LOCAL ? 'ICPanda_Messages_Master_Key' : 'PANDA'
