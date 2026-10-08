import type { HttpAgent } from '@icp-sdk/core/agent'
import type { _SERVICE } from '../canisters/generated/payment'
import { controlResult } from './account'
import { certifiedValue } from './certified'
import { canonical, equal, unhex } from '../protocol/codec'
import { candidValue } from '../protocol/account'
import { paymentMethod } from '../protocol/payment'
import { IDL } from '@icp-sdk/core/candid'
import { ensure } from '../errors'

/** Funds recovery needs only the payment canister and the payer's identity. */
export class PaymentClient {
  constructor(
    readonly payment: _SERVICE,
    readonly agent: HttpAgent,
    readonly paymentId: string
  ) {}
  async escrow(id: string) {
    const raw = unhex(id)
    ensure(raw.length === 32, 'INVALID_INPUT')
    const value = controlResult(await this.payment.get_escrow(raw))
    const proof = await certifiedValue(
      controlResult(await this.payment.get_escrow_certified([raw])),
      this.agent,
      this.paymentId,
      raw
    )
    const type = (paymentMethod('get_escrow').retTypes[0] as IDL.VariantClass)._fields.find(
      ([name]) => name === 'Ok'
    )![1]
    ensure(equal(canonical(candidValue(type, value)), proof.value), 'INTEGRITY_FAILED')
    return value
  }
  async list(after?: string) {
    return controlResult(await this.payment.list_my_escrows(after ? [unhex(after)] : []))
  }
  async funding(id: string, block: bigint) {
    return controlResult(await this.payment.check_funding(unhex(id), block))
  }
  async refund(id: string) {
    controlResult(await this.payment.expiry_refund(unhex(id)))
    return this.escrow(id)
  }
  async deposits(id: string, after?: bigint) {
    return controlResult(
      await this.payment.list_deposits(unhex(id), after === undefined ? [] : [after])
    )
  }
  async transfers(id: string, after?: bigint) {
    return controlResult(
      await this.payment.list_transfers(unhex(id), after === undefined ? [] : [after])
    )
  }
  async refundQuote(id: string, blocks: bigint[], reserve: boolean) {
    return controlResult(await this.payment.quote_refund(unhex(id), blocks, reserve))
  }
  async claimRefund(id: string, blocks: bigint[], reserve: boolean) {
    return controlResult(await this.payment.claim_refund(unhex(id), blocks, reserve))
  }
  async processTransfer(id: string, leg: bigint) {
    return controlResult(await this.payment.process_transfer(unhex(id), leg))
  }
  /**
   * Send the recipient and platform payouts of a settled escrow, together.
   * Each leg must leave within the ledger's 24-hour deduplication window;
   * anyone may retry one later, so a failure here never fails the caller.
   */
  async payouts(id: string) {
    try {
      const legs = (await this.transfers(id)).filter(
        (leg) =>
          ('Recipient' in leg.kind || 'Platform' in leg.kind) &&
          !('Succeeded' in leg.status || 'Superseded' in leg.status)
      )
      await Promise.allSettled(
        legs.map((leg) => this.payment.process_transfer(unhex(id), leg.leg_id))
      )
    } catch {
      // Recovery and the payout dispatcher retry from the durable legs.
    }
  }
  async reconcileTransfer(id: string, leg: bigint, block: bigint) {
    return controlResult(await this.payment.reconcile_transfer(unhex(id), leg, block))
  }
  async reviseTransfer(id: string, leg: bigint, fee: bigint) {
    return controlResult(await this.payment.revise_rejected_transfer(unhex(id), leg, fee))
  }
}
