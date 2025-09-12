import { createRequest } from '$lib/utils/fetcher'
import { SvelteMap } from 'svelte/reactivity'

const icpswapAPI = createRequest('https://api.icpswap.com', {
  headers: {
    'Accept': 'application/json'
  }
})

export interface TokenPrice {
  symbol: string
  address: string
  priceUSD: number
  priceUSDChange: number
}

export const tokensPrice: SvelteMap<string, TokenPrice> = new SvelteMap()

async function refreshPriceCache() {
  const tokens = await fetchAllTokens()
  tokens.forEach((token) => {
    tokensPrice.set(token.address, token)
  })

  if (tokens.length > 0) {
    setTimeout(refreshPriceCache, 3 * 60 * 1000)
  } else {
    setTimeout(refreshPriceCache, 60 * 1000)
  }
}

refreshPriceCache()

// curl https://api.icpswap.com/info/token/all
// curl https://uvevg-iyaaa-aaaak-ac27q-cai.raw.ic0.app/tickers
// curl -X POST -H "Content-Type: application/json" -d '{"pid":"druyg-tyaaa-aaaaq-aactq-cai"}' https://api.icpswap.com/token/data

// {
//   "code": 200,
//   "message": null,
//   "data": [Item]
// }
// Item = {
//   "tokenLedgerId": "druyg-tyaaa-aaaaq-aactq-cai",
//   "tokenName": "ICPanda",
//   "tokenSymbol": "PANDA",
//   "price": "0.003378752371428711",
//   "priceChange24H": "2.857424506130642600",
//   "tvlUSD": "127420.095786213876646595",
//   "tvlUSDChange24H": "3.300061722599762500",
//   "txCount24H": "33",
//   "volumeUSD24H": "1667.369964079700213096",
//   "volumeUSD7D": "10947.848119320552944907",
//   "totalVolumeUSD": "10785697.958864089915815329",
//   "priceLow24H": "0.000000000000000000",
//   "priceHigh24H": "0.005346426954528124",
//   "priceLow7D": "0.000000000000000000",
//   "priceHigh7D": "0.005532405752083479",
//   "priceLow30D": "0.000000000000000000",
//   "priceHigh30D": "0.005532405752083479"
// }
interface TokenListItem {
  tokenLedgerId: string
  tokenName: string
  tokenSymbol: string
  price: string
  priceChange24H: string
  tvlUSD: string
  tvlUSDChange24H: string
  txCount24H: string
  volumeUSD24H: string
  volumeUSD7D: string
  totalVolumeUSD: string
  priceLow24H: string
  priceHigh24H: string
  priceLow7D: string
  priceHigh7D: string
  priceLow30D: string
  priceHigh30D: string
}

async function fetchAllTokens(): Promise<TokenPrice[]> {
  try {
    const resp: {
      code: number
      message: string | null
      data: TokenListItem[]
    } = await icpswapAPI.get('/info/token/all')

    if (!Array.isArray(resp.data)) {
      throw new Error('Invalid response format: ' + JSON.stringify(resp))
    }

    return resp.data.map((item) => ({
      symbol: item.tokenSymbol,
      address: item.tokenLedgerId,
      priceUSD: parseFloat(item.price),
      priceUSDChange: parseFloat(item.priceChange24H),
      totalVolumeUSD: 0
    }))
  } catch (error) {
    console.error('Error fetching all tokens:', error)
    return []
  }
}
