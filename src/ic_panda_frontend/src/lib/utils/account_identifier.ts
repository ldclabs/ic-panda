import type { Principal } from '@icp-sdk/core/principal'
import { sha224 } from '@noble/hashes/sha2'
import { bytesToHex, hexToBytes } from '@noble/hashes/utils'

function crc32(bytes: Uint8Array): Uint8Array {
  let crc = 0xffffffff
  for (const byte of bytes) {
    crc ^= byte
    for (let bit = 0; bit < 8; bit++) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0)
    }
  }
  const out = new Uint8Array(4)
  new DataView(out.buffer).setUint32(0, (crc ^ 0xffffffff) >>> 0, false)
  return out
}

// simplified version from ic-js, removed @dfinity/nns-proto
export class AccountIdentifier {
  private constructor(private readonly bytes: Uint8Array) {}

  public static fromHex(hex: string): AccountIdentifier {
    return new AccountIdentifier(hexToBytes(hex))
  }

  public static fromPrincipal({
    principal,
    subAccount = SubAccount.fromID(0)
  }: {
    principal: Principal
    subAccount?: SubAccount
  }): AccountIdentifier {
    // Hash (sha224) the principal, the subAccount and some padding
    const padding = Array.from('\x0Aaccount-id', (char) => char.charCodeAt(0))

    const shaObj = sha224.create()
    shaObj.update(
      Uint8Array.from([
        ...padding,
        ...principal.toUint8Array(),
        ...subAccount.toUint8Array()
      ])
    )
    const hash = shaObj.digest()

    // Prepend the checksum of the hash and convert to a hex string
    const checksum = crc32(hash)
    const bytes = new Uint8Array([...checksum, ...hash])
    return new AccountIdentifier(bytes)
  }

  public toHex(): string {
    return bytesToHex(this.bytes)
  }

  public toUint8Array(): Uint8Array {
    return this.bytes
  }

  public toNumbers(): number[] {
    return Array.from(this.bytes)
  }

  public toAccountIdentifierHash(): { hash: Uint8Array } {
    return {
      hash: this.toUint8Array()
    }
  }
}

export class SubAccount {
  private constructor(private readonly bytes: Uint8Array) {}

  public static fromBytes(bytes: Uint8Array): SubAccount | Error {
    if (bytes.length != 32) {
      return Error('Subaccount length must be 32-bytes')
    }

    return new SubAccount(bytes)
  }

  public static fromID(id: number): SubAccount {
    if (id < 0) throw new Error('Number cannot be negative')

    if (id > Number.MAX_SAFE_INTEGER) {
      throw new Error('Number is too large to fit in 32 bytes.')
    }

    const view = new DataView(new ArrayBuffer(32))

    // Fix for IOS < 14.8 setBigUint64 absence
    if (typeof view.setBigUint64 === 'function') {
      view.setBigUint64(24, BigInt(id))
    } else {
      const TWO_TO_THE_32 = BigInt(1) << BigInt(32)
      view.setUint32(24, Number(BigInt(id) >> BigInt(32)))
      view.setUint32(28, Number(BigInt(id) % TWO_TO_THE_32))
    }

    const uint8Arary = new Uint8Array(view.buffer)
    return new SubAccount(uint8Arary)
  }

  public toUint8Array(): Uint8Array {
    return this.bytes
  }
}
