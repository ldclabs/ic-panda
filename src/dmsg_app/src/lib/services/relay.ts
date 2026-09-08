import { config } from '../config'
import { ensure } from '../errors'
import { canonical, hash, unb64 } from '../protocol/codec'

export interface RelayReadiness {
  ready: boolean
  protocol: string
  gates: string[]
  paid_contacts_enabled: boolean
}
export async function inspectRelay(): Promise<RelayReadiness> {
  ensure(config.relayOrigin, 'UNAVAILABLE', '尚未配置中继 API 地址。')
  const response = await fetch(new URL('/ready', config.relayOrigin), {
    credentials: 'omit',
    cache: 'no-store',
    redirect: 'error',
    signal: AbortSignal.timeout(10000)
  })
  ensure(response.ok, 'UNAVAILABLE', '无法取得中继就绪状态。')
  const text = await response.text()
  ensure(text.length <= 16384, 'QUOTA_EXCEEDED')
  const result = JSON.parse(text)
  ensure(
    result.ok === true &&
      result.data?.protocol === config.cloudProtocol &&
      typeof result.data.ready === 'boolean' &&
      Array.isArray(result.data.gates) &&
      result.data.gates.every((v: unknown) => typeof v === 'string'),
    'UNSUPPORTED_PROTOCOL'
  )
  return result.data
}
export function canonicalTarget(url: URL) {
  ensure(!/%2f|%5c/i.test(url.pathname) && !url.pathname.includes('\\'), 'INVALID_INPUT')
  const keys = new Set<string>(),
    entries = [...url.searchParams.entries()]
  for (const [key] of entries) {
    ensure(!keys.has(key), 'INVALID_INPUT')
    keys.add(key)
  }
  entries.sort(([a, av], [b, bv]) => (a < b ? -1 : a > b ? 1 : av < bv ? -1 : av > bv ? 1 : 0))
  const query = new URLSearchParams(entries).toString()
  return url.pathname + (query ? `?${query}` : '')
}
/** Candidate transport only. The caller must supply device PoP and a verified
 * security proof; service readiness is never sufficient to grant authority. */
export async function relayRequest(
  path: string,
  body: unknown,
  pop: string,
  freshUntil: number
) {
  ensure(
    config.environment !== 'production',
    'UNSUPPORTED_PROTOCOL',
    '候选云端合同禁止用于生产。'
  )
  ensure(
    config.relayOrigin &&
      path.startsWith('/v1/') &&
      !path.includes('://') &&
      freshUntil > Date.now() &&
      freshUntil <= Date.now() + 60000,
    'POLICY_STALE'
  )
  const url = new URL(path, config.relayOrigin)
  ensure(url.origin === config.relayOrigin, 'FORBIDDEN')
  canonicalTarget(url)
  unb64(pop, 16384)
  const payload = canonical(body)
  ensure(payload.length <= 262144, 'QUOTA_EXCEEDED')
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/cbor', 'x-dmsg-pop': pop },
    body: payload,
    credentials: 'omit',
    redirect: 'error',
    cache: 'no-store',
    signal: AbortSignal.timeout(15000)
  })
  const text = await response.text()
  ensure(text.length <= 524288, 'QUOTA_EXCEEDED')
  const result = JSON.parse(text)
  ensure(
    response.ok && result.ok === true,
    result.error?.code ?? 'UNAVAILABLE',
    result.error?.message ?? '中继请求未完成。'
  )
  return { data: result.data as unknown, bodyDigest: hash(payload) }
}
