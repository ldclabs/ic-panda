import { TokenAmountV2 as TokenAmount, type Token } from '@dfinity/utils'

export { TokenAmountV2 as TokenAmount } from '@dfinity/utils'

const locale = new Intl.Locale(window?.navigator.language || 'en')

export interface TokenInfo extends Token {
  fee: bigint
  one: bigint
}

export const ICPToken: TokenInfo = {
  name: 'Internet Computer',
  symbol: 'ICP',
  decimals: 8,
  fee: 10000n,
  one: 100000000n
}

export const PANDAToken: TokenInfo = {
  name: 'ICPanda',
  symbol: 'PANDA',
  decimals: 8,
  fee: 10000n,
  one: 100000000n
}

export interface TokenAmountDisplay {
  amount: bigint
  amountNum: number
  display: string
  full: string
  symbol: string
  feeAndAmount: bigint
  feeAndFull: string
}

export function formatToken(val: TokenAmount): TokenAmountDisplay {
  const token1 = 10n ** BigInt(val.token.decimals)
  const amount = val.toUlps()
  const converted = Number(amount) / Number(token1)
  const fee = (val.token as TokenInfo).fee || 0n
  const fullFormat = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: val.token.decimals,
    roundingMode: 'floor'
  } as Intl.NumberFormatOptions)

  return {
    amount,
    amountNum: converted,
    feeAndAmount: amount + fee,
    symbol: val.token.symbol,
    display: new Intl.NumberFormat(locale, {
      minimumFractionDigits: 0,
      maximumFractionDigits: 3,
      roundingMode: 'floor'
    } as Intl.NumberFormatOptions).format(converted),
    full: fullFormat.format(converted),
    feeAndFull: fullFormat.format(Number(amount + fee) / Number(token1))
  }
}

export function formatNumber(val: number, maxDigits: number = 3): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 0,
    maximumFractionDigits: maxDigits,
    roundingMode: 'floor'
  } as Intl.NumberFormatOptions).format(val)
}
