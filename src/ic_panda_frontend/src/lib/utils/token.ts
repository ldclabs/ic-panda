import { TokenAmountV2 as TokenAmount, type Token } from '@dfinity/utils'

export { TokenAmountV2 as TokenAmount } from '@dfinity/utils'

const locale = new Intl.Locale(window?.navigator.language || 'en')

export const ICPToken: Token = {
  name: 'Internet Computer',
  symbol: 'ICP',
  decimals: 8
}

export const PANDAToken: Token = {
  name: 'ICPanda',
  symbol: 'PANDA',
  decimals: 8
}

export interface TokenAmountDisplay {
  amount: bigint
  display: string
  detail: string
  symbol: string
}

export function formatToken(val: TokenAmount): TokenAmountDisplay {
  const token1 = 10n ** BigInt(val.token.decimals)
  const amount = val.toUlps()
  const converted = Number(amount) / Number(token1)

  return {
    amount,
    symbol: val.token.symbol,
    display: new Intl.NumberFormat(locale, {
      minimumFractionDigits: 0,
      maximumFractionDigits: 4
    }).format(converted),
    detail: new Intl.NumberFormat(locale, {
      minimumFractionDigits: val.token.decimals,
      maximumFractionDigits: val.token.decimals
    }).format(converted)
  }
}
