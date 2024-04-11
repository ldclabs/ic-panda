import { base64ToBytes } from '$lib/utils/crypto'
import { decode } from 'cborg'

//(Issuer code, Issue time, Expire, Claimable amount, Quantity)
export type Prize = [number, number, number, number, number]

export function decodePrize(prize: string): Prize | null {
  try {
    const cryptogram = decode(base64ToBytes(prize))
    return decode(cryptogram[0])
  } catch (error) {
    console.error('Error decoding prize:', error)
    return null
  }
}
