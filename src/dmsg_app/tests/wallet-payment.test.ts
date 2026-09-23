import { beforeEach, expect, it, vi } from 'vitest'
import { Principal } from '@icp-sdk/core/principal'
import type { HttpAgent } from '@icp-sdk/core/agent'
import type { CryptoClient } from '../src/lib/crypto/client'
import type { EscrowInfo } from '../src/lib/canisters/generated/payment'
import { WalletClient } from '../src/lib/services/wallet'

const ledger = vi.hoisted(() => ({ icrc1_fee: vi.fn(), icrc1_transfer: vi.fn() }))
vi.mock('@icp-sdk/core/agent', () => ({ Actor: { createActor: () => ledger } }))

beforeEach(() => vi.resetAllMocks())

function fixture() {
  const owner = Principal.selfAuthenticating(new Uint8Array([1]))
  const escrow = {
    escrow_id: new Uint8Array(32).fill(1),
    subaccount: new Uint8Array(32).fill(2),
    decision: { Pending: null },
    funded_at: [],
    quote: {
      payer: { owner, subaccount: [] },
      home_payment: Principal.selfAuthenticating(new Uint8Array([2])),
      ledger: Principal.selfAuthenticating(new Uint8Array([3])),
      amount: 1160n,
      max_network_fee: 20n,
      fund_by: BigInt(Date.now() + 600_000)
    }
  } as unknown as EscrowInfo
  let saved: string | null = null
  const crypto = {
    call: async (_method: string, _key: string, value?: string) => {
      if (value !== undefined) saved = value
      return saved
    }
  } as unknown as CryptoClient
  return { escrow, wallet: new WalletClient({} as HttpAgent, owner, crypto), job: () => JSON.parse(saved!) }
}

it('updates only a never-sent prepared payment after network fee maintenance', async () => {
  const { escrow, wallet, job } = fixture()
  ledger.icrc1_fee.mockResolvedValue(20n)
  await expect(wallet.transferEscrow(escrow, 10n)).rejects.toThrow('FEE_BLOCKED')
  expect(job().state).toBe('prepared')
  expect(ledger.icrc1_transfer).not.toHaveBeenCalled()
  ledger.icrc1_transfer.mockResolvedValue({ Ok: 7n })
  await expect(wallet.transferEscrow(escrow, 20n)).resolves.toBe(7n)
  expect(ledger.icrc1_transfer.mock.calls[0][0].fee).toEqual([20n])
  expect(ledger.icrc1_transfer.mock.calls[0][0].amount).toBe(1160n)
})

it('keeps an unknown payment frozen when configuration changes', async () => {
  const { escrow, wallet, job } = fixture()
  ledger.icrc1_fee.mockResolvedValue(10n)
  ledger.icrc1_transfer.mockRejectedValueOnce(new Error('lost reply'))
  await expect(wallet.transferEscrow(escrow, 10n)).rejects.toThrow('lost reply')
  const original = job().args
  expect(job().state).toBe('unknown')
  ledger.icrc1_fee.mockResolvedValue(20n)
  ledger.icrc1_transfer.mockResolvedValueOnce({ Err: { BadFee: { expected_fee: 20n } } })
  await expect(wallet.transferEscrow(escrow, 20n)).rejects.toThrow('BadFee')
  expect(job().args).toBe(original)
  expect(ledger.icrc1_transfer.mock.calls[1][0].fee).toEqual([10n])
  await expect(wallet.transferEscrow(escrow, 21n)).rejects.toThrow('FEE_BLOCKED')
  expect(ledger.icrc1_transfer).toHaveBeenCalledTimes(2)
})
