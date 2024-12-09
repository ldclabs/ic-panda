import type { ActorMethod } from '@dfinity/agent'
import type { IDL } from '@dfinity/candid'

export interface PublicTokenOverview {
  address: string
  feesUSD: number
  id: number
  name: string
  priceUSD: number
  priceUSDChange: number
  standard: string
  symbol: string
  totalVolumeUSD: number
  txCount: number
  volumeUSD: number
  volumeUSD1d: number
  volumeUSD7d: number
}

export interface _SERVICE {
  'getToken': ActorMethod<[string], PublicTokenOverview>
}
export declare const idlFactory: IDL.InterfaceFactory
