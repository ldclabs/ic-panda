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

export function getPriceNumber(v: number): string {
  if (v < 0.001) {
    return v.toFixed(6)
  } else if (v < 0.01) {
    return v.toFixed(5)
  } else if (v < 0.1) {
    return v.toFixed(4)
  } else if (v < 1) {
    return v.toFixed(3)
  } else if (v < 10) {
    return v.toFixed(2)
  }
  return v.toFixed(1)
}

export function pruneAddress(id: string, long?: boolean): string {
  if (long ?? window.innerWidth >= 640) {
    return id.length > 27 ? id.slice(0, 13) + '...' + id.slice(-11) : id
  }
  return id.length > 15 ? id.slice(0, 7) + '...' + id.slice(-5) : id
}
