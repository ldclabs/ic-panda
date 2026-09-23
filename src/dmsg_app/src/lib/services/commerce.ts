import { IDL } from '@icp-sdk/core/candid'
import { Principal } from '@icp-sdk/core/principal'
import {
  canonical,
  decodeCanonical,
  equalBytes,
  validateShape,
  validateCheckoutQuote,
  validateCheckoutRequest,
  validatePandaTerms,
  checkoutQuoteHash,
  checkoutId,
  pandaApplicationHash,
  pandaClaimId,
  type CheckoutRequest,
  type CheckoutQuote,
  type PandaApplicationTerms,
  type CheckoutView,
  type PandaClaimView,
  type ProductRegistration,
  type ApplicationApproval,
  type ProductAuthorizationRequest,
  type SettlementAssetView
} from '@dmsg/sdk'
import type { _SERVICE as Commerce } from '../canisters/generated/commerce'
import type { _SERVICE as Membership } from '../canisters/generated/membership'
import { idlFactory as userIDL } from '../canisters/generated/user/index.js'
import { AccountClient, controlResult } from './account'
import { certifiedValue } from './certified'
import { registeredApplication } from './registration'
import {
  apiMethod,
  beneficiary,
  beneficiaryValue,
  toCandid,
  wireResult
} from '../protocol/commerce'
import { decodeControl, encodeControl } from '../protocol/account'
import { b64, unb64, digest, hex, id, unhex } from '../protocol/codec'
import { xidBytes } from '../protocol/identity'
import { ensure, DmsgError } from '../errors'
export interface CheckoutJob {
  format: 'dmsg-checkout/2'
  id: string
  account: string
  actor: string
  origin: string
  request: string
  quote: string
  operation: string | null
  authorization: string | null
  stage: 'review' | 'authorized' | 'unknown' | 'accepted'
}
const wire = <T>(plane: 'commerce' | 'membership', method: string, result: unknown) =>
  decodeCanonical(canonical(wireResult(plane, method, result))) as unknown as T
const userService = userIDL({ IDL }) as IDL.ServiceClass
const approveTypes = userService._fields.find(([name]) => name === 'approve_application')![1]
  .argTypes
const pack = (v: unknown) => b64(canonical(v))
const unpack = <T>(v: string): T => decodeCanonical(unb64(v)) as unknown as T
/** One journal owns original terms, device proof and service operation through every retry. */
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
  private async read<T>(key: string): Promise<T | null> {
    const v = await this.account.crypto.call('commerceJournal', key)
    return v ? JSON.parse(v) : null
  }
  private save(job: CheckoutJob) {
    return this.account.crypto.call(
      'commerceJournal',
      `checkout:${job.id}`,
      JSON.stringify(job)
    )
  }
  async jobs() {
    return (await this.account.crypto.call('commerceJournals'))
      .filter((v) => v.key.startsWith('checkout:'))
      .map((v) => JSON.parse(v.value) as CheckoutJob)
      .filter((v) => v.account === this.accountId() && v.actor === this.wallet.toText())
  }
  async job(id: string) {
    const j = await this.read<CheckoutJob>(`checkout:${id}`)
    ensure(
      j && j.account === this.accountId() && j.actor === this.wallet.toText(),
      'FORBIDDEN'
    )
    return j
  }
  request(job: CheckoutJob) {
    return unpack<CheckoutRequest>(job.request)
  }
  terms(job: CheckoutJob) {
    return unpack<CheckoutQuote | PandaApplicationTerms>(job.quote)
  }
  private input(plane: 'commerce' | 'membership', method: string, values: unknown[]) {
    return apiMethod(plane, method).argTypes.map((t, i) => toCandid(t, values[i]))
  }
  private async call<T>(
    plane: 'commerce' | 'membership',
    method: string,
    values: unknown[] = []
  ): Promise<T> {
    const api = (plane === 'commerce' ? this.commerce : this.membership) as unknown as Record<
      string,
      (...v: any[]) => Promise<any>
    >
    return wire<T>(
      plane,
      method,
      controlResult(await api[method](...this.input(plane, method, values)))
    )
  }
  private async certified<T>(
    plane: 'commerce' | 'membership',
    method: string,
    values: unknown[],
    key: Uint8Array
  ): Promise<T> {
    const api = (plane === 'commerce' ? this.commerce : this.membership) as unknown as Record<
      string,
      (...v: any[]) => Promise<any>
    >
    const p = await certifiedValue(
      controlResult(await api[method](...this.input(plane, method, values))),
      this.account.agent,
      plane === 'commerce' ? this.commerceId : this.membershipId,
      key
    )
    return decodeCanonical(p.value) as unknown as T
  }
  async assets() {
    return this.certified<SettlementAssetView[]>(
      'commerce',
      'settlement_assets_certificate',
      [],
      digest('dmsg/settlement-assets/v2', 'supported')
    )
  }
  async catalog() {
    return this.certified<any>(
      'commerce',
      'get_catalog',
      [],
      digest('dmsg/commerce/catalog-key/v1', 'dmsg')
    )
  }
  async entitlement(refresh = false) {
    const b = beneficiary(this.account.home.toText(), this.accountId())
    if (refresh) {
      const value = await this.commerce.refresh_entitlement(b)
      if ('Err' in value && 'NotFound' in value.Err)
        controlResult(
          await this.account.user.refresh_execution_entitlement(xidBytes(this.accountId()))
        )
      else controlResult(value)
    }
    return this.certified<any>(
      'commerce',
      'get_entitlement_batch',
      [[beneficiaryValue(b)]],
      digest('dmsg/commerce/entitlement-key/v1', beneficiaryValue(b))
    )
  }
  async personal(sku: string, method: 'Cash' | 'Panda') {
    const offer = await this.call<CheckoutRequest['offer']>(
      'commerce',
      'prepare_account_subscription',
      [
        'dmsg',
        beneficiaryValue(beneficiary(this.account.home.toText(), this.accountId())),
        sku,
        unhex(id())
      ]
    )
    return {
      offer,
      method,
      approving_account: xidBytes(this.accountId()),
      product_approval: null
    } satisfies CheckoutRequest
  }
  private async application(appId: string, origin: string) {
    const { info, device } = await this.account.refresh(this.accountId())
    ensure(
      device?.input.capabilities.some((c) => 'FormalApprove' in c),
      'DEVICE_NOT_APPROVED',
      '请先在设备设置中启用正式批准权限。'
    )
    return registeredApplication(
      { commerce: this.commerce, agent: this.account.agent },
      appId,
      origin,
      'Checkout',
      {
        commerce: this.commerceId,
        user: this.account.home.toText(),
        cose: info.home_cose.toText(),
        environment: this.account.meta.environment
      }
    )
  }
  private async registration(request: CheckoutRequest, origin: string) {
    ensure(
      equalBytes(request.approving_account, xidBytes(this.accountId())),
      'ACCOUNT_MISMATCH'
    )
    const app = await this.application(request.offer.app_id, origin)
    const product = await this.certified<ProductRegistration>(
      'commerce',
      'integration_configuration_certificate',
      [app.app_id, request.offer.product_id],
      digest('dmsg/registration/product/v2', request.offer.product_id)
    )
    validateCheckoutRequest(request, app, product, BigInt(Date.now()))
    return { app, product }
  }
  async quote(request: CheckoutRequest, origin: string, selection: string) {
    const { app, product } = await this.registration(request, origin)
    const terms =
      request.method === 'Cash'
        ? await this.call<CheckoutQuote>('commerce', 'quote_checkout', [
            request.offer,
            Principal.fromText(selection).toUint8Array(),
            { owner: this.wallet.toUint8Array(), subaccount: null }
          ])
        : await this.call<PandaApplicationTerms>('membership', 'quote_panda_subscription', [
            request.offer,
            this.account.home.toUint8Array(),
            request.approving_account,
            unhex(selection)
          ])
    if ('cash' in terms) {
      await validateCheckoutQuote(
        terms,
        request,
        app,
        product,
        Principal.fromText(this.commerceId).toUint8Array(),
        this.wallet.toUint8Array(),
        BigInt(Date.now())
      )
      ensure(
        (await this.assets()).some(
          (v) => v.ledger_verified && equalBytes(canonical(v.policy), canonical(terms.asset))
        ),
        'POLICY_STALE'
      )
    } else
      await validatePandaTerms(
        terms,
        request,
        product,
        Principal.fromText(this.membershipId).toUint8Array(),
        this.account.home.toUint8Array(),
        this.wallet.toUint8Array(),
        unhex(selection),
        BigInt(Date.now())
      )
    const operation =
      'cash' in terms
        ? await checkoutId(Principal.fromText(this.commerceId).toUint8Array(), request)
        : await pandaClaimId(terms)
    const job: CheckoutJob = {
      format: 'dmsg-checkout/2',
      id: hex(operation),
      account: this.accountId(),
      actor: this.wallet.toText(),
      origin,
      request: pack(request),
      quote: pack(terms),
      operation: null,
      authorization: null,
      stage: 'review'
    }
    const old = await this.read<CheckoutJob>(`checkout:${job.id}`)
    if (old) {
      ensure(
        old.request === job.request && old.actor === job.actor && old.origin === origin,
        'IDEMPOTENCY_CONFLICT'
      )
      return old
    }
    await this.save(job)
    return job
  }
  async status(id: string): Promise<CheckoutView | PandaClaimView> {
    const job = await this.job(id)
    const cash = this.request(job).method === 'Cash'
    const value = await this.certified<CheckoutView | PandaClaimView>(
      cash ? 'commerce' : 'membership',
      cash ? 'checkout_certificate' : 'panda_claim_certificate',
      [unhex(id)],
      digest(
        cash ? 'dmsg/checkout/certificate/v2' : 'dmsg/panda/claim-certificate/v2',
        unhex(id)
      )
    )
    ensure(
      equalBytes(
        canonical('progress' in value ? value.quote : value.terms),
        canonical(this.terms(job))
      ),
      'INTEGRITY_FAILED'
    )
    return value
  }
  async approve(id: string, fresh = false) {
    let job = await this.job(id)
    const request = this.request(job),
      terms = this.terms(job),
      cash = 'cash' in terms
    if (job.stage !== 'review') {
      try {
        const existing = await this.status(id)
        if (!fresh) return existing
        ensure(!('progress' in existing), 'FORBIDDEN')
        if (existing.status === 'Applying') return this.reconcile(id)
        if (existing.status !== 'CoolingDown') return existing
      } catch (error) {
        if (
          fresh ||
          !(error instanceof DmsgError) ||
          !['NotFound', 'NOT_FOUND'].includes(error.code)
        )
          throw error
      }
    }
    if (fresh) {
      ensure(!cash, 'FORBIDDEN')
      if (job.operation)
        await this.account.crypto.call(
          'commerceJournal',
          `checkout-approval-history:${id}:${hex(digest('dmsg/approval-journal/v2', job.operation))}`,
          JSON.stringify(job)
        )
      job.operation = null
      job.authorization = null
    }
    if (!job.operation) {
      // Initial quote remains fixed. Post-cooling approval may outlive the offer admission window.
      const app = await this.application(request.offer.app_id, job.origin)
      const now = BigInt(Date.now()),
        deadline = cash ? terms.cash.funding_deadline_ms : terms.quote.application_deadline_ms
      ensure(now < deadline && (fresh || now < request.offer.accept_by_ms), 'EXPIRED')
      const state = await this.account.refresh(job.account)
      ensure(state.device, 'DEVICE_NOT_APPROVED')
      const application: ApplicationApproval = {
        version: 1n,
        environment: app.environment,
        app_id: app.app_id,
        app_config_version: app.config_version,
        origin: job.origin,
        approving_account: request.approving_account,
        service: Principal.fromText(cash ? this.commerceId : this.membershipId).toUint8Array(),
        beneficiary: request.offer.beneficiary,
        actor: this.wallet.toUint8Array(),
        purpose: cash ? 'CashCheckout' : 'PandaSubscription',
        action_digest: cash
          ? await checkoutQuoteHash(terms)
          : await pandaApplicationHash(terms),
        operation_id: request.offer.operation_id,
        nonce: unhex(globalId()),
        expires_at_ms: now + 240_000n < deadline ? now + 240_000n : deadline
      }
      validateShape('ApplicationApproval', application)
      const proof = {
        device_id: unhex(this.account.meta.deviceId),
        security_epoch: state.info.security_epoch,
        sequence: state.device.next_sequence,
        request_id: unhex(globalId()),
        expires_at: application.expires_at_ms,
        signature: new Uint8Array()
      }
      proof.signature = Uint8Array.from(
        await this.account.crypto.call(
          'deviceSign',
          digest('dmsg/device-approval/v2', [
            this.account.home.toUint8Array(),
            request.approving_account,
            'dmsg/application/approve/v1',
            proof.device_id,
            proof.security_epoch,
            proof.sequence,
            proof.request_id,
            proof.expires_at,
            digest('dmsg/application/approve/v1', application)
          ])
        )
      )
      job.operation = encodeControl('approve_application', [
        toCandid(approveTypes[0]!, application),
        proof
      ])
      job.authorization = pack({
        offer: request.offer,
        account_approval: application,
        user_home: this.account.home.toUint8Array(),
        approval_id: proof.request_id,
        product_approval: request.product_approval
      } satisfies ProductAuthorizationRequest)
      job.stage = 'authorized'
      await this.save(job)
    }
    const args = decodeControl('approve_application', job.operation) as Parameters<
      CommerceClient['account']['user']['approve_application']
    >
    // Retrying the saved proof cannot consume another sequence or change an economic instruction.
    controlResult(await this.account.user.approve_application(...args))
    job.stage = 'unknown'
    await this.save(job)
    const authorization = unpack<ProductAuthorizationRequest>(job.authorization!)
    if (fresh) await this.call('membership', 'advance_panda_claim', [unhex(id), authorization])
    else if (cash)
      await this.call('commerce', 'open_checkout', [{ quote: terms, authorization }])
    else await this.call('membership', 'request_panda_claim', [{ terms, authorization }])
    job.stage = 'accepted'
    await this.save(job)
    return this.status(id)
  }
  async reconcile(id: string) {
    const cash = this.request(await this.job(id)).method === 'Cash'
    await this.call(
      cash ? 'commerce' : 'membership',
      cash ? 'reconcile_checkout' : 'reconcile_panda_claim',
      [unhex(id)]
    )
    return this.status(id)
  }
  async refresh(id: string) {
    await this.call('membership', 'refresh_panda_claim', [unhex(id)])
    return this.status(id)
  }
  async cancel(id: string) {
    const cash = this.request(await this.job(id)).method === 'Cash'
    await this.call(
      cash ? 'commerce' : 'membership',
      cash ? 'cancel_checkout' : 'cancel_panda_application',
      [unhex(id)]
    )
    return this.status(id)
  }
  async funding(id: string, block: string) {
    ensure(/^(0|[1-9][0-9]*)$/.test(block), 'INVALID_INPUT')
    const terms = this.terms(await this.job(id))
    ensure('cash' in terms, 'FORBIDDEN')
    await this.call('commerce', 'check_checkout_funding', [
      unhex(id),
      { ledger: terms.cash.ledger, block_index: BigInt(block) }
    ])
    return this.status(id)
  }
  async refund(id: string, blocks: string[], ledger?: string) {
    const terms = this.terms(await this.job(id))
    ensure(
      'cash' in terms && blocks.every((b) => /^(0|[1-9][0-9]*)$/.test(b)),
      'INVALID_INPUT'
    )
    // Same selected deposits use the same operation even after a lost reply.
    const refundLedger = ledger ? Principal.fromText(ledger).toUint8Array() : terms.cash.ledger
    const operation = digest('dmsg/refund-selection/v2', [
      unhex(id),
      refundLedger,
      blocks.map(BigInt).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))
    ])
    return this.call<import('@dmsg/sdk').CashTransfer>('commerce', 'claim_checkout_refund', [
      unhex(id),
      refundLedger,
      blocks.map(BigInt),
      operation
    ])
  }
  async refundFees(id: string) {
    return this.call<import('@dmsg/sdk').CashTransfer>(
      'commerce',
      'claim_checkout_fee_reserve',
      [unhex(id)]
    )
  }
  async collect(id: string) {
    return this.call<import('@dmsg/sdk').CashTransfer>(
      'commerce',
      'collect_checkout_revenue',
      [unhex(id)]
    )
  }
  async processTransfer(id: string, block?: string) {
    const t = await this.call<import('@dmsg/sdk').CashTransfer>(
      'commerce',
      'get_checkout_transfer',
      [unhex(id)]
    )
    return block
      ? this.call('commerce', 'reconcile_checkout_transfer', [
          unhex(id),
          { ledger: t.ledger, block_index: BigInt(block) }
        ])
      : this.call('commerce', 'process_checkout_transfer', [unhex(id)])
  }
  async reviseFee(id: string, fee: string) {
    return this.call<import('@dmsg/sdk').CashTransfer>(
      'commerce',
      'revise_checkout_transfer_fee',
      [unhex(id), BigInt(fee)]
    )
  }
}
const globalId = id
