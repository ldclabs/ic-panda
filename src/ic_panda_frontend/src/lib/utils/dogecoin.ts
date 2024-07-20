import { sha256 } from '@noble/hashes/sha2'
import { createBase58check, hex } from '@scure/base'

const base58check = createBase58check(sha256)

export interface ChainParams {
  chain_name: string
  p2pkh_address_prefix: number
  p2sh_address_prefix: number
  pkey_prefix: number
}

export const DOGE_MAIN: ChainParams = {
  chain_name: 'main',
  p2pkh_address_prefix: 0x1e, // D
  p2sh_address_prefix: 0x16, // 9 or A
  pkey_prefix: 0x9e // Q or 6
}

export const DOGE_TEST: ChainParams = {
  chain_name: 'test',
  p2pkh_address_prefix: 0x71, // n
  p2sh_address_prefix: 0xc4, // 2
  pkey_prefix: 0xf1 // 9 or c
}

export const DOGE_REGTEST: ChainParams = {
  chain_name: 'regtest',
  p2pkh_address_prefix: 0x6f, // n
  p2sh_address_prefix: 0xc4, // 2
  pkey_prefix: 0xef //
}

export function toHashString(bytes: Uint8Array | number[]): string {
  return hex.encode(Uint8Array.from(bytes).reverse())
}

export class Chain {
  readonly chain: ChainParams

  constructor(chain: string) {
    switch (chain) {
      case 'main':
        this.chain = DOGE_MAIN
        break
      case 'regtest':
        this.chain = DOGE_REGTEST
        break
      default:
        this.chain = DOGE_TEST
    }
  }

  decodeAddress(address: string): Uint8Array {
    const addr = base58check.decode(address)
    if (addr.length !== 21) {
      throw new Error('invalid address length')
    }

    // only support p2pkh address for now
    if (addr[0] === this.chain.p2pkh_address_prefix) {
      return addr
    }

    throw new Error(
      `invalid address prefix for ${this.chain.chain_name} network`
    )
  }

  get explorer(): string {
    switch (this.chain.chain_name) {
      case 'main':
        return 'https://sochain.com/DOGE'
      default:
        return 'https://sochain.com/DOGETEST'
    }
  }

  blockExplorer(height: bigint): string {
    switch (this.chain.chain_name) {
      case 'main':
        return 'https://sochain.com/block/DOGE/' + height
      default:
        return 'https://sochain.com/block/DOGETEST/' + height
    }
  }

  addressExplorer(address: string): string {
    if (address.length < 30) {
      return this.explorer
    }

    switch (this.chain.chain_name) {
      case 'main':
        return 'https://sochain.com/address/DOGE/' + address
      default:
        return 'https://sochain.com/address/DOGETEST/' + address
    }
  }

  txExplorer(txid: string): string {
    if (txid.length < 30) {
      return this.explorer
    }

    switch (this.chain.chain_name) {
      case 'main':
        return 'https://sochain.com/tx/DOGE/' + txid
      default:
        return 'https://sochain.com/tx/DOGETEST/' + txid
    }
  }
}
