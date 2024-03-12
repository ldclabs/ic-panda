import { PUBLIC_DFX_NETWORK, PUBLIC_APP_VERSION } from '$env/static/public'

export const APP_VERSION = PUBLIC_APP_VERSION
export const IS_LOCAL = PUBLIC_DFX_NETWORK === 'local' // 'local' | 'ic'

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const LUCKYPOOL_CANISTER_ID = 'a7cug-2qaaa-aaaap-ab3la-cai' // ic & local
export const ICP_LEDGER_CANISTER_ID = 'ryjl3-tyaaa-aaaaa-aaaba-cai' // ic & local