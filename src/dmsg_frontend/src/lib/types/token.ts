export interface SendTokenArgs {
  to: string
  amount: bigint
}

export interface LedgerAPI {
  balance: () => Promise<bigint>
  transfer: (to: string, amount: bigint) => Promise<bigint>
}
