export async function sleep(ms: number): Promise<void> {
  return new Promise((res) => setTimeout(res, ms))
}

export function getCurrentTimeString(ts: bigint | number): string {
  const now = Date.now()
  const t = Number(ts)
  if (t >= now - 24 * 3600 * 1000) {
    return new Date(t).toLocaleTimeString()
  } else if (t >= now - 7 * 24 * 3600 * 1000) {
    return new Date(t).toLocaleDateString(undefined, { weekday: 'long' })
  }
  return new Date(t).toLocaleDateString()
}

export function getBytesString(bytes: number | BigInt): string {
  const n = Number(bytes)
  if (n < 1024) {
    return `${n}`
  } else if (n < 1024 * 1024) {
    return `${(n / 1024).toFixed(2)}KB`
  } else if (n < 1024 * 1024 * 1024) {
    return `${(n / 1024 / 1024).toFixed(2)}MB`
  }
  return `${(n / 1024 / 1024 / 1024).toFixed(2)}GB`
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
