import { TokenAmount } from '$lib/utils/token'

export interface SendTokenArgs {
  to: string
  amount: number
  tokenAmount: TokenAmount
}
