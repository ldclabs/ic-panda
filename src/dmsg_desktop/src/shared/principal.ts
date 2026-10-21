/** Principal text includes a CRC32 checksum and a canonical base32 payload. */
export function validPrincipal(text: string): boolean {
  if (!/^[a-z2-7]+(?:-[a-z2-7]+)*$/.test(text) || text.length > 63) return false
  const alphabet = 'abcdefghijklmnopqrstuvwxyz234567',
    plain = text.replaceAll('-', '')
  let acc = 0,
    bits = 0
  const bytes: number[] = []
  for (const char of plain) {
    acc = (acc << 5) | alphabet.indexOf(char)
    bits += 5
    if (bits >= 8) {
      bits -= 8
      bytes.push((acc >>> bits) & 255)
    }
  }
  if (bytes.length < 4 || bytes.length > 33 || (bits && acc & ((1 << bits) - 1))) return false
  let crc = 0xffffffff
  for (const b of bytes.slice(4)) {
    crc ^= b
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0)
  }
  crc = ~crc >>> 0
  if (bytes.slice(0, 4).some((b, i) => b !== ((crc >>> (24 - i * 8)) & 255))) return false
  let encoded = '',
    buffer = 0,
    n = 0
  for (const b of bytes) {
    buffer = (buffer << 8) | b
    n += 8
    while (n >= 5) {
      n -= 5
      encoded += alphabet[(buffer >>> n) & 31]
    }
  }
  if (n) encoded += alphabet[(buffer << (5 - n)) & 31]
  return encoded.match(/.{1,5}/g)?.join('-') === text
}
