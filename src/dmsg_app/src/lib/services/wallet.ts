import { Actor, type HttpAgent } from '@icp-sdk/core/agent'
import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import { b64, unb64, digest, hex } from '../protocol/codec'
import type { CryptoClient } from '../crypto/client'
import type { EscrowInfo } from '../canisters/generated/payment'
import type { CheckoutView } from '@dmsg/sdk'
import { ensure } from '../errors'
const blob = IDL.Vec(IDL.Nat8),
  account = IDL.Record({ owner: IDL.Principal, subaccount: IDL.Opt(blob) })
const transfer = IDL.Record({
  to: account,
  amount: IDL.Nat,
  fee: IDL.Opt(IDL.Nat),
  memo: IDL.Opt(blob),
  from_subaccount: IDL.Opt(blob),
  created_at_time: IDL.Opt(IDL.Nat64)
})
const error = IDL.Variant({
  BadFee: IDL.Record({ expected_fee: IDL.Nat }),
  BadBurn: IDL.Record({ min_burn_amount: IDL.Nat }),
  InsufficientFunds: IDL.Record({ balance: IDL.Nat }),
  TooOld: IDL.Null,
  CreatedInFuture: IDL.Record({ ledger_time: IDL.Nat64 }),
  TemporarilyUnavailable: IDL.Null,
  Duplicate: IDL.Record({ duplicate_of: IDL.Nat }),
  GenericError: IDL.Record({ error_code: IDL.Nat, message: IDL.Text })
})
const factory = () =>
  IDL.Service({
    icrc1_balance_of: IDL.Func([account], [IDL.Nat], ['query']),
    icrc1_fee: IDL.Func([], [IDL.Nat], ['query']),
    icrc1_transfer: IDL.Func([transfer], [IDL.Variant({ Ok: IDL.Nat, Err: error })], [])
  })
interface Ledger {
  icrc1_balance_of(value: unknown): Promise<bigint>
  icrc1_fee(): Promise<bigint>
  icrc1_transfer(value: unknown): Promise<{ Ok: bigint } | { Err: Record<string, any> }>
}
/** The UI obtains a short, explicitly ledger-targeted wallet delegation.
 * This class never exports its signing key or invents a new transfer on retry. */
export class WalletClient {
  constructor(
    readonly agent: HttpAgent,
    readonly owner: Principal,
    readonly crypto: CryptoClient
  ) {}
  private actor(ledger: string) {
    return Actor.createActor<Ledger>(factory, { agent: this.agent, canisterId: ledger })
  }
  balance(ledger: string) {
    return this.actor(ledger).icrc1_balance_of({ owner: this.owner, subaccount: [] })
  }
  async transferEscrow(escrow: EscrowInfo, ledgerFee: bigint) {
    const quote = escrow.quote,
      identifier = hex(Uint8Array.from(escrow.escrow_id)),
      key = `delivery-funding:${identifier}`
    ensure(quote.payer.owner.toText() === this.owner.toText(), 'FORBIDDEN')
    const saved = await this.crypto.call('commerceJournal', key)
    let job = saved ? JSON.parse(saved) : null
    if (job?.block) return BigInt(job.block)
    if (!job) {
      ensure(
        quote.fund_by > BigInt(Date.now()) &&
          'Pending' in escrow.decision &&
          !escrow.funded_at.length,
        'EXPIRED'
      )
      const args = {
        to: { owner: quote.home_payment, subaccount: [escrow.subaccount] },
        amount: quote.amount,
        fee: [ledgerFee],
        from_subaccount: quote.payer.subaccount,
        memo: [digest('dmsg/delivery/funding/v1', Uint8Array.from(escrow.escrow_id))],
        created_at_time: [BigInt(Date.now()) * 1000000n]
      }
      job = {
        ledger: quote.ledger.toText(),
        payer: this.owner.toText(),
        args: b64(new Uint8Array(IDL.encode([transfer], [args]))),
        state: 'prepared'
      }
      await this.crypto.call('commerceJournal', key, JSON.stringify(job))
    }
    ensure(
      job.ledger === quote.ledger.toText() &&
        job.payer === this.owner.toText() &&
        ledgerFee <= quote.max_network_fee &&
        (await this.actor(job.ledger).icrc1_fee()) === ledgerFee,
      'FEE_BLOCKED'
    )
    // A prepared job has never reached the ledger. A fee check may have stopped
    // it before the controller updated payment's configuration. Only this state
    // can adopt the newly approved fee; unknown/rejected attempts stay frozen.
    if (job.state === 'prepared') {
      const args = IDL.decode([transfer], unb64(job.args))[0] as { fee: bigint[] }
      if (args.fee[0] !== ledgerFee) {
        args.fee = [ledgerFee]
        job.args = b64(new Uint8Array(IDL.encode([transfer], [args])))
      }
    }
    job.state = 'unknown'
    await this.crypto.call('commerceJournal', key, JSON.stringify(job))
    const reply = await this.actor(job.ledger).icrc1_transfer(
      IDL.decode([transfer], unb64(job.args))[0]
    )
    if ('Ok' in reply) job.block = reply.Ok.toString()
    else if ('Duplicate' in reply.Err) job.block = reply.Err.Duplicate.duplicate_of.toString()
    else {
      job.state = 'rejected'
      job.error = Object.keys(reply.Err)[0]
      await this.crypto.call('commerceJournal', key, JSON.stringify(job))
      throw new Error(`账本拒绝原来信付款：${job.error}`)
    }
    job.state = 'confirmed'
    await this.crypto.call('commerceJournal', key, JSON.stringify(job))
    return BigInt(job.block)
  }
  async transferCheckout(order: CheckoutView) {
    const quote = order.quote,
      cash = quote.cash,
      key = `checkout-funding:${hex(order.progress.order_id)}`
    ensure(
      Principal.fromUint8Array(cash.payer.owner).toText() === this.owner.toText(),
      'FORBIDDEN'
    )
    const saved = await this.crypto.call('commerceJournal', key)
    let job = saved ? JSON.parse(saved) : null
    if (job?.block) return BigInt(job.block)
    if (!job) {
      ensure(
        order.progress.status === 'AwaitingFunding' &&
          cash.funding_deadline_ms > BigInt(Date.now()),
        'EXPIRED'
      )
      const args = {
        to: {
          owner: Principal.fromUint8Array(cash.deposit.owner),
          subaccount: cash.deposit.subaccount ? [cash.deposit.subaccount] : []
        },
        amount: cash.amount_atomic + cash.fee_reserve_atomic,
        fee: [quote.asset.network_fee_atomic],
        from_subaccount: cash.payer.subaccount ? [cash.payer.subaccount] : [],
        memo: [digest('dmsg/checkout/funding/v2', order.progress.order_id)],
        created_at_time: [BigInt(Date.now()) * 1_000_000n]
      }
      job = {
        ledger: Principal.fromUint8Array(cash.ledger).toText(),
        payer: this.owner.toText(),
        args: b64(new Uint8Array(IDL.encode([transfer], [args]))),
        state: 'prepared'
      }
      await this.crypto.call('commerceJournal', key, JSON.stringify(job))
    }
    ensure(
      job.ledger === Principal.fromUint8Array(cash.ledger).toText() &&
        job.payer === this.owner.toText(),
      'FORBIDDEN'
    )
    if (job.state === 'prepared')
      ensure(
        (await this.actor(job.ledger).icrc1_fee()) === quote.asset.network_fee_atomic,
        'FEE_BLOCKED'
      )
    // Unknown attempts preserve original memo/time/fee, even when the live fee changed.
    const uncertain = job.state === 'unknown'
    job.state = 'unknown'
    await this.crypto.call('commerceJournal', key, JSON.stringify(job))
    const reply = await this.actor(job.ledger).icrc1_transfer(
      IDL.decode([transfer], unb64(job.args))[0]
    )
    if ('Ok' in reply) job.block = reply.Ok.toString()
    else if ('Duplicate' in reply.Err) job.block = reply.Err.Duplicate.duplicate_of.toString()
    else {
      job.state = uncertain ? 'unknown' : 'rejected'
      job.error = Object.keys(reply.Err)[0]
      await this.crypto.call('commerceJournal', key, JSON.stringify(job))
      throw new Error(`账本返回 ${job.error}；保留原转账，请按原区块对账。`)
    }
    job.state = 'confirmed'
    await this.crypto.call('commerceJournal', key, JSON.stringify(job))
    return BigInt(job.block)
  }
}
