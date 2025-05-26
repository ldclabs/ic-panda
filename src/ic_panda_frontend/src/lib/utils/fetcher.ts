import { decodeCBOR, encodeCBOR } from '@ldclabs/cose-ts/utils'
import { joinURL, toURLSearchParams, type URLSearchParamsInit } from './url'

const CBOR_MIME_TYPE = 'application/cbor'
const JSON_MIME_TYPE = 'application/json'

export enum RequestMethod {
  GET = 'GET',
  POST = 'POST',
  PUT = 'PUT',
  PATCH = 'PATCH',
  DELETE = 'DELETE'
}

export interface HTTPError {
  code: number
  message: string
  data?: any
  requestId?: string | null
}

export interface ErrorResponse {
  error: HTTPError
}

export interface SuccessResponse<T> {
  result: T
}

export function createRequest(baseURL: string, defaultOptions: RequestInit) {
  const request = async <T>(
    path: string,
    params?: URLSearchParamsInit,
    options?: RequestInit
  ) => {
    const pa = toURLSearchParams(params ?? {})
    const url =
      path.startsWith('http://') || path.startsWith('https://')
        ? joinURL(path, '', pa)
        : joinURL(baseURL, path, pa)

    const headers = new Headers(defaultOptions.headers)
    new Headers(options?.headers).forEach((value, key) =>
      headers.set(key, value)
    )
    if (!headers.has('Accept')) headers.set('Accept', CBOR_MIME_TYPE)

    if (options) {
      options.mode = 'cors'
    }
    const resp = await fetch(url, { ...defaultOptions, ...options, headers })
    const { status } = resp
    const body =
      resp.headers.get('Content-Type') === CBOR_MIME_TYPE
        ? mapToObj(decodeCBOR(new Uint8Array(await resp.arrayBuffer())))
        : resp.headers.get('Content-Type')?.startsWith(JSON_MIME_TYPE)
          ? await resp.json()
          : await resp.text()
    if (resp.ok) {
      return body as T
    } else {
      const requestId = resp.headers.get('X-Request-Id')
      throw createHTTPError(status, body, requestId)
    }
  }

  request.defaultOptions = Object.freeze(defaultOptions)
  request.get = <T>(
    path: string,
    params?: URLSearchParamsInit,
    signal: AbortSignal | null | undefined = null
  ) => {
    return request<T>(path, params, {
      method: RequestMethod.GET,
      signal
    })
  }
  request.post = <T>(
    path: string,
    body?: object,
    signal: AbortSignal | null | undefined = null
  ) => {
    return request<T>(path, undefined, {
      method: RequestMethod.POST,
      body: body ? (encodeCBOR(body) as BufferSource) : null,
      headers: { 'Content-Type': CBOR_MIME_TYPE },
      signal
    })
  }
  request.put = <T>(
    path: string,
    body?: object,
    signal: AbortSignal | null | undefined = null
  ) => {
    return request<T>(path, undefined, {
      method: RequestMethod.PUT,
      body: body ? (encodeCBOR(body) as BufferSource) : null,
      headers: { 'Content-Type': CBOR_MIME_TYPE },
      signal
    })
  }
  request.patch = <T>(
    path: string,
    body?: object,
    signal: AbortSignal | null | undefined = null
  ) => {
    return request<T>(path, undefined, {
      method: RequestMethod.PATCH,
      body: body ? (encodeCBOR(body) as BufferSource) : null,
      headers: { 'Content-Type': CBOR_MIME_TYPE },
      signal
    })
  }
  request.delete = <T>(
    path: string,
    params?: URLSearchParamsInit,
    body?: object,
    signal: AbortSignal | null | undefined = null
  ) => {
    return request<T>(path, params, {
      method: RequestMethod.DELETE,
      body: body ? (encodeCBOR(body) as BufferSource) : null,
      headers: { 'Content-Type': CBOR_MIME_TYPE },
      signal
    })
  }
  return request
}

//#region request error
function createHTTPError(
  status: number,
  body: unknown,
  requestId: string | null
): HTTPError {
  if (typeof body === 'object' && !!body) {
    if ('error' in body && typeof body.error === 'object') {
      return { ...body.error, requestId } as HTTPError
    }
    if ('message' in body && typeof body.message === 'string') {
      return {
        code: status,
        message: body.message,
        data: body,
        requestId
      } as HTTPError
    }
    return { code: status, message: JSON.stringify(body), requestId }
  }

  return { code: status, message: String(body), requestId }
}

export function mapToObj(m: any) {
  if (!(m instanceof Map)) {
    return m
  }

  const obj: any = {}
  for (const [key, val] of m) {
    obj[typeof key === 'string' ? key : String(key)] = val
  }
  return obj
}
