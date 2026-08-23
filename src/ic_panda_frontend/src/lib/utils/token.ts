import {
  ICP_LEDGER_CANISTER_ID,
  TOKEN_LEDGER_CANISTER_ID
} from '$lib/constants'
const locale = new Intl.Locale(globalThis.navigator?.language || 'en')

export interface Token {
  symbol: string
  name: string
  decimals: number
  logo?: string
}

export enum FromStringToTokenError {
  FractionalMoreThan8Decimals,
  InvalidFormat,
  FractionalTooManyDecimals
}

export class TokenAmount {
  private constructor(
    private readonly ulps: bigint,
    public readonly token: Token
  ) {}

  static fromUlps({ amount, token }: { amount: bigint; token: Token }) {
    return new TokenAmount(amount, token)
  }

  static fromString({
    amount,
    token
  }: {
    amount: string
    token: Token
  }): TokenAmount | FromStringToTokenError {
    const ulps = convertStringToUlps(amount, token.decimals)
    return typeof ulps === 'bigint' ? new TokenAmount(ulps, token) : ulps
  }

  static fromNumber({ amount, token }: { amount: number; token: Token }) {
    const value = TokenAmount.fromString({
      amount: amount.toFixed(token.decimals),
      token
    })
    if (value instanceof TokenAmount) return value
    if (value === FromStringToTokenError.FractionalTooManyDecimals) {
      throw new Error(
        `Number ${amount} has more than ${token.decimals} decimals`
      )
    }
    throw new Error(`Invalid number ${amount}`)
  }

  toUlps(): bigint {
    return this.ulps
  }
}

function convertStringToUlps(
  value: string,
  decimals: number
): bigint | FromStringToTokenError {
  const amount = value.trim().replace(/[,']/g, '')
  const match = amount.match(/\d*(\.\d*)?/)
  if (!match || match[0] !== amount) return FromStringToTokenError.InvalidFormat

  const [integral, fractional] = amount.split('.')
  let ulps = 0n
  const one = 10n ** BigInt(decimals)

  try {
    if (integral) ulps += BigInt(integral) * one
    if (fractional) {
      if (fractional.length > decimals) {
        return FromStringToTokenError.FractionalTooManyDecimals
      }
      ulps += BigInt(fractional.padEnd(decimals, '0'))
    }
  } catch {
    return FromStringToTokenError.InvalidFormat
  }
  return ulps
}

export interface TokenInfo extends Token {
  fee: bigint
  one: bigint
  canisterId: string
}

export const ICPToken: TokenInfo = {
  name: 'Internet Computer',
  symbol: 'ICP',
  decimals: 8,
  fee: 10000n,
  one: 100000000n,
  canisterId: ICP_LEDGER_CANISTER_ID
}

export const PANDAToken: TokenInfo = {
  name: 'ICPanda',
  symbol: 'PANDA',
  decimals: 8,
  fee: 10000n,
  one: 100000000n,
  canisterId: TOKEN_LEDGER_CANISTER_ID
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
