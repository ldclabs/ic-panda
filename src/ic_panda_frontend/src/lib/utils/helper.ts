export async function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms))
}

export function getShortNumber(v: number | BigInt): string {
  const n = Number(v)
  if (n < 1000) {
    return `${n}`
  } else if (n < 1000 * 1000) {
    return `${(n / 1000).toFixed(2)}K`
  } else if (n < 1000 * 1000 * 1000) {
    return `${(n / 1000 / 1000).toFixed(2)}M`
  } else if (n < 1000 * 1000 * 1000 * 1000) {
    return `${(n / 1000 / 1000 / 1000).toFixed(2)}G`
  }
  return `${(n / 1000 / 1000 / 1000 / 1000).toFixed(2)}T`
}

export function shortId(id: string, long: boolean = false): string {
  if (long) {
    return id.length > 28 ? id.slice(0, 14) + '...' + id.slice(-14) : id
  }
  return id.length > 14 ? id.slice(0, 7) + '...' + id.slice(-7) : id
}
