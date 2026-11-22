import { afterEach, expect, it, vi } from 'vitest'
import { normalizePromo, publicPromo } from './promo'

afterEach(() => vi.unstubAllGlobals())
it('keeps private codes and the referring link out of anonymous requests', async () => {
  const fetch = vi.fn(async () =>
    Response.json({ ok: true, data: { valid: true } })
  )
  vi.stubGlobal('fetch', fetch)
  const signal = new AbortController().signal
  expect(await publicPromo('dmsg-k7p4-9x2m-3q8r', signal)).not.toHaveProperty(
    'valid'
  )
  expect(await publicPromo(' PH-PLUS ', signal)).toMatchObject({ valid: true })
  await publicPromo('DMSG-K7P4-9X', signal)
  expect(fetch).toHaveBeenNthCalledWith(
    1,
    expect.stringMatching(/\/public\/promos\/preview$/),
    expect.objectContaining({
      body: '{}',
      referrerPolicy: 'no-referrer',
      credentials: 'omit'
    })
  )
  expect(fetch).toHaveBeenNthCalledWith(
    2,
    expect.any(String),
    expect.objectContaining({ body: '{"code":"PHPLUS","surface":"landing"}' })
  )
  expect(fetch).toHaveBeenNthCalledWith(
    3,
    expect.any(String),
    expect.objectContaining({ body: '{}' })
  )
  expect(normalizePromo('file#private')).toBe('')
})
