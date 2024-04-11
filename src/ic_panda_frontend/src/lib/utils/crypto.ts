export function bytesToBase64Url(bytes: Uint8Array): string {
  return btoa(String.fromCodePoint(...bytes))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replaceAll('=', '')
}

export function base64ToBytes(str: string): Uint8Array {
  return Uint8Array.from(
    atob(str.replaceAll('-', '+').replaceAll('_', '/')),
    (m) => m.codePointAt(0)!
  )
}
