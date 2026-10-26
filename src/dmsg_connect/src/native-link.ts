/** Navigation hints only. These URLs carry no payload, invitation secret or callback. */
export function nativeLink(kind: 'pair' | 'request', account?: string, request?: string, universal = true): string {
  if (account !== undefined && !/^[0-9a-v]{20}$/.test(account)) throw new Error('Invalid account')
  if (kind === 'request' && (!account || !request || !/^[0-9a-f]{64}$/.test(request))) throw new Error('Invalid request')
  if (kind === 'pair' && request !== undefined) throw new Error('Unexpected request')
  const url = new URL(universal ? `https://connect.dmsg.net/app/${kind}` : `dmsg://connect/${kind}`)
  if (account) url.searchParams.set('account', account)
  if (request) url.searchParams.set('request', request)
  return url.href
}
