import { API_ORIGIN } from '$lib/constants'

export function normalizePromo(input: string) {
  if (input.length > 96 || !/^[a-zA-Z0-9\s-]+$/.test(input)) return ''
  const value = input.replace(/[\s-]/g, '').toUpperCase()
  return /^[A-Z0-9]{4,32}$/.test(value) ? value : ''
}
export type PublicPromo = {
  valid?: boolean
  title?: string
  plan?: string
  duration_days?: number
  ends_at_ms?: number
  minimum_versions?: string
}
/** Private codes stay in the fragment. Only public aliases are eligible for a preview. */
export async function publicPromo(
  input: string,
  signal: AbortSignal,
  surface: 'landing' | 'offer' = 'landing'
): Promise<PublicPromo> {
  const code = normalizePromo(input)
  const privateCode = code.startsWith('DMSG')
  const res = await fetch(API_ORIGIN + '/public/promos/preview', {
    method: 'POST',
    credentials: 'omit',
    referrerPolicy: 'no-referrer',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(code && !privateCode ? { code, surface } : {}),
    signal
  })
  if (!res.ok) throw new Error('Preview unavailable')
  const result = await res.json()
  if (!result.ok) throw new Error('Preview unavailable')
  if (!code || privateCode)
    return { minimum_versions: result.data.minimum_versions }
  return result.data
}
