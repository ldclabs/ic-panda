import { decodeCBOR, encodeCBOR } from '@ldclabs/cose-ts/utils'

export type URLSearchParamsInit =
  | URLSearchParams
  | string
  | Record<string, string | number | boolean | null | undefined>

export function joinURL(
  baseURL: string,
  path: string,
  params?: URLSearchParamsInit
) {
  const url = new URL(baseURL)
  if (path) {
    if (!url.pathname.endsWith('/')) url.pathname += '/'
    if (path.startsWith('/')) path = path.slice(1)
    url.pathname += path
  }

  toURLSearchParams(params ?? {}).forEach((value, key) => {
    url.searchParams.set(key, value)
  })
  return url.href
}

export function joinURLPath(path: string, params?: URLSearchParamsInit) {
  const baseURL = 'http://localhost/'
  return joinURL(baseURL, path, params).slice(baseURL.length)
}

export function isURL(url: string) {
  try {
    new URL(url)
    return true
  } catch {
    return false
  }
}

export function isBlobURL(url: string | null | undefined): url is string {
  try {
    if (!url) return false
    const { protocol } = new URL(url)
    return protocol === 'blob:'
  } catch {
    return false
  }
}

export function toURLSearchParams(params: URLSearchParamsInit) {
  if (params instanceof URLSearchParams) return params
  if (typeof params === 'string') return new URLSearchParams(params)
  return new URLSearchParams(
    Object.entries(params)
      .filter((kv) => kv[1] != null)
      .map(([k, v]) => [k, String(v)])
  )
}

export function createBlobURL(object: unknown) {
  return btoa(
    URL.createObjectURL(
      new Blob([encodeCBOR(object) as BufferSource], {
        type: 'application/cbor'
      })
    )
  )
}

export async function parseBlobURL<T>(url: string) {
  try {
    url = atob(url)
    if (!isBlobURL(url)) return null
    const resp = await fetch(url)
    const blob = await resp.blob()
    if (blob.type !== 'application/cbor') return null
    const buffer = await blob.arrayBuffer()
    return decodeCBOR(new Uint8Array(buffer)) as T
  } catch {
    return null
  }
}

export function revokeBlobURL(url: string) {
  try {
    url = atob(url)
    isBlobURL(url) && URL.revokeObjectURL(url)
  } catch {
    // ...
  }
}
