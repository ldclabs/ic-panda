import {
  CKDOGE_LEDGER_CANISTER_ID,
  ICP_LEDGER_CANISTER_ID,
  TOKEN_LEDGER_CANISTER_ID
} from '$lib/constants'
import { TokenAmountV2 as TokenAmount, type Token } from '@dfinity/utils'

export { TokenAmountV2 as TokenAmount } from '@dfinity/utils'

const locale = new Intl.Locale(window?.navigator.language || 'en')

export interface TokenInfo extends Token {
  fee: bigint
  one: bigint
  canisterId: string
}

export const ICPToken: TokenInfo = {
  name: 'Dev ICP',
  symbol: 'dICP',
  decimals: 8,
  fee: 10000n,
  one: 100000000n,
  canisterId: ICP_LEDGER_CANISTER_ID
}

export const PANDAToken: TokenInfo = {
  name: 'Dev ICPanda',
  symbol: 'dPANDA',
  decimals: 8,
  fee: 10000n,
  one: 100000000n,
  canisterId: TOKEN_LEDGER_CANISTER_ID
}

export const ckDOGEToken: TokenInfo = {
  name: 'Chain-key DogeCoin',
  symbol: 'ckDOGE',
  decimals: 8,
  fee: 100000n,
  one: 100000000n,
  canisterId: CKDOGE_LEDGER_CANISTER_ID
}

export const DOGEToken: TokenInfo = {
  name: 'DogeCoin',
  symbol: 'DOGE',
  decimals: 8,
  fee: 1000000n,
  one: 100000000n,
  canisterId: '' // local
}

export function formatNumber(val: number, maxDigits: number = 3): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 0,
    maximumFractionDigits: maxDigits,
    roundingMode: 'floor'
  } as Intl.NumberFormatOptions).format(val)
}

export class TokenDisplay {
  readonly billedToSource: boolean
  readonly token: TokenInfo
  readonly one: bigint
  readonly formater: Intl.NumberFormat

  amount: bigint
  fee: bigint

  // Initialize from a string. Accepted formats:
  //   1234567.8901
  //   1'234'567.8901
  //   1,234,567.8901
  //
  static fromString(
    token: TokenInfo,
    amount: string,
    billedToSource: boolean = true
  ): TokenDisplay {
    const val = TokenAmount.fromString({ amount, token }) as TokenAmount
    return new TokenDisplay(token, val.toUlps(), billedToSource)
  }

  // Initialize from a number.
  // 1 integer is considered 10^{token.decimals} ulps
  static fromNumber(
    token: TokenInfo,
    amount: number,
    billedToSource: boolean = true
  ): TokenDisplay {
    const val = TokenAmount.fromNumber({ amount, token }) as TokenAmount
    return new TokenDisplay(token, val.toUlps(), billedToSource)
  }

  constructor(
    token: TokenInfo,
    amount: bigint,
    billedToSource: boolean = true
  ) {
    this.billedToSource = billedToSource
    this.token = token
    this.one = 10n ** BigInt(token.decimals)
    this.formater = new Intl.NumberFormat(locale, {
      minimumFractionDigits: 1,
      maximumFractionDigits: token.decimals,
      roundingMode: 'floor'
    } as Intl.NumberFormatOptions)
    this.amount = amount
    this.fee = token.fee
  }

  get num(): number {
    return Number(this.amount) / Number(this.one)
  }

  set num(amount: number) {
    const val = TokenAmount.fromNumber({
      amount,
      token: this.token
    }) as TokenAmount
    this.amount = val.toUlps()
  }

  get total(): bigint {
    return this.billedToSource ? this.amount + this.fee : this.amount
  }

  get received(): bigint {
    return this.billedToSource ? this.amount : this.amount - this.fee
  }

  fullFormat(value: number | bigint): string {
    return this.formater.format(value)
  }

  short(maxDigits: number = 3): string {
    return formatNumber(this.num, maxDigits)
  }

  toString(): string {
    return this.fullFormat(this.num)
  }

  display(): string {
    return this.toString()
  }

  displayValue(value: bigint): string {
    return this.fullFormat(Number(value) / Number(this.one))
  }

  displayFee(): string {
    return this.displayValue(this.fee)
  }

  displayTotal(): string {
    return this.displayValue(this.total)
  }

  displayReceived(): string {
    return this.displayValue(this.received)
  }
}
