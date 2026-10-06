const src = globalThis.location?.href || ''

export const APP_VERSION = '3.1.2'
export const IS_LOCAL = src.includes('localhost') || src.includes('127.0.0.1')

export const LUCKYPOOL_CANISTER_ID = 'a7cug-2qaaa-aaaap-ab3la-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local
export const TOKEN_LEDGER_CANISTER_ID = 'druyg-tyaaa-aaaaq-aactq-cai' // ic & local

export const ICPSWAP_TOKENS_CANISTER_ID = 'moe7a-tiaaa-aaaag-qclfq-cai'
