import { icpSwapAPI } from '$lib/canisters/icpswap'
import { readable, type Readable } from 'svelte/store'

export interface TokenPrice {
  symbol: string
  address: string
  priceUSD: number
  priceUSDChange: number
  totalVolumeUSD: number
}

const priceCache: Record<string, TokenPrice> = {}

export function getTokenPrice(
  token: string,
  syncing: boolean = false
): Readable<TokenPrice | null> {
  return readable<TokenPrice | null>(priceCache[token] || null, (set) => {
    let s = syncing
    const f = () =>
      icpSwapAPI.getToken(token).then((data) => {
        if (data) {
          priceCache[token] = data
          set(data)
        }
        s && setTimeout(f, 60000)
      })

    f()
    return () => (s = false)
  })
}
