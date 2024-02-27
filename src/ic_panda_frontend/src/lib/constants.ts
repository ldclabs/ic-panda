import { PUBLIC_APP_VERSION, PUBLIC_DFX_NETWORK } from '$env/static/public'

export const APP_VERSION = PUBLIC_APP_VERSION as string
export const MODE = PUBLIC_DFX_NETWORK as string
export const IS_LOCAL = MODE === 'local'
export const IS_STAGING = MODE === 'staging'
export const IS_PROD = MODE === 'ic'

export const LOCAL_INTERNET_IDENTITY_CANISTER_ID = 'br5f7-7uaaa-aaaaa-qaaca-cai' // test ii
export const LUCKYPOOL_CANISTER_ID = 'bkyz2-fmaaa-aaaaa-qaaaq-cai'
