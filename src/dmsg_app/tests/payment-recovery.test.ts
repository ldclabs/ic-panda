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

it('sends unsettled recipient and platform payouts and never fails on their errors', async () => {
  const leg = (leg_id: bigint, kind: object, status: object) => ({ leg_id, kind, status })
  const api = {
    list_transfers: vi.fn(async () => ({
      Ok: [
        leg(0n, { Refund: {} }, { Pending: null }),
        leg(1n, { Recipient: null }, { Pending: null }),
        leg(2n, { Platform: null }, { Unknown: null }),
        leg(3n, { Recipient: null }, { Superseded: null }),
        leg(4n, { Platform: null }, { Succeeded: null })
      ]
    })),
    process_transfer: vi.fn(async (_: Uint8Array, leg: bigint) => {
      if (leg === 1n) throw new Error('transport')
      return { Ok: {} }
    })
  }
  const client = new PaymentClient(api as any, {} as any, 'aaaaa-aa')
  await client.payouts('03'.repeat(32))
  expect(api.process_transfer.mock.calls.map(([, leg]) => leg)).toEqual([1n, 2n])
  api.list_transfers.mockRejectedValueOnce(new Error('offline'))
  await expect(client.payouts('03'.repeat(32))).resolves.toBeUndefined()
})
