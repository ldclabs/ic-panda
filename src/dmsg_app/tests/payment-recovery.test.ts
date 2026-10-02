import { expect, it, vi } from 'vitest'
import { PaymentClient } from '../src/lib/services/payment'

it('recovers payer-indexed escrows and reconciles outgoing blocks without a relay or account client', async () => {
  const api = {
    list_my_escrows: vi.fn(async () => ({ Ok: [] })),
    reconcile_transfer: vi.fn(async () => ({ Ok: { leg_id: 3n } }))
  }
  const client = new PaymentClient(api as any, {} as any, 'aaaaa-aa')
  expect(await client.list()).toEqual([])
  await client.list('01'.repeat(32))
  expect(api.list_my_escrows.mock.calls[1]).toEqual([[new Uint8Array(32).fill(1)]])
  expect(await client.reconcileTransfer('02'.repeat(32), 3n, 4n)).toEqual({ leg_id: 3n })
  expect(api.reconcile_transfer).toHaveBeenCalledExactlyOnceWith(
    new Uint8Array(32).fill(2),
    3n,
    4n
  )
})
