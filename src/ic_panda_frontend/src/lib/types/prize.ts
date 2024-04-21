import { base64ToBytes, decodeCBOR } from '@ldclabs/cose-ts/utils'

//(Issuer code, Issue time, Expire, Claimable amount, Quantity)
export type Prize = [number, number, number, number, number]

export function decodePrize(prize: string): Prize | null {
  if (prize.startsWith('PRIZE:')) prize = prize.slice(6)
  if (!prize) return null

  try {
    const cryptogram: Uint8Array[] = decodeCBOR(base64ToBytes(prize))
    return decodeCBOR(cryptogram[0] as Uint8Array)
  } catch (error) {
    console.error('Error decoding prize:', error)
    return null
  }
}
