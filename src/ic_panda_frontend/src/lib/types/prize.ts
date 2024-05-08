import { base64ToBytes, decodeCBOR } from '@ldclabs/cose-ts/utils'

//(Issuer code, Issue time, Expire, Claimable amount, Quantity)
export type Prize = [number, number, number, number, number]

export function decodePrize(code: string): Prize | null {
  if (code.startsWith('PRIZE:')) code = code.slice(6)
  if (!code) return null

  try {
    const cryptogram: Uint8Array[] = decodeCBOR(base64ToBytes(code))
    const prize: Prize = decodeCBOR(cryptogram[0] as Uint8Array)
    if (prize.length !== 5 || !prize[3] || !prize[4]) return null
    return prize
  } catch (_) {
    return null
  }
}

export function decodeAirdropCode(code: string): Prize | null {
  if (!code) return null

  try {
    const cryptogram: Uint8Array[] = decodeCBOR(base64ToBytes(code))
    const prize: Prize = decodeCBOR(cryptogram[0] as Uint8Array)
    if (prize.length !== 5 || prize[3] > 0 || !prize[4]) return null
    return prize
  } catch (_) {
    return null
  }
}
