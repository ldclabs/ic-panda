import { expect, it, vi } from 'vitest'
import { InboxClient } from '../src/lib/services/inbox'
import { id } from '../src/lib/protocol/codec'
import { xidText } from '../src/lib/protocol/identity'

it('retains blocks, invitations and capacity while changing contact mode', async () => {
  const accountId = xidText(new Uint8Array(12).fill(1))
  const old = {
    mode: 'closed',
    version: 1,
    hash: id(),
    inbox_key_version: 1,
    allow: [],
    block: [accountId],
    invitations: [{ id: id(), account: accountId, expires_at: Date.now() + 86400000 }],
    capacity: 27
  }
  const crypto = {
    call: vi.fn(async (method: string) =>
      method === 'inboxKey'
        ? { publicKey: '01'.repeat(32), rootGeneration: 1 }
        : method === 'deviceSign'
          ? new Uint8Array(64).fill(1)
          : null
    )
  }
  const client = new InboxClient(
    { meta: { rootGeneration: 1 }, crypto } as any,
    {} as any,
    {} as any,
    'aaaaa-aa',
    accountId
  )
  client.session.state = {
    info: { vault_write_state: { Ready: null }, current_root: [{ generation: 1n }] },
    verified: { securityEpoch: 1 }
  } as any
  vi.spyOn(client.session, 'context').mockResolvedValue({
    accountId,
    issuer: `https://example.test/${accountId}`,
    deviceId: id(),
    securityEpoch: 1,
    requestId: id(),
    deadline: Date.now() + 45000
  })
  vi.spyOn(client, 'ownPolicy').mockResolvedValue(old)
  const post = vi
    .spyOn(client as any, 'post')
    .mockImplementation(async (...args: any[]) => args[3])
  const value = await client.configure('free', '', '0')
  expect(value.block).toEqual(old.block)
  expect(value.invitations).toEqual(old.invitations)
  expect(value.capacity).toBe(27)
  expect(post).toHaveBeenCalledOnce()
})
