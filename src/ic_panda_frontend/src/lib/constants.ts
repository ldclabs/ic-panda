import { PUBLIC_DFX_NETWORK } from '$env/static/public'

export const APP_VERSION = '0.2.0'
export const IS_LOCAL = PUBLIC_DFX_NETWORK === 'local' // 'local' | 'ic'

export const INTERNET_IDENTITY_CANISTER_ID = 'rdmx6-jaaaa-aaaaa-aaadq-cai' // ic & local
export const LUCKYPOOL_CANISTER_ID = 'a7cug-2qaaa-aaaap-ab3la-cai' // ic & local
