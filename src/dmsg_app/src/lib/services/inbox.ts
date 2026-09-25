import { Principal } from '@icp-sdk/core/principal'
import { IDL } from '@icp-sdk/core/candid'
import type {
  _SERVICE as Payment,
  OpenEscrow,
  EscrowInfo
} from '../canisters/generated/payment'
import { AccountClient, controlResult } from './account'
import { RelayError, type CloudClient } from './relay'
import { CloudSession } from './cloud-session'
import { readCloudSecurity } from './cloud-security'
import { certifiedValue } from './certified'
import { verifyChannelEvent } from './channel-proof'
import {
  offerDigest,
  offerFromWire,
  quoteFromWire,
  receiptFromWire,
  deliveryQuoteDigest,
  admissionDigest,
  paymentLeafKey,
  paymentMethod,
  paymentAmount,
  encodePayment,
  decodePayment
} from '../protocol/payment'
import { candidValue } from '../protocol/account'
import {
  b64,
  unb64,
  unhex,
  hex,
  id,
  utf8,
  hash,
  canonical,
  decodeCanonical,
  equal
} from '../protocol/codec'
import {
  readCloudCommand,
  signCloudCommand,
  verifyCloudCommand,
  type CloudAction
} from '../protocol/cloud'
import { ed25519 } from '../crypto/primitives'
import { ensure } from '../errors'
import { xidBytes } from '../protocol/identity'
import { WalletClient } from './wallet'

export class InboxClient {
  readonly session: CloudSession
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient,
    readonly payment: Payment,
    readonly paymentId: string,
    readonly accountId: string
  ) {
    this.session = new CloudSession(account, cloud, accountId)
  }
  private async journal(key: string) {
    const raw = await this.account.crypto.call('commerceJournal', `inbox:${key}`)
    return raw ? JSON.parse(raw) : null
  }
  private save(key: string, value: unknown) {
    return this.account.crypto.call('commerceJournal', `inbox:${key}`, JSON.stringify(value))
  }
  private async post(
    key: string,
    path: string,
    action: CloudAction,
    payload: Record<string, unknown>,
    repeat = false
  ) {
    let job = await this.journal(key)
    if (repeat && job?.result) job = null
    if (job) ensure(equal(canonical(job.payload), canonical(payload)), 'IDEMPOTENCY_CONFLICT')
    if (job?.result) return job.result
    const context = await this.session.context(
      job && job.deadline > Date.now() ? job.requestId : id()
    )
    if (!job || job.deadline <= Date.now()) {
      job = {
        payload,
        requestId: context.requestId,
        deadline: context.deadline,
        signed: await signCloudCommand(context, action, payload, this.session.sign)
      }
      await this.save(key, job)
    }
    const result = await this.cloud.post(path, job.signed, context, this.session.sign)
    job.result = result
    await this.save(key, job)
    return result
  }
  async terms(epoch?: bigint, feeVersion?: bigint) {
    const batch = controlResult(
      await this.payment.get_configuration_certified(
        epoch === undefined ? [] : [epoch],
        feeVersion === undefined ? [] : [feeVersion]
      )
    )
    const config = decodeCanonical<any>(
      (await certifiedValue(batch, this.account.agent, this.paymentId, utf8('configuration')))
        .value
    )
    const selectedEpoch = epoch ?? BigInt(config.signer_epoch)
    const signer = decodeCanonical<any>(
      (
        await certifiedValue(
          batch,
          this.account.agent,
          this.paymentId,
          paymentLeafKey('signer', selectedEpoch)
        )
      ).value
    )
    const feeEntry = batch.entries.find(
      (e) => new TextDecoder().decode(Uint8Array.from(e.key).slice(0, 4)) === 'fee/'
    )!
    const policy = decodeCanonical<any>(
      (
        await certifiedValue(
          batch,
          this.account.agent,
          this.paymentId,
          Uint8Array.from(feeEntry.key)
        )
      ).value
    )
    ensure(
      config.schema === 1 &&
        config.home_user instanceof Uint8Array &&
        equal(config.home_user, this.account.home.toUint8Array()) &&
        signer.epoch == selectedEpoch &&
        signer.public_key instanceof Uint8Array &&
        signer.public_key.length === 32 &&
        (feeVersion === undefined || BigInt(policy.version) === feeVersion),
      'INTEGRITY_FAILED'
    )
    return {
      config: {
        ...config,
        ledger_fee: paymentAmount(config.ledger_fee),
        max_fee: paymentAmount(config.max_fee)
      },
      signer,
      policy: { ...policy, minimum_atomic: paymentAmount(policy.minimum_atomic) }
    }
  }
  async ownPolicy() {
    return this.session.find(`/v1/inboxes/${this.accountId}/policy`)
  }
  async configure(
    mode: 'closed' | 'contacts' | 'free' | 'paid',
    payee: string,
    net: string,
    allow: string[] = []
  ) {
    await this.session.context()
    ensure(
      this.session.state &&
        'Ready' in this.session.state.info.vault_write_state &&
        this.session.state.info.current_root[0]?.generation ===
          BigInt(this.account.meta.rootGeneration),
      'REKEY_REQUIRED'
    )
    const old = await this.ownPolicy(),
      version = (old?.inbox_key_version ?? 0) + 1
    const matches = (policy: any) =>
      policy?.mode === mode &&
      equal(canonical(policy.allow), canonical(allow)) &&
      (mode !== 'paid' ||
        (policy.offer?.offer.recipient.owner === payee &&
          policy.offer.offer.recipient_net === net &&
          policy.offer.offer.expires_at > Date.now()))
    if (
      matches(old) &&
      old.security_epoch === this.session.state!.verified.securityEpoch &&
      old.root_generation === this.account.meta.rootGeneration
    )
      return old
    // Resume the exact signed policy while its embedded key approval is live.
    // After expiry a new explicitly approved CAS at the same version cannot
    // apply twice, even if the original request arrives late.
    const pending = await this.journal(`policy:${old?.version ?? 0}`)
    if (pending && !pending.result && pending.deadline > Date.now()) {
      ensure(matches(pending.payload), 'IDEMPOTENCY_CONFLICT')
      return this.post(
        `policy:${old?.version ?? 0}`,
        `/v1/inboxes/${this.accountId}/policy`,
        'dmsg/inbox/policy/v1',
        pending.payload
      )
    }
    const key = await this.account.crypto.call('inboxKey', version)
    const context = await this.session.context(),
      keyDescriptor = await signCloudCommand(
        { ...context, requestId: id() },
        'dmsg/inbox/key/v1',
        {
          inbox_key_version: version,
          inbox_hpke_pub: key.publicKey,
          root_generation: key.rootGeneration
        },
        this.session.sign
      )
    let offer: any = null
    if (mode === 'paid') {
      ensure(
        this.session.state.device?.input.capabilities.some((c) => 'PaymentOffer' in c),
        'FORBIDDEN',
        '先在设备与认证中明确启用收款条款批准能力。'
      )
      const terms = await this.terms()
      const value = {
        account_id: this.accountId,
        device_id: this.account.meta.deviceId,
        security_epoch: this.session.state.verified.securityEpoch,
        home_payment: this.paymentId,
        offer_id: id(),
        ledger: Principal.fromUint8Array(terms.config.ledger).toText(),
        recipient: { owner: Principal.fromText(payee).toText(), subaccount: null },
        recipient_net: net,
        quote_scope: id(),
        version: (old?.version ?? 0) + 1,
        issued_at: Date.now(),
        expires_at: Date.now() + 86400000
      }
      ensure(BigInt(net) > 0n, 'INVALID_INPUT')
      offer = {
        offer: value,
        signature: b64(
          await this.account.crypto.call(
            'deviceSign',
            offerDigest(offerFromWire({ offer: value, signature: '' }).offer)
          )
        )
      }
    }
    const payload = {
      expected_version: old?.version ?? 0,
      prev_hash: old?.hash ?? null,
      mode,
      block: [],
      allow,
      invitations: [],
      capacity: 100,
      inbox_key_version: version,
      inbox_hpke_pub: key.publicKey,
      root_generation: key.rootGeneration,
      key_descriptor: keyDescriptor,
      offer
    }
    const result = await this.post(
      pending
        ? `policy:${payload.expected_version}:${id()}`
        : `policy:${payload.expected_version}`,
      `/v1/inboxes/${this.accountId}/policy`,
      'dmsg/inbox/policy/v1',
      payload
    )
    return result
  }
  async recipientPolicy(recipient: string) {
    await this.session.context()
    const peer = await readCloudSecurity(this.account.user, this.account.agent, {
      accountId: recipient,
      issuer: this.session.state!.info.issuer.slice(0, -this.accountId.length) + recipient,
      homeUser: this.account.home.toText()
    })
    await this.cloud.publishSecurity(peer.evidence)
    const policy = await this.session.get(`/public/inboxes/${recipient}/policy`)
    const key = readCloudCommand(policy.key_descriptor),
      device = peer.devices.find(
        (d) => hex(Uint8Array.from(d.input.device_id)) === hex(key.kid) && !d.revoked_at.length
      )
    ensure(device && policy.security_epoch === peer.securityEpoch, 'POLICY_STALE')
    const descriptor = verifyCloudCommand(
      policy.key_descriptor,
      { issuer: String(peer.snapshot.issuer), deviceId: hex(key.kid) },
      Uint8Array.from(device.input.signing_pub),
      'dmsg/inbox/key/v1'
    ).payload
    ensure(
      descriptor.inbox_hpke_pub === policy.inbox_hpke_pub &&
        descriptor.inbox_key_version === policy.inbox_key_version &&
        descriptor.root_generation === policy.root_generation,
      'INTEGRITY_FAILED'
    )
    if (policy.offer) {
      const offer = offerFromWire(policy.offer),
        author = peer.devices.find(
          (d) =>
            hex(Uint8Array.from(d.input.device_id)) ===
              hex(Uint8Array.from(offer.offer.device_id)) &&
            !d.revoked_at.length &&
            d.input.capabilities.some((c) => 'PaymentOffer' in c)
        )
      ensure(
        author &&
          ed25519.verify(
            Uint8Array.from(offer.signature),
            offerDigest(offer.offer),
            Uint8Array.from(author.input.signing_pub)
          ) &&
          offer.offer.expires_at > BigInt(Date.now()),
        'AUTH_REQUIRED'
      )
    }
    return policy
  }
  async contact(recipient: string, text: string, payer: string, orderId = id()) {
    const prior = await this.journal(`outgoing:${orderId}`)
    if (prior) {
      ensure(
        prior.recipient === recipient &&
          prior.textDigest === hash(utf8(text)) &&
          (prior.payload.payer?.owner ?? '') === payer,
        'IDEMPOTENCY_CONFLICT'
      )
      try {
        return await this.status(recipient, orderId)
      } catch (error) {
        if (!(error instanceof RelayError && error.code === 'NOT_FOUND')) throw error
        return this.post(
          `contact:${orderId}`,
          `/v1/inboxes/${recipient}/contacts`,
          'dmsg/inbox/contact/v1',
          prior.payload
        )
      }
    }
    const policy = await this.recipientPolicy(recipient),
      ciphertext = await this.account.crypto.call(
        'inboxSeal',
        recipient,
        policy.inbox_key_version,
        policy.inbox_hpke_pub,
        orderId,
        text
      )
    const payload = {
      recipient,
      order_id: orderId,
      inbox_key_version: policy.inbox_key_version,
      ciphertext,
      invitation: null,
      ...(payer
        ? { payer: { owner: Principal.fromText(payer).toText(), subaccount: null } }
        : {})
    }
    await this.save(`outgoing:${orderId}`, {
      recipient,
      textDigest: hash(utf8(text)),
      payload
    })
    return this.post(
      `contact:${orderId}`,
      `/v1/inboxes/${recipient}/contacts`,
      'dmsg/inbox/contact/v1',
      payload
    )
  }
  async status(recipient: string, order: string) {
    const value = await this.session.get(`/v1/inboxes/${recipient}/orders/${order}`)
    const prior = await this.journal(`outgoing:${order}`)
    if (prior)
      ensure(
        value.order_id === order &&
          value.recipient === prior.recipient &&
          value.ciphertext === prior.payload.ciphertext &&
          value.sender === this.accountId,
        'INTEGRITY_FAILED'
      )
    return value
  }
  async outgoing() {
    return (await this.account.crypto.call('commerceJournals'))
      .filter((row) => row.key.startsWith('inbox:outgoing:'))
      .map((row) => {
        const value = JSON.parse(row.value)
        return { id: row.key.slice('inbox:outgoing:'.length), recipient: value.recipient }
      })
  }
  async resume(order: string) {
    const prior = await this.journal(`outgoing:${order}`)
    ensure(prior, 'NOT_FOUND')
    try {
      return await this.status(prior.recipient, order)
    } catch (error) {
      if (!(error instanceof RelayError && error.code === 'NOT_FOUND')) throw error
      return this.post(
        `contact:${order}`,
        `/v1/inboxes/${prior.recipient}/contacts`,
        'dmsg/inbox/contact/v1',
        prior.payload
      )
    }
  }
  async openEscrow(order: any) {
    const local = await this.journal(`outgoing:${order.order_id}`)
    ensure(
      local &&
        order.recipient === local.recipient &&
        order.sender === this.accountId &&
        order.ciphertext === local.payload.ciphertext &&
        equal(canonical(order.quote?.payer), canonical(local.payload.payer)) &&
        order.quote &&
        order.offer &&
        order.quote_signature &&
        order.quote.home_payment === this.paymentId &&
        order.quote.envelope_digest === hash(unb64(order.ciphertext)),
      'INTEGRITY_FAILED'
    )
    const quote = quoteFromWire(order.quote),
      offer = offerFromWire(order.offer),
      terms = await this.terms(quote.signer_epoch, quote.fee_policy_version)
    const fee = (quote.recipient_net * BigInt(terms.policy.rate_bps) + 9999n) / 10000n,
      serviceFee =
        fee > BigInt(terms.policy.minimum_atomic) ? fee : BigInt(terms.policy.minimum_atomic)
    ensure(
      terms.config.enabled &&
        !terms.signer.revoked &&
        BigInt(terms.signer.valid_from) <= quote.created_at &&
        quote.accept_by < BigInt(terms.signer.valid_until) &&
        quote.amount === quote.recipient_net + quote.service_fee + quote.fee_reserve &&
        quote.service_fee === serviceFee &&
        equal(quote.ledger.toUint8Array(), terms.config.ledger) &&
        equal(quote.platform.owner.toUint8Array(), terms.config.platform.owner) &&
        equal(
          canonical(
            quote.platform.subaccount[0] ? Uint8Array.from(quote.platform.subaccount[0]) : null
          ),
          canonical(terms.config.platform.subaccount)
        ) &&
        equal(Uint8Array.from(offer.offer.account_id), xidBytes(order.recipient)) &&
        quote.home_payment.toText() === offer.offer.home_payment.toText() &&
        quote.ledger.toText() === offer.offer.ledger.toText() &&
        equal(canonical(order.quote.recipient), canonical(order.offer.offer.recipient)) &&
        quote.recipient_net === offer.offer.recipient_net &&
        equal(Uint8Array.from(quote.quote_scope), Uint8Array.from(offer.offer.quote_scope)) &&
        equal(offerDigest(offer.offer), Uint8Array.from(quote.offer_digest)) &&
        ed25519.verify(
          unb64(order.quote_signature),
          deliveryQuoteDigest(quote),
          terms.signer.public_key
        ),
      'INTEGRITY_FAILED'
    )
    let job = await this.journal(`escrow:${order.order_id}`)
    const input: OpenEscrow = {
      op_id: unhex(order.order_id),
      quote,
      offer,
      quote_signature: unb64(order.quote_signature)
    }
    const encoded = encodePayment('open_escrow', [input])
    if (job) ensure(job.input === encoded, 'IDEMPOTENCY_CONFLICT')
    if (!job) {
      job = { input: encoded, state: 'prepared' }
      await this.save(`escrow:${order.order_id}`, job)
    }
    const prior = await this.payment.get_escrow_by_operation(
      quote.payer.owner,
      unhex(order.order_id)
    )
    if ('Ok' in prior) return this.escrow(hex(Uint8Array.from(prior.Ok.escrow_id)))
    ensure('NotFound' in prior.Err, 'EXECUTION_UNKNOWN')
    job.state = 'unknown'
    await this.save(`escrow:${order.order_id}`, job)
    const result = controlResult(
      await this.payment.open_escrow(decodePayment('open_escrow', job.input)[0] as OpenEscrow)
    )
    job.state = 'opened'
    job.escrow = hex(Uint8Array.from(result.escrow_id))
    await this.save(`escrow:${order.order_id}`, job)
    return this.escrow(job.escrow)
  }
  async escrow(id: string) {
    const value = controlResult(await this.payment.get_escrow(unhex(id))),
      batch = controlResult(await this.payment.get_escrow_certified([unhex(id)]))
    const proof = await certifiedValue(batch, this.account.agent, this.paymentId, unhex(id))
    const result = paymentMethod('get_escrow').retTypes[0] as IDL.VariantClass,
      type = result._fields.find(([name]) => name === 'Ok')![1]
    ensure(equal(canonical(candidValue(type, value)), proof.value), 'INTEGRITY_FAILED')
    return value
  }
  async fund(escrow: EscrowInfo, wallet: WalletClient) {
    const terms = await this.terms(escrow.quote.signer_epoch, escrow.quote.fee_policy_version)
    const block = await wallet.transferEscrow(escrow, BigInt(terms.config.ledger_fee))
    controlResult(await this.payment.check_funding(escrow.escrow_id, block))
    return this.escrow(hex(Uint8Array.from(escrow.escrow_id)))
  }
  async admit(order: any, escrow: EscrowInfo) {
    const batch = controlResult(await this.payment.get_escrow_certified([escrow.escrow_id])),
      entry = batch.entries[0]
    await certifiedValue(
      batch,
      this.account.agent,
      this.paymentId,
      Uint8Array.from(escrow.escrow_id)
    )
    const proof = {
      canister: this.paymentId,
      certificate: b64(Uint8Array.from(batch.certificate)),
      escrow_id: hex(Uint8Array.from(escrow.escrow_id)),
      value: b64(Uint8Array.from(entry.value[0]!)),
      witness: b64(Uint8Array.from(entry.witness))
    }
    // An admission is unique per order. Read the committed receipt before
    // refreshing a time-limited funds proof after a lost response.
    let result = await this.status(order.recipient, order.order_id)
    if (!result.receipt)
      result = await this.post(
        `admit:${order.order_id}:${hash(canonical(proof))}`,
        `/v1/inboxes/${order.recipient}/admit`,
        'dmsg/inbox/admit/v1',
        { order_id: order.order_id, proof }
      )
    ensure(result.receipt, 'EXECUTION_UNKNOWN')
    const receipt = receiptFromWire(result.receipt),
      terms = await this.terms(receipt.receipt.signer_epoch, escrow.quote.fee_policy_version)
    ensure(
      ed25519.verify(
        Uint8Array.from(receipt.signature),
        admissionDigest(receipt.receipt),
        terms.signer.public_key
      ) &&
        equal(
          Uint8Array.from(receipt.receipt.quote_digest),
          deliveryQuoteDigest(escrow.quote)
        ) &&
        hex(Uint8Array.from(receipt.receipt.envelope_digest)) === order.envelope_digest,
      'INTEGRITY_FAILED'
    )
    const finalized = controlResult(await this.payment.finalize_receipt(receipt))
    const fresh = controlResult(
        await this.payment.get_escrow_certified([finalized.escrow_id])
      ),
      leaf = fresh.entries[0]
    result = await this.cloud.postRaw(
      `/v1/inboxes/${order.recipient}/orders/${order.order_id}/reconcile`,
      {
        canister: this.paymentId,
        certificate: b64(Uint8Array.from(fresh.certificate)),
        escrow_id: hex(Uint8Array.from(finalized.escrow_id)),
        value: b64(Uint8Array.from(leaf.value[0]!)),
        witness: b64(Uint8Array.from(leaf.witness))
      },
      await this.session.context(),
      this.session.sign
    )
    return {
      order: result,
      escrow: await this.escrow(hex(Uint8Array.from(finalized.escrow_id)))
    }
  }
  async list() {
    const entries = new Map<string, any>()
    let after = 0
    for (let pageNumber = 0; pageNumber < 10000; pageNumber++) {
      const page = await this.session.get(
        `/v1/inboxes/${this.accountId}?after=${after}&limit=20`
      )
      ensure(
        Array.isArray(page.entries) &&
          Number.isSafeInteger(page.next_cursor) &&
          page.next_cursor >= after,
        'INTEGRITY_FAILED'
      )
      for (const row of page.entries) {
        ensure(row.cursor > after && row.cursor <= page.next_cursor, 'INTEGRITY_FAILED')
        const value = row.value
        ensure(
          ['visible', 'aborted'].includes(value.state) && value.recipient === this.accountId,
          'FORBIDDEN'
        )
        if (value.state === 'aborted') {
          ensure(
            /^[0-9a-f]{64}$/.test(value.order_id) &&
              typeof value.sender === 'string' &&
              typeof value.resolved === 'boolean' &&
              typeof value.archived === 'boolean' &&
              !('ciphertext' in value) &&
              !('signed' in value) &&
              !('evidence' in value),
            'INTEGRITY_FAILED'
          )
          // This is relay management metadata, never an authenticated message.
          const terminal = { ...value, text: '' }
          entries.set(value.order_id, terminal)
          await this.save(`received:${value.order_id}`, terminal)
          continue
        }
        // A letter verified and opened earlier keeps its text; only state fields change.
        const saved = await this.journal(`received:${value.order_id}`)
        let text: string
        if (
          saved?.signed?.cose_sign1 === value.signed?.cose_sign1 &&
          saved.ciphertext === value.ciphertext
        )
          text = saved.text
        else {
          const verified = await verifyChannelEvent(
            { signed: value.signed, evidence: value.evidence, stored_at: value.created_at },
            {
              home: this.account.home.toText(),
              namespace: this.session.state!.info.issuer.slice(0, -this.accountId.length),
              agent: this.account.agent
            }
          )
          ensure(
            verified.account === value.sender &&
              verified.body.action === 'dmsg/inbox/contact/v1' &&
              verified.body.payload.order_id === value.order_id &&
              verified.body.payload.recipient === value.recipient &&
              verified.body.payload.ciphertext === value.ciphertext,
            'INTEGRITY_FAILED'
          )
          text = await this.account.crypto.call('inboxOpen', value)
        }
        entries.set(value.order_id, { ...value, text })
        await this.save(`received:${value.order_id}`, { ...value, text })
      }
      if (page.next_cursor === after) return [...entries.values()]
      after = page.next_cursor
    }
    throw new Error('QUOTA_EXCEEDED')
  }
  async mark(order: string, read: boolean, archived: boolean) {
    return this.post(
      `mark:${order}:${read}:${archived}`,
      `/v1/inboxes/${this.accountId}/mark`,
      'dmsg/inbox/mark/v1',
      { order_id: order, read, archived, reopen_contact: archived },
      true
    )
  }
  async refund(escrow: string) {
    controlResult(await this.payment.expiry_refund(unhex(escrow)))
    return this.escrow(escrow)
  }
}
