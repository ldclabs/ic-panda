import { afterEach, expect, it, vi } from 'vitest'
import { InboxClient } from '../src/lib/services/inbox'
import fixture from './fixtures/inbox-terminal.json'

afterEach(() => vi.restoreAllMocks())
function client(saved?: unknown) {
  const crypto = { call: vi.fn().mockResolvedValue(saved ? JSON.stringify(saved) : null) }
  const value = new InboxClient(
    { crypto } as never,
    {} as never,
    {} as never,
    'aaaaa-aa',
    fixture.entries[0].value.recipient
  )
  const get = vi.spyOn(value.session, 'get'),
    save = vi
      .spyOn(value as unknown as { save(key: string, value: unknown): Promise<void> }, 'save')
      .mockResolvedValue()
  return { value, get, save, crypto }
}
it('shows a hidden terminal as management metadata and applies the latest cursor', async () => {
  const { value, get, save } = client()
  get
    .mockResolvedValueOnce(fixture)
    .mockResolvedValueOnce({
      entries: [
        { cursor: 4, value: { ...fixture.entries[0].value, resolved: true, archived: true } }
      ],
      next_cursor: 4
    })
    .mockResolvedValueOnce({ entries: [], next_cursor: 4 })
  const orders = await value.list()
  expect(orders).toHaveLength(1)
  expect(orders[0]).toMatchObject({
    state: 'aborted',
    resolved: true,
    archived: true,
    text: ''
  })
  expect(save).toHaveBeenCalledTimes(2)
})
it('rejects hidden content and a terminal from another inbox', async () => {
  for (const extra of [
    { ciphertext: 'YQ' },
    { signed: {} },
    { evidence: {} },
    { recipient: fixture.entries[0].value.sender }
  ]) {
    const { value, get } = client()
    get.mockResolvedValueOnce({
      entries: [{ cursor: 3, value: { ...fixture.entries[0].value, ...extra } }],
      next_cursor: 3
    })
    await expect(value.list()).rejects.toThrow()
  }
})
const cachedLetter = {
  ...fixture.entries[0].value,
  state: 'visible',
  signed: { cose_sign1: 'previously-verified-signature' },
  ciphertext: 'previously-opened-ciphertext',
  text: 'Verified letter',
  read: false
}
it('reuses an opened letter while updating its read state', async () => {
  const { value, get, save, crypto } = client(cachedLetter)
  const updated = { ...cachedLetter, read: true }
  get
    .mockResolvedValueOnce({ entries: [{ cursor: 1, value: updated }], next_cursor: 1 })
    .mockResolvedValueOnce({ entries: [], next_cursor: 1 })
  await expect(value.list()).resolves.toEqual([updated])
  expect(save).toHaveBeenCalledWith(`received:${cachedLetter.order_id}`, updated)
  expect(crypto.call.mock.calls.every(([method]) => method === 'commerceJournal')).toBe(true)
})
it('rejects a substituted sender even when the cached signature and ciphertext match', async () => {
  const { value, get, save } = client(cachedLetter)
  get
    .mockResolvedValueOnce({
      entries: [{ cursor: 1, value: { ...cachedLetter, sender: cachedLetter.recipient } }],
      next_cursor: 1
    })
    .mockResolvedValueOnce({ entries: [], next_cursor: 1 })
  await expect(value.list()).rejects.toMatchObject({ code: 'INTEGRITY_FAILED' })
  expect(save).not.toHaveBeenCalled()
})
