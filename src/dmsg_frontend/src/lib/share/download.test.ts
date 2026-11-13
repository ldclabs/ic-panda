// @vitest-environment node
import { afterEach, expect, it, vi } from 'vitest'
import { ShareDownload, authorizeMetadata } from './download'
import { sha256Hex, type Descriptor } from './decrypt'

const origin = `https://${'1'.repeat(32)}.r2.cloudflarestorage.com`
const api = 'https://api.dmsg.test'
const data = new Uint8Array([1, 2, 3])
async function sample() {
  const part = { size: data.length, digest: await sha256Hex(data) }
  const descriptor: Descriptor = {
    share_id: 'a'.repeat(64),
    upload_id: 'b'.repeat(64),
    object_id: 'a'.repeat(64),
    version_id: 'c'.repeat(64),
    status: 'available',
    expires_at: Date.now() + 3600000,
    max_downloads: 1,
    downloads: 0,
    password: false,
    key_wrap: null,
    created_at: Date.now(),
    manifest: part,
    chunks: [part]
  }
  const grant = {
    upload_id: descriptor.upload_id,
    expires_at: Date.now() + 50000,
    session_expires_at: descriptor.expires_at,
    manifest: { ...part, url: origin + '/bucket/manifest?signature=test' },
    chunks: [{ ...part, url: origin + '/bucket/0?signature=test' }]
  }
  return { descriptor, grant }
}
const json = (data: unknown) => Response.json({ ok: true, data })
afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

it('uses a prefetched metadata authorization without a second grant request or a session', async () => {
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (url: string) =>
    url.startsWith(api) ? json(grant) : new Response(data)
  )
  vi.stubGlobal('fetch', fetcher)
  const initial = await authorizeMetadata(api, descriptor.share_id)
  const download = new ShareDownload(api, descriptor)
  expect(await download.metadata(initial)).toEqual(data)
  expect(
    fetcher.mock.calls.filter(([url]) => url.startsWith(api))
  ).toHaveLength(1)
  expect(download.hasSession).toBe(false)
})

it('refreshes a prefetched metadata URL that expired while entering a password', async () => {
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (url: string) =>
    url.startsWith(api) ? json(grant) : new Response(data)
  )
  vi.stubGlobal('fetch', fetcher)
  const initial = {
    ...grant,
    expires_at: Date.now() - 1000,
    manifest: { ...grant.manifest, url: origin + '/expired' }
  }
  const download = new ShareDownload(api, descriptor)
  expect(await download.metadata(initial)).toEqual(data)
  expect(fetcher.mock.calls.map(([url]) => url)).toEqual([
    `${api}/public/shares/${descriptor.share_id}/metadata`,
    grant.manifest.url
  ])
  expect(download.hasSession).toBe(false)
})

it('reads metadata without a session, then starts one only for file content', async () => {
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
    if (url.endsWith('/metadata')) {
      const { chunks, session_expires_at, ...metadata } = grant
      return json(metadata)
    }
    return url.startsWith(api) ? json(grant) : new Response(data)
  })
  vi.stubGlobal('fetch', fetcher)
  const download = new ShareDownload(api, descriptor)
  expect(await download.metadata()).toEqual(data)
  expect(download.hasSession).toBe(false)
  expect(fetcher.mock.calls.some(([, init]) => init?.method === 'POST')).toBe(
    false
  )
  expect(await download.bytes('chunks/0')).toEqual(data)
  expect(download.hasSession).toBe(true)
  expect(await download.bytes('chunks/0')).toEqual(data)
  expect(
    fetcher.mock.calls.filter(([, init]) => init?.method === 'POST')
  ).toHaveLength(1)
  expect(
    fetcher.mock.calls.filter(([url]) => url === grant.manifest.url)
  ).toHaveLength(1)
})

it('retries metadata URLs without opening a session and checks their digest', async () => {
  const { descriptor, grant } = await sample()
  let reads = 0
  const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
    if (url.startsWith(api)) return json(grant)
    if (reads++ === 0) return new Response(null, { status: 403 })
    return new Response(new Uint8Array([9, 2, 3]))
  })
  vi.stubGlobal('fetch', fetcher)
  await expect(
    new ShareDownload(api, descriptor).metadata()
  ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  const controls = fetcher.mock.calls.filter(([url]) => url.startsWith(api))
  expect(controls.map(([url, init]) => [url, init?.method])).toEqual([
    [`${api}/public/shares/${descriptor.share_id}/metadata`, 'GET'],
    [`${api}/public/shares/${descriptor.share_id}/metadata`, 'GET']
  ])
})

it('uses one session for multiple slots and renews it without another start', async () => {
  vi.useFakeTimers({ toFake: ['Date'] })
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (input: string, init?: RequestInit) =>
    input.startsWith(api)
      ? json({ ...grant, expires_at: Date.now() + 50000 })
      : new Response(data)
  )
  vi.stubGlobal('fetch', fetcher)
  const download = new ShareDownload(api, descriptor)
  expect(await download.bytes('manifest')).toEqual(data)
  expect(await download.bytes('chunks/0')).toEqual(data)
  expect(
    fetcher.mock.calls.filter(([url]) => url.startsWith(api))
  ).toHaveLength(1)
  vi.setSystemTime(Date.now() + 51000)
  expect(await download.bytes('chunks/0')).toEqual(data)
  const controls = fetcher.mock.calls.filter(([url]) => url.startsWith(api))
  expect(controls.map(([, init]) => init?.method)).toEqual(['POST', 'GET'])
  expect(controls[0]![0]).toBe(controls[1]![0])
  for (const [, init] of fetcher.mock.calls)
    expect(init).toMatchObject({
      credentials: 'omit',
      redirect: 'error',
      referrerPolicy: 'no-referrer',
      cache: 'no-store'
    })
})

it('reuses the initial request ID after a lost authorization reply', async () => {
  const { descriptor, grant } = await sample()
  let lost = true
  const fetcher = vi.fn(async (url: string) => {
    if (url.startsWith(api)) {
      if (lost) {
        lost = false
        throw new TypeError('network')
      }
      return json(grant)
    }
    return new Response(data)
  })
  vi.stubGlobal('fetch', fetcher)
  const download = new ShareDownload(api, descriptor)
  await expect(download.bytes('manifest')).rejects.toThrow('network')
  expect(await download.bytes('manifest')).toEqual(data)
  expect(fetcher.mock.calls[0]![0]).toBe(fetcher.mock.calls[1]![0])
})

it('rechecks authority after an expired URL and stops on revocation', async () => {
  const { descriptor, grant } = await sample()
  let controls = 0
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) =>
      url.startsWith(api)
        ? ++controls === 1
          ? json(grant)
          : Response.json({ error: { message: 'revoked' } }, { status: 410 })
        : new Response(null, { status: 403 })
    )
  )
  await expect(
    new ShareDownload(api, descriptor).bytes('manifest')
  ).rejects.toMatchObject({ gone: 'revoked' })
  expect(controls).toBe(2)
})

it('rejects corrupt bytes and unexpected download hosts without exposing credentials', async () => {
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (url: string) =>
    url.startsWith(api) ? json(grant) : new Response(new Uint8Array([1, 2, 4]))
  )
  vi.stubGlobal('fetch', fetcher)
  await expect(
    new ShareDownload(api, descriptor).bytes('manifest')
  ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  grant.manifest.url = 'http://127.0.0.1/private'
  fetcher.mockClear()
  await expect(
    new ShareDownload(api, descriptor).bytes('manifest')
  ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  expect(fetcher).toHaveBeenCalledTimes(1)
})

it('fails missing committed bytes after one bounded renewal', async () => {
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (url: string) =>
    url.startsWith(api) ? json(grant) : new Response(null, { status: 404 })
  )
  vi.stubGlobal('fetch', fetcher)
  await expect(
    new ShareDownload(api, descriptor).bytes('manifest')
  ).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  expect(fetcher).toHaveBeenCalledTimes(4)
})

it.each(['connection', 'timeout', 'stream', 'throttled'])(
  'renews the same session and retries only the failed block after %s failure',
  async (failure) => {
    const { descriptor, grant } = await sample()
    let failed = false
    const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
      if (url.startsWith(api)) return json(grant)
      if (url === grant.chunks[0]!.url && !failed) {
        failed = true
        if (failure === 'connection') throw new TypeError(`network: ${url}`)
        if (failure === 'timeout')
          throw new DOMException('timed out', 'TimeoutError')
        if (failure === 'throttled') return new Response(null, { status: 429 })
        let reads = 0
        return new Response(
          new ReadableStream({
            pull(controller) {
              if (reads++ === 0) controller.enqueue(data.slice(0, 1))
              else controller.error(new TypeError(`network: ${url}`))
            }
          })
        )
      }
      return new Response(data)
    })
    vi.stubGlobal('fetch', fetcher)
    const download = new ShareDownload(api, descriptor)
    expect(await download.bytes('manifest')).toEqual(data)
    expect(await download.bytes('chunks/0')).toEqual(data)
    const controls = fetcher.mock.calls.filter(([url]) => url.startsWith(api))
    expect(controls.map(([, init]) => init?.method)).toEqual(['POST', 'GET'])
    expect(controls[0]![0]).toBe(controls[1]![0])
    expect(
      fetcher.mock.calls.filter(([url]) => url === grant.manifest.url)
    ).toHaveLength(1)
    expect(
      fetcher.mock.calls.filter(([url]) => url === grant.chunks[0]!.url)
    ).toHaveLength(2)
  }
)

it('bounds repeated network failures and hides the bearer URL in the error', async () => {
  const { descriptor, grant } = await sample()
  const fetcher = vi.fn(async (url: string) => {
    if (url.startsWith(api)) return json(grant)
    throw new TypeError(`network: ${url}`)
  })
  vi.stubGlobal('fetch', fetcher)
  await expect(
    new ShareDownload(api, descriptor).bytes('manifest')
  ).rejects.toThrow(/^File download unavailable$/)
  expect(fetcher).toHaveBeenCalledTimes(4)
})
