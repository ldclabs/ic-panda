import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE as Commerce,
  Account,
  Catalog,
  BillingOrder,
  OrderAction,
  OrderQuote,
  MembershipIntent,
  ClaimRequest
} from '../canisters/generated/commerce'
import type { _SERVICE as Membership } from '../canisters/generated/membership'
import { AccountClient, controlResult } from './account'
import { certifiedValue } from './certified'
import {
  apiMethod,
  beneficiary,
  beneficiaryValue,
  claimDigest,
  claimId,
  commerceInput,
  decodeCommerce,
  encodeCommerce,
  orderId,
  quoteDigest,
  quoteValue
} from '../protocol/commerce'
import { candidValue } from '../protocol/account'
import { canonical, decodeCanonical, digest, equal, hex, id, unhex } from '../protocol/codec'
import { ensure } from '../errors'
import { xidBytes } from '../protocol/identity'
interface OrderJob {
  format: 'dmsg-order-job/1'
  id: string
  account: string
  payer: string
  input: string
  stage: 'review' | 'authorized' | 'unknown' | 'opened'
  blocks: string[]
}
interface ClaimJob {
  format: 'dmsg-membership-job/1'
  id: string
  account: string
  actor: string
  input: string
  stage: 'review' | 'authorized' | 'unknown' | 'submitted'
}
export class CommerceClient {
  constructor(
    readonly account: AccountClient,
    readonly commerce: Commerce,
    readonly membership: Membership,
    readonly commerceId: string,
    readonly membershipId: string,
    readonly wallet: Principal
  ) {}
  private accountId() {
    const value = this.account.meta.account?.id
    ensure(value, 'AUTH_REQUIRED')
    return value
  }
  private subject() {
    return beneficiary(this.account.home.toText(), this.accountId())
  }
  private async read<T>(key: string): Promise<T | null> {
    const value = await this.account.crypto.call('commerceJournal', key)
    return value ? JSON.parse(value) : null
  }
  private save(key: string, value: unknown) {
    return this.account.crypto.call('commerceJournal', key, JSON.stringify(value))
  }
  async orders() {
    return (await this.account.crypto.call('commerceJournals'))
      .filter((v) => v.key.startsWith('order:'))
      .map((v) => JSON.parse(v.value) as OrderJob)
  }
  async claims() {
    return (await this.account.crypto.call('commerceJournals'))
      .filter((v) => v.key.startsWith('sns:'))
      .map((v) => JSON.parse(v.value) as ClaimJob)
  }
  async catalog() {
    const proof = await certifiedValue(
      controlResult(await this.commerce.get_catalog()),
      this.account.agent,
      this.commerceId,
      digest('dmsg/commerce/catalog-key/v1', 'dmsg')
    )
    const catalog = decodeCanonical<Record<string, any>>(proof.value)
    ensure(
      catalog.schema === 1 &&
        catalog.ledger instanceof Uint8Array &&
        catalog.terms_digest instanceof Uint8Array &&
        catalog.terms_digest.length === 32 &&
        Array.isArray(catalog.plans) &&
        catalog.plans.length === 4 &&
        new Set(catalog.plans.map((p) => p.plan_id)).size === 4 &&
        Number(catalog.decimals) <= 18,
      'INTEGRITY_FAILED'
    )
    const known = await this.read<{ version: string }>('catalog-high-water')
    ensure(!known || BigInt(catalog.version) >= BigInt(known.version), 'POLICY_STALE')
    await this.save('catalog-high-water', { version: String(catalog.version) })
    return {
      value: catalog,
      encoded: proof.value,
      ledger: Principal.fromUint8Array(catalog.ledger).toText()
    }
  }
  async entitlement(refresh = false) {
    if (refresh) {
      const refreshed = await this.commerce.refresh_entitlement(this.subject())
      if ('Err' in refreshed && 'NotFound' in refreshed.Err)
        controlResult(
          await this.account.user.refresh_execution_entitlement(xidBytes(this.accountId()))
        )
      else controlResult(refreshed)
    }
    const proof = await certifiedValue(
      controlResult(await this.commerce.get_entitlement_batch([this.subject()])),
      this.account.agent,
      this.commerceId,
      digest('dmsg/commerce/entitlement-key/v1', beneficiaryValue(this.subject()))
    )
    const value = decodeCanonical<Record<string, any>>(proof.value)
    ensure(
      value.schema === 1 &&
        equal(value.home_commerce, Principal.fromText(this.commerceId).toUint8Array()) &&
        equal(canonical(value.beneficiary), canonical(beneficiaryValue(this.subject()))) &&
        BigInt(value.valid_until_ms) > BigInt(Date.now()),
      'POLICY_STALE'
    )
    const known = await this.read<{ business: string; lease: string }>('commerce-high-water')
    ensure(
      !known ||
        (BigInt(value.business_revision) >= BigInt(known.business) &&
          BigInt(value.lease_revision) >= BigInt(known.lease)),
      'POLICY_STALE'
    )
    await this.save('commerce-high-water', {
      business: String(value.business_revision),
      lease: String(value.lease_revision)
    })
    return value
  }
  private intent(
    service: string,
    operation: Uint8Array | number[],
    action: Uint8Array
  ): MembershipIntent {
    const environment =
      this.account.meta.environment === 'production'
        ? { Production: null }
        : this.account.meta.environment === 'staging'
          ? { Staging: null }
          : { Local: null }
    return {
      actor: this.wallet,
      beneficiary: this.subject(),
      application_id: operation,
      environment,
      service_canister: Principal.fromText(service),
      action_digest: action,
      nonce: unhex(id()),
      valid_until_ms: BigInt(Date.now() + 86400000 - 1000)
    }
  }
  async quote(action: OrderAction, payer: Account = { owner: this.wallet, subaccount: [] }) {
    ensure(payer.owner.toText() === this.wallet.toText(), 'AUTH_REQUIRED')
    const entitlement = await this.entitlement(true),
      catalog = await this.catalog()
    const quote = controlResult(
      await this.commerce.quote_order({
        op_id: unhex(id()),
        beneficiary: this.subject(),
        action,
        expected_business_revision: BigInt(entitlement.business_revision),
        payer
      })
    )
    const encodedCatalog = canonical((quoteValue(quote) as any).catalog)
    ensure(
      equal(encodedCatalog, catalog.encoded) &&
        quote.home_commerce.toText() === this.commerceId &&
        quote.created_at_ms <= BigInt(Date.now()) &&
        quote.fund_by_ms > BigInt(Date.now()) &&
        quote.amount_atomic > 0n &&
        quote.fee_reserve >= 0n,
      'INTEGRITY_FAILED'
    )
    const input = {
      quote,
      authorization: this.intent(this.commerceId, quote.request.op_id, quoteDigest(quote))
    }
    const job: OrderJob = {
      format: 'dmsg-order-job/1',
      id: hex(orderId(quote)),
      account: this.accountId(),
      payer: this.wallet.toText(),
      input: encodeCommerce('commerce', 'open_order', [input]),
      stage: 'review',
      blocks: []
    }
    await this.save(`order:${job.id}`, job)
    return { job, quote }
  }
  async order(id: string) {
    const value = controlResult(await this.commerce.get_operation(unhex(id)))
    const proof = await certifiedValue(
      controlResult(await this.commerce.get_order_certified(unhex(id))),
      this.account.agent,
      this.commerceId,
      digest('dmsg/commerce/order-key/v1', unhex(id))
    )
    const result = apiMethod('commerce', 'open_order').retTypes[0] as IDL.VariantClass
    const type = result._fields.find(([name]) => name === 'Ok')![1]
    ensure(
      equal(canonical(candidValue(type, value)), proof.value) &&
        hex(Uint8Array.from(value.order_id)) === id,
      'INTEGRITY_FAILED'
    )
    ensure(
      value.confirmed_in ===
        value.service_reserve +
          value.earned +
          value.refundable +
          value.fee_reserve +
          value.outgoing +
          value.transferred +
          value.network_fees,
      'INTEGRITY_FAILED',
      '订单资产记录不守恒。'
    )
    return value
  }
  async open(id: string) {
    const job = await this.read<OrderJob>(`order:${id}`)
    ensure(
      job && job.account === this.accountId() && job.payer === this.wallet.toText(),
      'AUTH_REQUIRED'
    )
    const input = decodeCommerce('commerce', 'open_order', job.input)[0] as unknown as {
      quote: OrderQuote
      authorization: MembershipIntent
    }
    const existing = await this.commerce.get_operation(unhex(id))
    if ('Ok' in existing && !('Authorizing' in existing.Ok.status)) {
      job.stage = 'opened'
      await this.save(`order:${id}`, job)
      return this.order(id)
    }
    if ('Err' in existing) ensure('NotFound' in existing.Err, 'EXECUTION_UNKNOWN')
    if (await this.account.pending()) await this.account.resume()
    if (input.authorization.valid_until_ms > BigInt(Date.now())) {
      await this.account.mutate(job.account, {
        AuthorizeMembership: { intent: input.authorization }
      })
      job.stage = 'authorized'
      await this.save(`order:${id}`, job)
    }
    ensure(
      input.authorization.valid_until_ms > BigInt(Date.now()),
      'EXPIRED',
      '原商业意图已过期；先确认原订单结果，再重新报价。'
    )
    job.stage = 'unknown'
    await this.save(`order:${id}`, job)
    controlResult(await this.commerce.open_order(input))
    const order = await this.order(id)
    ensure(
      equal(
        canonical(commerceInput('open_order', order.input)),
        canonical(commerceInput('open_order', input))
      ),
      'INTEGRITY_FAILED'
    )
    job.stage = 'opened'
    await this.save(`order:${id}`, job)
    return order
  }
  async funding(id: string, block: string) {
    ensure(
      /^(0|[1-9][0-9]*)$/.test(block) && BigInt(block) <= 0xffffffffffffffffn,
      'INVALID_INPUT'
    )
    const job = await this.read<OrderJob>(`order:${id}`)
    ensure(job, 'NOT_FOUND')
    if (!job.blocks.includes(block)) {
      job.blocks.push(block)
      await this.save(`order:${id}`, job)
    }
    controlResult(await this.commerce.check_order_funding(unhex(id), BigInt(block)))
    return this.order(id)
  }
  async reconcile(id: string) {
    controlResult(await this.commerce.reconcile_order(unhex(id)))
    return this.order(id)
  }
  async refund(id: string) {
    const order = await this.order(id)
    ensure(
      order.input.quote.request.payer.owner.toText() === this.wallet.toText(),
      'AUTH_REQUIRED'
    )
    let stored = await this.read<{ args: string }>(`refund:${id}`)
    if (!stored) {
      const intent = this.intent(
        this.commerceId,
        unhex(globalId()),
        digest('dmsg/commerce/refund/v1', unhex(id))
      )
      stored = { args: encodeCommerce('commerce', 'request_refund', [unhex(id), intent]) }
      await this.save(`refund:${id}`, stored)
    }
    const args = decodeCommerce('commerce', 'request_refund', stored.args) as unknown as [
      Uint8Array,
      MembershipIntent
    ]
    if (await this.account.pending()) await this.account.resume()
    await this.account.mutate(this.accountId(), { AuthorizeMembership: { intent: args[1] } })
    controlResult(await this.commerce.request_refund(...args))
    return this.order(id)
  }
  async refundDeposit(id: string, block: string) {
    return controlResult(await this.commerce.claim_deposit_refund(unhex(id), BigInt(block)))
  }
  async refundFees(id: string) {
    return controlResult(await this.commerce.claim_fee_reserve(unhex(id)))
  }
  async processTransfer(id: string, transfer: string, block?: string) {
    const record = controlResult(await this.commerce.get_transfer(unhex(id), BigInt(transfer)))
    if ('Unknown' in record.status || 'InFlight' in record.status) {
      ensure(
        block !== undefined,
        'EXECUTION_UNKNOWN',
        '转账结果未知；提供原账本区块对账，不创建新转账。'
      )
      return controlResult(
        await this.commerce.reconcile_transfer(unhex(id), BigInt(transfer), BigInt(block))
      )
    }
    return controlResult(await this.commerce.process_transfer(unhex(id), BigInt(transfer)))
  }
  async sns(
    policy: bigint,
    neuron: string,
    benefit: Uint8Array,
    change: ClaimRequest['change'] = { Start: null }
  ) {
    ensure(/^[0-9a-f]{64}$/.test(neuron), 'INVALID_INPUT')
    const policyValue = await this.policy(policy, benefit)
    const ent = await this.entitlement(true),
      op = unhex(id())
    const request: ClaimRequest = {
      authorization: this.intent(this.membershipId, op, new Uint8Array(32)),
      neuron_id: unhex(neuron),
      policy_version: policy,
      benefit_id: benefit,
      expected_business_revision: BigInt(ent.business_revision),
      term: { CalendarYear: null },
      change
    }
    request.authorization.action_digest = claimDigest(request)
    const job: ClaimJob = {
      format: 'dmsg-membership-job/1',
      id: hex(claimId(this.membershipId, request.authorization)),
      account: this.accountId(),
      actor: this.wallet.toText(),
      input: encodeCommerce('membership', 'request_claim', [request]),
      stage: 'review'
    }
    await this.save(`sns:${job.id}`, job)
    return { job, policy: policyValue }
  }
  private async policy(version: bigint, benefit: Uint8Array) {
    const proof = await certifiedValue(
      controlResult(await this.membership.get_policy_certified([version])),
      this.account.agent,
      this.membershipId,
      digest('membership/policy-key/v1', version)
    )
    const value = decodeCanonical<any>(proof.value)
    ensure(
      value.product_id === 'dmsg' &&
        BigInt(value.version) === version &&
        equal(value.benefit_id, benefit),
      'INTEGRITY_FAILED'
    )
    return value
  }
  async reviewSns(id: string) {
    const job = await this.read<ClaimJob>(`sns:${id}`)
    ensure(
      job && job.actor === this.wallet.toText() && job.account === this.accountId(),
      'AUTH_REQUIRED'
    )
    const request = decodeCommerce(
      'membership',
      'request_claim',
      job.input
    )[0] as unknown as ClaimRequest
    ensure(
      hex(claimId(this.membershipId, request.authorization)) === id &&
        request.authorization.actor.toText() === job.actor &&
        equal(Uint8Array.from(request.authorization.action_digest), claimDigest(request)) &&
        equal(
          canonical(beneficiaryValue(request.authorization.beneficiary)),
          canonical(beneficiaryValue(this.subject()))
        ),
      'INTEGRITY_FAILED'
    )
    return {
      request,
      policy: await this.policy(request.policy_version, Uint8Array.from(request.benefit_id))
    }
  }
  async submitSns(id: string) {
    const job = await this.read<ClaimJob>(`sns:${id}`)
    ensure(
      job && job.actor === this.wallet.toText() && job.account === this.accountId(),
      'AUTH_REQUIRED'
    )
    const request = decodeCommerce(
      'membership',
      'request_claim',
      job.input
    )[0] as unknown as ClaimRequest
    const prior = await this.membership.get_operation(unhex(id))
    if ('Ok' in prior) return this.claimStatus(id)
    ensure('NotFound' in prior.Err, 'EXECUTION_UNKNOWN')
    if (await this.account.pending()) await this.account.resume()
    await this.account.mutate(this.accountId(), {
      AuthorizeMembership: { intent: request.authorization }
    })
    job.stage = 'unknown'
    await this.save(`sns:${id}`, job)
    controlResult(await this.membership.request_claim(request))
    job.stage = 'submitted'
    await this.save(`sns:${id}`, job)
    return this.claimStatus(id)
  }
  async claimStatus(id: string) {
    const proof = await certifiedValue(
      controlResult(await this.membership.get_claim_certified([unhex(id)])),
      this.account.agent,
      this.membershipId,
      digest('membership/claim-key/v1', unhex(id))
    )
    const value = decodeCanonical<any>(proof.value)
    ensure(
      equal(value.claim_id, unhex(id)) &&
        equal(canonical(value.beneficiary), canonical(beneficiaryValue(this.subject()))),
      'INTEGRITY_FAILED'
    )
    return value
  }
  async advanceSns(id: string) {
    const job = await this.read<ClaimJob>(`sns:${id}`)
    ensure(
      job && job.actor === this.wallet.toText() && job.account === this.accountId(),
      'AUTH_REQUIRED'
    )
    const request = decodeCommerce(
      'membership',
      'request_claim',
      job.input
    )[0] as unknown as ClaimRequest
    ensure(request.authorization.valid_until_ms > BigInt(Date.now()), 'EXPIRED')
    if (await this.account.pending()) await this.account.resume()
    await this.account.mutate(this.accountId(), {
      AuthorizeMembership: { intent: request.authorization }
    })
    controlResult(await this.membership.advance_application(unhex(id)))
    return this.claimStatus(id)
  }
  async refreshSns(id: string) {
    controlResult(await this.membership.refresh_claim(unhex(id)))
    return this.claimStatus(id)
  }
  async reconcileSns(id: string) {
    controlResult(await this.membership.reconcile_claim(unhex(id)))
    return this.claimStatus(id)
  }
  async closeSns(id: string) {
    let saved = await this.read<{ args: string }>(`sns-close:${id}`)
    if (!saved) {
      const intent = this.intent(
        this.membershipId,
        unhex(globalId()),
        digest('membership/close/v1', unhex(id))
      )
      saved = { args: encodeCommerce('membership', 'request_change', [unhex(id), intent]) }
      await this.save(`sns-close:${id}`, saved)
    }
    const intent = decodeCommerce(
      'membership',
      'request_change',
      saved.args
    )[1] as unknown as MembershipIntent
    await this.account.mutate(this.accountId(), { AuthorizeMembership: { intent } })
    return controlResult(await this.membership.request_change(unhex(id), intent))
  }
}
const globalId = id
