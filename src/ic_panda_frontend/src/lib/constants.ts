import { PUBLIC_APP_VERSION, PUBLIC_DFX_NETWORK } from "$env/static/public";

export const APP_VERSION = PUBLIC_APP_VERSION as string;
export const MODE = PUBLIC_DFX_NETWORK as string;
export const IS_LOCAL = MODE === "local";
export const IS_STAGING = MODE === "staging";
export const IS_PROD = MODE === "ic";

export const INTERNET_IDENTITY_CANISTER_ID = "rdmx6-jaaaa-aaaaa-aaadq-cai"; // ic & local
export const LUCKYPOOL_CANISTER_ID = "a7cug-2qaaa-aaaap-ab3la-cai"; // ic & local
