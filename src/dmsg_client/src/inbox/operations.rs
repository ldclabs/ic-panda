//! Inbox network operations take the client only for local steps. The
//! unlocked account is bound once; locking stops later signing and writes.

use super::*;
use crate::account::Security;
use crate::client::peers_security;

pub(super) struct Unlocked<'a, H> {
    inner: &'a H,
    account: AccountId,
    generation: u64,
}

impl<'a, H: Hold> Unlocked<'a, H> {
    pub(super) async fn new(inner: &'a H) -> Result<Self> {
        let (account, generation) = inner
            .with(|c| {
                c.keys()?;
                Ok((c.account()?.id, c.session_generation))
            })
            .await?;
        Ok(Self {
            inner,
            account,
            generation,
        })
    }
}

impl<H: Hold> Hold for Unlocked<'_, H> {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        self.inner
            .with(|c| {
                c.keys()?;
                ensure(
                    c.account()?.id == self.account && c.session_generation == self.generation,
                    Error::Locked,
                )?;
                step(c)
            })
            .await
    }
}

/// A journaled command: reuse its request ID and ciphertext after a lost
/// response. Callers serialize mutations of the same order/policy.
pub(super) async fn post<H: Hold>(
    h: &H,
    key: &str,
    path: &str,
    action: &str,
    payload: Json,
    repeat: bool,
) -> Result<Json> {
    if let Some(result) = h
        .with(|c| {
            if let Some(job) = c.inbox_job::<Operation>(key)? {
                if repeat && job.result.is_some() {
                    return Ok(None);
                }
                ensure(
                    job.payload == payload && job.path == path,
                    Error::IdempotencyConflict,
                )?;
                return Ok(job.result);
            }
            Ok(None)
        })
        .await?
    {
        return Ok(result);
    }
    let (relay, (mut job, request)) = h
        .signed(|c| {
            let state = c.fresh_state().ok_or(Error::PolicyStale)?;
            let now = now_ms();
            if action == POLICY {
                ensure(
                    payload["root_generation"].as_u64() == Some(writable_root(c, state)?),
                    Error::RekeyRequired,
                )?;
            }
            let previous = c
                .inbox_job::<Operation>(key)?
                .filter(|j| !repeat || j.result.is_none());
            if let Some(j) = &previous {
                ensure(
                    j.payload == payload && j.path == path,
                    Error::IdempotencyConflict,
                )?;
            }
            let job = match previous {
                Some(job) if job.deadline > now + 5_000 => job,
                previous => {
                    let signer = c.signer(state)?;
                    let request_id = previous.map_or_else(random_hex, |j| j.request_id);
                    let deadline = signer.deadline(now)?;
                    let job = Operation {
                        action: action.into(),
                        path: path.into(),
                        payload: payload.clone(),
                        cose: sign_command(&signer, action, &payload, &request_id, deadline),
                        request_id,
                        deadline,
                        result: None,
                    };
                    c.save_inbox_job(key, &job)?;
                    job
                }
            };
            let request = c.cloud.prepare_signed(
                &c.signer(state)?,
                &job.path,
                &job.cose,
                &job.request_id,
                job.deadline,
                now,
            )?;
            Ok((job, request))
        })
        .await?;
    let result = relay.send(request).await?;
    job.result = Some(result.clone());
    h.with(|c| c.save_inbox_job(key, &job)).await?;
    Ok(result)
}

/// Query an order and verify it against the locally sealed outgoing letter.
/// # Errors
/// As [`Client::inbox_order`].
pub async fn order<H: Hold>(h: &H, recipient: &AccountId, order: &str) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let value = h
        .get(&format!("/v1/inboxes/{recipient}/orders/{order}"))
        .await?;
    h.with(|c| {
        if let Some(prior) = c.inbox_job::<Outgoing>(&format!("outgoing:{order}"))? {
            ensure(
                value["order_id"] == order
                    && value["recipient"] == prior.recipient.as_str()
                    && value["ciphertext"] == prior.payload.ciphertext.as_str()
                    && value["sender"] == c.account()?.id.to_string().as_str(),
                Error::IntegrityFailed,
            )?;
        }
        Ok(value)
    })
    .await
}

/// Mark a letter without waiting under the client. An ACK updates the
/// cached flags immediately; the next incremental page confirms them.
/// # Errors
/// As [`Client::inbox_mark`].
pub async fn mark<H: Hold>(h: &H, order: &str, read: bool, archived: bool) -> Result<Json> {
    ensure_valid(crate::channel_log::is_hash(order), "order")?;
    let h = Unlocked::new(h).await?;
    let me = h.account;
    let result = post(
        &h,
        &format!("mark:{order}:{read}:{archived}"),
        &format!("/v1/inboxes/{me}/mark"),
        "dmsg/inbox/mark/v1",
        json!({"order_id":order,"read":read,"archived":archived,"reopen_contact":archived}),
        true,
    )
    .await?;
    h.with(|c| {
        ensure(
            result["order_id"] == order && result["read"] == read && result["archived"] == archived,
            Error::IntegrityFailed,
        )?;
        let name = format!("{LETTER}{order}");
        if let Some(mut letter) = c.workspace.private::<Letter>(c.keys()?, &name)? {
            letter.read = read;
            letter.archived = archived;
            c.workspace.put_private(c.keys()?, &name, &letter)?;
        }
        Ok(())
    })
    .await?;
    Ok(result)
}

/// Read the quoted terms and create a local approval; no payment is made.
/// # Errors
/// As [`Client::letter_request`].
pub async fn request<H: Hold>(
    h: &H,
    source: &str,
    recipient: &AccountId,
    id: &str,
) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let value = order(&h, recipient, id).await?;
    ensure(!value["quote"].is_null(), Error::NotFound)?;
    let quote: WireQuote =
        serde_json::from_value(value["quote"].clone()).map_err(|_| Error::IntegrityFailed)?;
    let digest = digest(
        "dmsg/letter-approval/v1",
        &(recipient.to_string(), id, &quote),
    );
    h.with(|c| {
        let request = c.approval_create(
            source,
            crate::approval::Action::Letter {
                recipient: recipient.to_string(),
                order: id.into(),
                digest,
                amount: quote.amount,
                ledger: quote.ledger,
            },
            now_ms() + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        c.approval_view(&request)
    })
    .await
}

async fn recipient_policy<H: Hold>(h: &H, recipient: &AccountId) -> Result<Json> {
    let relay = h.with(|c| Ok(c.relay())).await?;
    let path = format!("/public/inboxes/{recipient}/policy");
    // Independent reads: recipient evidence and the public contact policy.
    let accounts = [*recipient];
    let (mut peers, policy) =
        futures_util::try_join!(peers_security(h, &accounts), relay.public(&path))?;
    let security = peers.pop().map(|(_, s)| s).ok_or(Error::NotFound)?;
    let policy = policy.ok_or(Error::NotFound)?;
    verify_recipient_policy(&security, &policy)?;
    Ok(policy)
}

fn verify_recipient_policy(security: &Security, policy: &Json) -> Result<()> {
    let cose = policy["key_descriptor"]["cose_sign1"]
        .as_str()
        .ok_or(Error::IntegrityFailed)?;
    let (device, descriptor): (Hash, KeyDescriptor) = crate::cloud::verify_signed_command(
        cose,
        &security.snapshot.issuer,
        &security.devices,
        "dmsg/inbox/key/v1",
    )?;
    ensure(
        security.active(&device).is_ok()
            && policy["security_epoch"].as_u64() == Some(security.snapshot.security_epoch),
        Error::PolicyStale,
    )?;
    ensure(
        policy["inbox_hpke_pub"] == descriptor.inbox_hpke_pub.as_str()
            && policy["inbox_key_version"].as_u64() == Some(descriptor.inbox_key_version)
            && policy["root_generation"].as_u64() == Some(descriptor.root_generation),
        Error::IntegrityFailed,
    )?;
    if !policy["offer"].is_null() {
        let wire: WireSignedOffer =
            serde_json::from_value(policy["offer"].clone()).map_err(|_| Error::IntegrityFailed)?;
        let offer = Wire::offer(&wire)?;
        let author = security
            .active(&offer.offer.device_id)
            .ok()
            .filter(|d| d.input.capabilities.contains(&Capability::PaymentOffer))
            .ok_or(Error::AuthRequired)?;
        dmsg_protocol::verify(
            &author.input.signing_pub,
            digest("dmsg/payment-offer/v1", &offer.offer).as_slice(),
            offer.signature.as_slice(),
        )
        .map_err(|_| Error::AuthRequired)?;
        ensure(offer.offer.expires_at > now_ms(), Error::AuthRequired)?;
    }
    Ok(())
}

/// Seal and submit a letter, reusing the same journal after a lost reply.
/// # Errors
/// As [`Client::inbox_send`].
pub async fn send<H: Hold>(
    h: &H,
    recipient: &AccountId,
    text: &str,
    id: Option<&str>,
    invitation: Option<&str>,
    pay: bool,
) -> Result<Json> {
    let id = id.map_or_else(random_hex, str::to_string);
    ensure_valid(crate::channel_log::is_hash(&id), "order")?;
    ensure_valid(!text.is_empty() && text.len() <= MAX_LETTER, "text")?;
    ensure_valid(
        invitation.is_none_or(crate::channel_log::is_hash),
        "invitation",
    )?;
    let h = Unlocked::new(h).await?;
    let payer = if pay {
        Some(crate::payer::payer(&h).await?.0)
    } else {
        None
    };
    let wire_payer = payer.map(|owner| WireAccount {
        owner: owner.to_text(),
        subaccount: None,
    });
    let key = format!("outgoing:{id}");
    let text_digest = hex::encode(*sha256(text.as_bytes()));
    let path = format!("/v1/inboxes/{recipient}/contacts");
    let prior = h.with(|c| c.inbox_job::<Outgoing>(&key)).await?;
    if let Some(prior) = prior {
        ensure(
            prior.recipient == recipient.to_string()
                && prior.text_digest == text_digest
                && prior.payload.invitation.as_deref() == invitation
                && prior.payload.payer == wire_payer,
            Error::IdempotencyConflict,
        )?;
        return match order(&h, recipient, &id).await {
            Err(Error::NotFound) => {
                post(
                    &h,
                    &format!("contact:{id}"),
                    &path,
                    CONTACT,
                    serde_json::to_value(&prior.payload).expect("json"),
                    false,
                )
                .await
            }
            other => other,
        };
    }
    let policy = recipient_policy(&h, recipient).await?;
    let payload = h
        .with(|c| {
            if let Some(payer) = payer {
                c.ensure_payer(payer)?;
            }
            let version = policy["inbox_key_version"]
                .as_u64()
                .ok_or(Error::IntegrityFailed)?;
            let public: [u8; 32] =
                hex::decode(policy["inbox_hpke_pub"].as_str().unwrap_or_default())
                    .ok()
                    .and_then(|b| b.try_into().ok())
                    .ok_or(Error::IntegrityFailed)?;
            let me = c.account()?.id.to_string();
            let recipient = recipient.to_string();
            let sealed = hpke_seal(
                &public,
                &canonical(&LetterBody { text: text.into() }),
                &letter_context(&me, &recipient, &id, version),
            )?;
            let payload = ContactPayload {
                recipient: recipient.clone(),
                order_id: id.clone(),
                inbox_key_version: version,
                ciphertext: b64(&canonical(&sealed)),
                invitation: invitation.map(str::to_string),
                payer: wire_payer,
            };
            c.save_inbox_job(
                &key,
                &Outgoing {
                    recipient,
                    text_digest,
                    payload: payload.clone(),
                },
            )?;
            Ok(payload)
        })
        .await?;
    post(
        &h,
        &format!("contact:{id}"),
        &path,
        CONTACT,
        serde_json::to_value(payload).expect("json"),
        false,
    )
    .await
}

async fn payment_home<H: Hold>(h: &H) -> Result<Principal> {
    let home = h.with(|c| Ok(c.account()?.home)).await?;
    let config: dmsg_types::user::UserInit =
        h.reader().await?.query(home, "user_config", ()).await?;
    Ok(config.payment_canister)
}

/// Read escrow status and transfers without holding the client.
/// # Errors
/// As [`Client::inbox_escrow`].
pub async fn escrow<H: Hold>(h: &H, order: &str) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let (payer, ic) = crate::payer::payer(&h).await?;
    let canister = payment_home(&h).await?;
    let payment = Payment { ic: &ic, canister };
    let op_id = crate::upload::hex_id(order)?;
    let escrow = payment
        .by_operation(payer, &op_id)
        .await?
        .ok_or(Error::NotFound)?;
    let (escrow, transfers) = futures_util::try_join!(
        payment.escrow(&escrow.escrow_id),
        payment.transfers(&escrow.escrow_id)
    )?;
    h.with(|c| c.ensure_payer(payer)).await?;
    Ok(
        json!({"escrow":escrow_view(&escrow),"transfers":transfers.iter().map(leg_view).collect::<Vec<_>>()}),
    )
}

fn matches_policy(
    policy: &Json,
    input: &PolicyInput,
    payee: Principal,
    price: Option<u128>,
    now: u64,
) -> bool {
    policy["mode"] == input.mode.as_str()
        && policy["allow"] == json!(input.allow)
        && input
            .block
            .as_ref()
            .is_none_or(|b| policy["block"] == json!(b))
        && input
            .invitations
            .as_ref()
            .is_none_or(|i| policy["invitations"] == json!(i))
        && input
            .capacity
            .is_none_or(|c| policy["capacity"].as_u64() == Some(c))
        && (input.mode != "paid"
            || (policy["offer"]["offer"]["recipient"]["owner"] == payee.to_text().as_str()
                && policy["offer"]["offer"]["recipient_net"]
                    == price.unwrap_or(0).to_string().as_str()
                && policy["offer"]["offer"]["expires_at"]
                    .as_u64()
                    .is_some_and(|t| t > now + 3_600_000)))
}

/// Publish contact rules without holding the client across relay or IC calls.
/// # Errors
/// As [`Client::inbox_configure`].
pub async fn configure<H: Hold>(h: &H, input: &PolicyInput) -> Result<Json> {
    ensure_valid(
        matches!(input.mode.as_str(), "closed" | "contacts" | "free" | "paid"),
        "mode",
    )?;
    for account in input.allow.iter().chain(input.block.iter().flatten()) {
        account
            .parse::<AccountId>()
            .map_err(|_| dmsg_types::invalid("account"))?;
    }
    let h = Unlocked::new(h).await?;
    let payee = match &input.payee {
        Some(text) => Principal::from_text(text).map_err(|_| dmsg_types::invalid("payee"))?,
        None if input.mode == "paid" => crate::payer::payer(&h).await?.0,
        None => Principal::anonymous(),
    };
    let price = match (&input.mode[..], &input.price) {
        ("paid", Some(price)) => {
            let price: u128 = price.parse().map_err(|_| dmsg_types::invalid("price"))?;
            ensure_valid(price > 0, "price")?;
            Some(price)
        }
        ("paid", None) => return Err(dmsg_types::invalid("price")),
        _ => None,
    };
    let old = policy(&h).await?;
    let (_, unchanged) = h
        .signed(|c| {
            let state = c.fresh_state().ok_or(Error::PolicyStale)?;
            let generation = writable_root(c, state)?;
            Ok(old
                .as_ref()
                .filter(|old| {
                    matches_policy(old, input, payee, price, now_ms())
                        && old["security_epoch"].as_u64() == Some(state.snapshot.security_epoch)
                        && old["root_generation"].as_u64() == Some(generation)
                })
                .cloned())
        })
        .await?;
    if let Some(old) = unchanged {
        return Ok(old);
    }
    let expected = old
        .as_ref()
        .and_then(|o| o["version"].as_u64())
        .unwrap_or(0);
    let key = format!("policy:{expected}");
    let pending = h.with(|c| c.inbox_job::<Operation>(&key)).await?;
    let now = now_ms();
    let payload = if let Some(job) = pending
        .as_ref()
        .filter(|j| j.result.is_none() && j.deadline > now + 5_000)
    {
        let payload: PolicyPayload =
            serde_json::from_value(job.payload.clone()).map_err(|_| Error::IntegrityFailed)?;
        ensure(
            matches_policy(
                &serde_json::to_value(payload).expect("json"),
                input,
                payee,
                price,
                now,
            ),
            Error::IdempotencyConflict,
        )?;
        job.payload.clone()
    } else {
        let terms = if price.is_some() {
            let canister = payment_home(&h).await?;
            let home = h.with(|c| Ok(c.account()?.home)).await?;
            let ic = h.reader().await?;
            Some((
                canister,
                Payment { ic: &ic, canister }
                    .terms(home, None, None)
                    .await?,
            ))
        } else {
            None
        };
        h.signed(|c| {
            let state = c.fresh_state().cloned().ok_or(Error::PolicyStale)?;
            let generation = writable_root(c, &state)?;
            if input.mode == "paid" && input.payee.is_none() {
                c.ensure_payer(payee)?;
            }
            prepare_policy(
                c,
                input,
                &old,
                payee,
                price,
                expected,
                &state,
                generation,
                terms.as_ref(),
            )
        })
        .await?
        .1
    };
    let key = if pending
        .as_ref()
        .is_some_and(|j| j.result.is_some() || j.deadline <= now_ms() + 5_000)
    {
        format!("{key}:{}", random_hex())
    } else {
        key
    };
    let result = post(
        &h,
        &key,
        &format!("/v1/inboxes/{}/policy", h.account),
        POLICY,
        payload,
        false,
    )
    .await?;
    h.with(|c| c.workspace.put_private(c.keys()?, CACHED_POLICY, &result))
        .await?;
    Ok(result)
}

fn writable_root(c: &Client, state: &Security) -> Result<u64> {
    let generation = c.roots()?.generation;
    ensure(
        state.snapshot.vault_write_state == dmsg_types::user::VaultWriteState::Ready
            && state.snapshot.content_root_generation == generation,
        Error::RekeyRequired,
    )?;
    Ok(generation)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the immutable inputs of one policy preparation step"
)]
fn prepare_policy(
    c: &mut Client,
    input: &PolicyInput,
    old: &Option<Json>,
    payee: Principal,
    price: Option<u128>,
    expected: u64,
    state: &Security,
    generation: u64,
    terms: Option<&(Principal, Terms)>,
) -> Result<Json> {
    let now = now_ms();
    let version = old
        .as_ref()
        .and_then(|o| o["inbox_key_version"].as_u64())
        .unwrap_or(0)
        + 1;
    let public = c.inbox_key(version)?;
    let signer = c.signer(state)?;
    let deadline = signer.deadline(now)?;
    let descriptor = sign_command(
        &signer,
        "dmsg/inbox/key/v1",
        &KeyDescriptor {
            inbox_key_version: version,
            inbox_hpke_pub: hex::encode(public),
            root_generation: generation,
        },
        &random_hex(),
        deadline,
    );
    let offer = match price {
        Some(net) => {
            ensure(
                state
                    .active(&c.workspace.meta.device_id)?
                    .input
                    .capabilities
                    .contains(&Capability::PaymentOffer),
                Error::Forbidden,
            )?;
            let (canister, terms) = terms.ok_or(Error::IntegrityFailed)?;
            let offer = PaymentOffer {
                account_id: c.account()?.id,
                device_id: c.workspace.meta.device_id,
                security_epoch: state.snapshot.security_epoch,
                home_payment: *canister,
                offer_id: Hash::new(crate::crypto::random_id()),
                ledger: terms.config.ledger,
                recipient: Account {
                    owner: payee,
                    subaccount: None,
                },
                recipient_net: net,
                quote_scope: Hash::new(crate::crypto::random_id()),
                version: expected + 1,
                issued_at: now,
                expires_at: now + dmsg_types::DAY,
            };
            let signature = sign(
                &c.keys()?.signing,
                digest("dmsg/payment-offer/v1", &offer).as_slice(),
            );
            Some(WireSignedOffer {
                offer: Wire::wire_offer(&offer),
                signature: b64(&signature),
            })
        }
        None => None,
    };
    let payload = PolicyPayload {
        expected_version: expected,
        prev_hash: old
            .as_ref()
            .and_then(|o| o["hash"].as_str())
            .map(str::to_string),
        mode: input.mode.clone(),
        block: input.block.clone().unwrap_or_else(|| {
            old.as_ref()
                .and_then(|o| serde_json::from_value(o["block"].clone()).ok())
                .unwrap_or_default()
        }),
        allow: input.allow.clone(),
        invitations: input.invitations.clone().unwrap_or_else(|| {
            old.as_ref()
                .and_then(|o| serde_json::from_value(o["invitations"].clone()).ok())
                .unwrap_or_default()
        }),
        capacity: input.capacity.unwrap_or_else(|| {
            old.as_ref()
                .and_then(|o| o["capacity"].as_u64())
                .unwrap_or(100)
        }),
        inbox_key_version: version,
        inbox_hpke_pub: hex::encode(public),
        root_generation: generation,
        key_descriptor: CoseRef {
            cose_sign1: descriptor,
        },
        offer,
    };

    Ok(serde_json::to_value(payload).expect("json"))
}

/// Run the original escrow operation without holding the client during IC calls.
/// # Errors
/// As [`Client::inbox_refund`].
pub async fn refund<H: Hold>(h: &H, order: &str) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let (payer, ic) = crate::payer::payer(&h).await?;
    let canister = payment_home(&h).await?;
    let payment = Payment { ic: &ic, canister };
    let op_id = crate::upload::hex_id(order)?;
    let escrow = payment
        .by_operation(payer, &op_id)
        .await?
        .ok_or(Error::NotFound)?;
    let mut escrow = payment.escrow(&escrow.escrow_id).await?;
    if escrow.decision == FundsDecision::Pending && now_ms() >= escrow.quote.accept_by {
        h.with(|c| c.ensure_payer(payer)).await?;
        escrow = payment.expiry_refund(&escrow.escrow_id).await?;
    }
    let deposits = payment.deposits(&escrow.escrow_id).await?;
    let blocks: Vec<u64> = deposits
        .iter()
        .filter(|d| d.refundable > 0 && d.from == escrow.quote.payer)
        .map(|d| d.block)
        .collect();
    // The decided payment's remainder, once no payout still needs it.
    let reserve = match escrow.decision {
        FundsDecision::Pending => false,
        _ => match payment.quote_refund(&escrow.escrow_id, &[], true).await {
            Ok(quote) => quote.available > 0,
            Err(Error::Pending) if !blocks.is_empty() => false,
            Err(error) => return Err(error),
        },
    };
    ensure(
        !blocks.is_empty() || reserve || escrow.decision != FundsDecision::Pending,
        Error::Pending,
    )?;
    h.with(|c| c.ensure_payer(payer)).await?;
    let leg = match blocks.is_empty() && !reserve {
        true => None,
        false => Some(
            payment
                .claim_refund(&escrow.escrow_id, &blocks, reserve)
                .await?,
        ),
    };
    if let Some(leg) = &leg {
        h.with(|c| c.ensure_payer(payer)).await?;
        let _ = payment
            .process_transfer(&escrow.escrow_id, leg.leg_id)
            .await;
    }
    let escrow = payment.escrow(&escrow.escrow_id).await?;
    h.with(|c| c.ensure_payer(payer)).await?;
    Ok(json!({"escrow": escrow_view(&escrow), "refund_leg": leg.map(|l| l.leg_id)}))
}

/// Run the original escrow operation without holding the client during IC calls.
/// # Errors
/// As [`Client::payment_revise`].
pub async fn revise<H: Hold>(h: &H, escrow: &Hash, leg: u64) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let (payer, ic) = crate::payer::payer(&h).await?;
    let canister = payment_home(&h).await?;
    let payment = Payment { ic: &ic, canister };
    let current = payment
        .transfers(escrow)
        .await?
        .into_iter()
        .find(|t| t.leg_id == leg)
        .ok_or(Error::NotFound)?;
    let fee = match current.status {
        LegStatus::FeeBlocked => current.expected_fee.ok_or(Error::FeeBlocked)?,
        LegStatus::Rejected => current.fee,
        _ => return Err(Error::VersionConflict),
    };
    h.with(|c| c.ensure_payer(payer)).await?;
    let revised = payment.revise(escrow, leg, fee).await?;
    // The revision is durable; a failed send stays for the next try.
    h.with(|c| c.ensure_payer(payer)).await?;
    let sent = payment
        .process_transfer(escrow, revised.leg_id)
        .await
        .unwrap_or(revised);
    h.with(|c| c.ensure_payer(payer)).await?;
    Ok(leg_view(&sent))
}
