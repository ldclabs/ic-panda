import { afterEach, expect, it, vi } from 'vitest'
import { InboxClient } from '../src/lib/services/inbox'
import fixture from './fixtures/inbox-terminal.json'

afterEach(() => vi.restoreAllMocks())
function client() {
  const value = new InboxClient(
    {} as never,
    {} as never,
    {} as never,
    'aaaaa-aa',
    fixture.entries[0].value.recipient
  )
  const get = vi.spyOn(value.session, 'get'),
    save = vi
      .spyOn(value as unknown as { save(key: string, value: unknown): Promise<void> }, 'save')
      .mockResolvedValue()
  return { value, get, save }
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
