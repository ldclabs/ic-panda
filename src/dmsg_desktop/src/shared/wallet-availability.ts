/** Release gate shared by desktop controls and the main-process RPC boundary.
 * Existing wallets and previously authorized activation/refund work remain available.
 * Reopen creation here after the team-wallet redesign is ready for users.
 */
export const WALLET_CREATION_PAUSED = true
