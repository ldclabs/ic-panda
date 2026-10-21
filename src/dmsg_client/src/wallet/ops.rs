//! Wallet operations wait on the network without holding the shared client.
use super::*;

/// Refund: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn refund<H: Hold>(h: &H, quote_id: &str) -> Result<Json> {
    let quote = h.with(|c| c.wallet_saved_quote(quote_id)).await?;
    let hub = h.with(|c| c.wallet_hub()).await?;
    // The journal names the identity that paid; a deposit paid some
    // other way goes to the current payer.
    let key = format!("{ACTIVATION}{}", hex::encode(*quote.quote_id));
    let owner = match h.with(|c| c.payment_journal(&key)).await? {
        Some(payment) => payment.payer,
        None => crate::payer::payer(h).await?.0,
    };
    let to = Account {
        owner,
        subaccount: None,
    };
    let (_, ic) = identity(h).await?;
    let block: Result<u64> = ic
        .update(hub, "claim_activation_refund", (quote.quote_id, to))
        .await?;
    let block = block?;
    h.with(|c| c.workspace.delete_private(&format!("{QUOTE}{quote_id}")))
        .await?;
    Ok(json!({"quote_id": quote_id, "to": to.to_string(), "block": block}))
}

/// Join request: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn join_request<H: Hold>(h: &H, source: &str, wallet: Principal) -> Result<Json> {
    let hub = h.with(|c| c.wallet_hub()).await?;
    let (_, ic) = identity(h).await?;
    let record: Result<WalletRecord> = ic.update(hub, "sync_wallet", (wallet,)).await?;
    let record = record?;
    ensure(
        record
            .members
            .contains(&h.with(|c| c.wallet_member()).await?),
        Error::Forbidden,
    )?;
    h.with(|c| {
        c.wallet_request(
            source,
            JobKind::Join {
                wallet,
                name: record.name,
            },
        )
    })
    .await
}

/// Decide request: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn decide_request<H: Hold>(
    h: &H,
    source: &str,
    wallet: Principal,
    id: u64,
    decision: Decision,
) -> Result<Json> {
    let view = state(h, wallet).await?;
    let proposal = proposal_raw(h, wallet, id).await?;
    let wanted = match decision {
        Decision::Veto => ProposalStatus::Delayed,
        _ => ProposalStatus::Open,
    };
    ensure(proposal.status == wanted, Error::VersionConflict)?;
    h.with(|c| {
        c.wallet_request(
            source,
            JobKind::Decide {
                wallet,
                name: view.name.clone(),
                context: Box::new(view),
                proposal: Box::new(proposal),
                decision,
            },
        )
    })
    .await
}

/// Propose: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn propose<H: Hold>(
    h: &H,
    wallet: Principal,
    intent: Intent,
    via_agent: Option<String>,
) -> Result<Json> {
    validate_intent(&intent)?;
    let (_, ic) = identity(h).await?;
    let args = ProposeArgs {
        intent,
        op_id: Hash::new(random_id()),
        via_agent,
    };
    let p: Result<Proposal> = ic.update(wallet, "propose", (args,)).await?;
    let mut out = proposal_view(&p?);
    out["wallet"] = json!(wallet.to_text());
    Ok(out)
}

/// Member agents: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn member_agents<H: Hold>(
    h: &H,
    wallet: Principal,
    agent_ids: Vec<String>,
) -> Result<Json> {
    let me = h.with(|c| c.wallet_member()).await?;
    let view = state(h, wallet).await?;
    let member = view
        .members
        .iter()
        .find(|m| m.id == me)
        .ok_or(Error::NotFound)?;
    let intent = Intent::UpdateMember(MemberInput {
        id: me,
        role: member.role,
        label: member.label.clone(),
        agent_ids,
    });
    propose(h, wallet, intent, None).await
}

/// Send: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn send<H: Hold>(
    h: &H,
    wallet: Principal,
    from: u32,
    asset: &str,
    to: &str,
    amount: &str,
    max_fee: &str,
    memo: Option<&str>,
    via_agent: Option<String>,
) -> Result<Json> {
    let view = state(h, wallet).await?;
    let asset = parse_asset(asset)?;
    let to = parse_destination(&asset, to, view.btc_network)?;
    let amount = amount.parse().map_err(|_| dmsg_types::invalid("amount"))?;
    let max_fee = max_fee
        .parse()
        .map_err(|_| dmsg_types::invalid("max_fee"))?;
    let memo = memo
        .map(|m| hex::decode(m).map_err(|_| dmsg_types::invalid("memo")))
        .transpose()?
        .map(Into::into);
    let intent = Intent::Transfer {
        from,
        asset,
        to,
        amount,
        max_fee,
        memo,
    };
    propose(h, wallet, intent, via_agent).await
}

/// Cancel: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn cancel<H: Hold>(h: &H, wallet: Principal, id: u64) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let p: Result<Proposal> = ic.update(wallet, "cancel_proposal", (id,)).await?;
    let mut out = proposal_view(&p?);
    out["wallet"] = json!(wallet.to_text());
    Ok(out)
}

/// Freeze: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn freeze<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let r: Result<()> = ic.update(wallet, "freeze", ()).await?;
    r?;
    Ok(json!({"frozen": true}))
}

/// Refuel: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn refuel<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let r: Result<Option<u128>> = ic.update(wallet, "refuel", ()).await?;
    Ok(json!({"cycles": r?.map(|c| c.to_string())}))
}

/// Reconcile: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn reconcile<H: Hold>(h: &H, wallet: Principal, id: u64) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let r: Result<ExecuteResult> = ic.update(wallet, "reconcile", (id,)).await?;
    Ok(proposal_view(&r?.proposal))
}

/// Execute: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn execute<H: Hold>(h: &H, source: &str, wallet: Principal, id: u64) -> Result<Json> {
    let proposal = proposal_raw(h, wallet, id).await?;
    let view = state(h, wallet).await?;
    let Some((from, chain)) = external_source(&proposal.intent) else {
        // An ICP source whose cosign policy names a key of this device
        // needs the person too; the cosignature covers the proposal digest.
        let icp_source = source_address(&proposal.intent)
            .and_then(|from| view.addresses.iter().find(|a| a.id == from));
        if let Some(address) = icp_source
            && let Some(cosigner) = h.with(|c| c.local_cosigner_of(&view, address)).await?
        {
            return h
                .with(|c| {
                    c.wallet_request(
                        source,
                        JobKind::Cosign {
                            wallet,
                            name: view.name.clone(),
                            context: Box::new(view.clone()),
                            proposal: Box::new(proposal),
                            plan: None,
                            cosigners: vec![cosigner],
                            collect_only: address.cosign.as_ref().is_some_and(|p| p.threshold > 1),
                        },
                    )
                })
                .await;
        }
        let (_, ic) = identity(h).await?;
        let r: Result<ExecuteResult> = ic
            .update(
                wallet,
                "execute",
                (ExecuteArgs {
                    proposal: id,
                    plan: None,
                    cosigs: vec![],
                },),
            )
            .await?;
        return Ok(proposal_view(&r?.proposal));
    };
    let address = view
        .addresses
        .iter()
        .find(|a| a.id == from)
        .cloned()
        .ok_or(Error::NotFound)?;
    let params = params(h, &view, &proposal, &address, chain).await?;
    let (_, ic) = identity(h).await?;
    let plan: Result<ExecutionPlan> = ic.update(wallet, "prepare_execution", (id, params)).await?;
    let plan = plan?;
    decode::verify_plan(&view, &proposal.intent, &plan)?;
    // A cosign policy naming a key of this device needs the person.
    if let Some(cosigner) = h.with(|c| c.local_cosigner_of(&view, &address)).await? {
        return h
            .with(|c| {
                c.wallet_request(
                    source,
                    JobKind::Cosign {
                        wallet,
                        name: view.name.clone(),
                        context: Box::new(view.clone()),
                        proposal: Box::new(proposal),
                        plan: Some(Box::new(plan)),
                        cosigners: vec![cosigner],
                        collect_only: address.cosign.as_ref().is_some_and(|p| p.threshold > 1),
                    },
                )
            })
            .await;
    }
    sign_and_send(h, wallet, &view, &proposal, &plan, vec![]).await
}

/// Sign and send: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn sign_and_send<H: Hold>(
    h: &H,
    wallet: Principal,
    view: &WalletView,
    proposal: &Proposal,
    plan: &ExecutionPlan,
    cosigs: Vec<CosignerSig>,
) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let r: Result<ExecuteResult> = ic
        .update(
            wallet,
            "execute",
            (ExecuteArgs {
                proposal: proposal.id,
                plan: Some(plan.seq),
                cosigs,
            },),
        )
        .await?;
    let r = r?;
    let signed = r.signed.ok_or(Error::IntegrityFailed)?;
    let mut out = proposal_view(&r.proposal);
    match broadcast_signed(h, view, proposal, &signed).await {
        Ok(txid) => {
            let reported = report(
                h,
                wallet,
                proposal.id,
                signed.plan,
                ChainOutcome::Broadcast,
                &format!("broadcast {txid}"),
            )
            .await?;
            out = reported;
            out["broadcast"] = json!(txid);
        }
        // The signed transaction stays on the wallet; broadcast it again later.
        Err(error) => out["broadcast_error"] = json!(error.to_string()),
    }
    Ok(out)
}

/// Broadcast signed: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn broadcast_signed<H: Hold>(
    h: &H,
    view: &WalletView,
    proposal: &Proposal,
    signed: &SignedTx,
) -> Result<String> {
    let chains = h.with(|c| c.chains()).await?;
    match external_source(&proposal.intent).map(|(_, c)| c) {
        Some(Chain::Bitcoin) => chains.btc_broadcast(&signed.raw).await,
        Some(Chain::Evm) => {
            chains
                .evm_send(evm_chain(&proposal.intent)?, &signed.raw)
                .await
        }
        Some(Chain::Solana) => chains.sol_send(&signed.raw).await,
        _ => {
            let _ = view;
            Err(Error::InvalidInput("not an external-chain proposal".into()))
        }
    }
}

/// Broadcast: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn broadcast<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u64,
    plan: Option<u32>,
) -> Result<Json> {
    let view = state(h, wallet).await?;
    let proposal = proposal_raw(h, wallet, id).await?;
    let signed = match plan {
        Some(seq) => proposal.signed.iter().find(|s| s.plan == seq),
        None => proposal.signed.last(),
    }
    .cloned()
    .ok_or(Error::NotFound)?;
    let txid = broadcast_signed(h, &view, &proposal, &signed).await?;
    let mut out = report(
        h,
        wallet,
        id,
        signed.plan,
        ChainOutcome::Broadcast,
        &format!("broadcast {txid}"),
    )
    .await?;
    out["broadcast"] = json!(txid);
    Ok(out)
}

/// Settle: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn settle<H: Hold>(h: &H, wallet: Principal, id: u64) -> Result<Json> {
    let proposal = proposal_raw(h, wallet, id).await?;
    let latest = proposal.signed.last().cloned().ok_or(Error::NotFound)?;
    let chain = external_source(&proposal.intent).map(|(_, c)| c);
    let chains = h.with(|c| c.chains()).await?;
    let (mut pending, mut error) = (false, None);
    for signed in proposal.signed.iter().rev() {
        let status = match chain {
            Some(Chain::Bitcoin) => chains
                .btc_confirmed(&signed.txid)
                .await
                .map(|confirmed| confirmed.then_some(true)),
            Some(Chain::Evm) => {
                chains
                    .evm_receipt(evm_chain(&proposal.intent)?, &signed.txid)
                    .await
            }
            Some(Chain::Solana) => chains.sol_status(&signed.txid).await,
            _ => return Err(Error::InvalidInput("not an external-chain proposal".into())),
        };
        let (outcome, note) = match status {
            Ok(Some(true)) => (ChainOutcome::Settled, "final on chain"),
            Ok(Some(false)) => (ChainOutcome::Failed, "failed on chain"),
            Ok(None) => {
                pending = true;
                continue;
            }
            // A replaced transaction may be unknown to the endpoint.
            Err(e) => {
                error.get_or_insert(e);
                continue;
            }
        };
        return report(h, wallet, id, signed.plan, outcome, note).await;
    }
    match error {
        Some(e) if !pending => Err(e),
        _ => Ok(json!({"pending": true, "txid": latest.txid})),
    }
}

/// Report: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn report<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u64,
    plan: u32,
    outcome: ChainOutcome,
    note: &str,
) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let p: Result<Proposal> = ic
        .update(
            wallet,
            "report",
            (ChainReport {
                proposal: id,
                plan,
                outcome,
                note: note.chars().take(256).collect(),
            },),
        )
        .await?;
    let mut out = proposal_view(&p?);
    out["wallet"] = json!(wallet.to_text());
    Ok(out)
}

/// Params: the client is taken only for synchronous steps.
#[allow(clippy::too_many_arguments)]
pub async fn params<H: Hold>(
    h: &H,
    view: &WalletView,
    p: &Proposal,
    a: &Address,
    chain: Chain,
) -> Result<ExecutionParams> {
    let chains = h.with(|c| c.chains()).await?;
    Ok(match chain {
        Chain::Bitcoin => {
            // A replacement spends the first signed plan's inputs.
            let utxos = match p.signed.first() {
                Some(first) => {
                    let (_, ic) = identity(h).await?;
                    let plan: Result<ExecutionPlan> = ic
                        .query(view.wallet, "get_plan", (p.id, first.plan))
                        .await?;
                    match plan?.params {
                        ExecutionParams::Bitcoin { utxos, .. } => utxos,
                        _ => return Err(Error::IntegrityFailed),
                    }
                }
                None => chains
                    .btc_utxos(&a.text)
                    .await?
                    .into_iter()
                    .take(MAX_BTC_INPUTS)
                    .map(|u| {
                        let mut txid: [u8; 32] = hex::decode(&u.txid)
                            .ok()
                            .and_then(|b| b.try_into().ok())
                            .ok_or(Error::IntegrityFailed)?;
                        txid.reverse();
                        Ok(Utxo {
                            txid: Hash::new(txid),
                            vout: u.vout,
                            value: u.value,
                            address: a.id,
                        })
                    })
                    .collect::<Result<_>>()?,
            };
            ensure(!utxos.is_empty(), Error::FeeBlocked)?;
            let mut rate = chains.btc_fee_rate().await?;
            if let Some(last) = p.signed.last() {
                let (_, ic) = identity(h).await?;
                let previous: Result<ExecutionPlan> =
                    ic.query(view.wallet, "get_plan", (p.id, last.plan)).await?;
                if let ExecutionParams::Bitcoin { fee_rate, .. } = previous?.params {
                    rate = rate.max(fee_rate.saturating_add(fee_rate / 5).saturating_add(1));
                }
            }
            ExecutionParams::Bitcoin {
                utxos,
                // A replacement must pay more than the transaction it replaces.
                fee_rate: rate,
                locktime: 0,
                timelock: false,
            }
        }
        Chain::Evm => {
            let chain_id = evm_chain(&p.intent)?;
            let nonce = match (&p.intent, p.signed.first()) {
                // A replacement keeps the nonce of the transaction it replaces.
                (_, Some(first)) => {
                    let (_, ic) = identity(h).await?;
                    let plan: Result<ExecutionPlan> = ic
                        .query(view.wallet, "get_plan", (p.id, first.plan))
                        .await?;
                    match plan?.params {
                        ExecutionParams::Evm { nonce, .. } => nonce,
                        _ => return Err(Error::IntegrityFailed),
                    }
                }
                // A cancellation takes the oldest nonce not yet mined.
                (Intent::CancelEvmNonce { .. }, None) => {
                    let v = chains
                        .evm(
                            chain_id,
                            "eth_getTransactionCount",
                            json!([a.text, "latest"]),
                        )
                        .await?;
                    u64::from_str_radix(v.as_str().unwrap_or_default().trim_start_matches("0x"), 16)
                        .map_err(|_| Error::Unavailable("nonce".into()))?
                }
                _ => chains.evm_nonce(chain_id, &a.text).await?,
            };
            let (mut max_fee, mut tip) = chains.evm_fees(chain_id).await?;
            if let Some(last) = p.signed.last() {
                let (_, ic) = identity(h).await?;
                let previous: Result<ExecutionPlan> =
                    ic.query(view.wallet, "get_plan", (p.id, last.plan)).await?;
                if let ExecutionParams::Evm {
                    max_fee_per_gas,
                    max_priority_fee_per_gas,
                    ..
                } = previous?.params
                {
                    max_fee = max_fee.max(
                        max_fee_per_gas
                            .saturating_add(max_fee_per_gas / 5)
                            .saturating_add(1),
                    );
                    tip = tip.max(
                        max_priority_fee_per_gas
                            .saturating_add(max_priority_fee_per_gas / 5)
                            .saturating_add(1),
                    );
                }
            } else if matches!(p.intent, Intent::CancelEvmNonce { .. }) {
                max_fee = max_fee.saturating_add(max_fee / 5);
                tip = tip.saturating_add(tip / 5);
            }
            let gas_limit = match &p.intent {
                Intent::Transfer {
                    asset:
                        AssetKey::Evm {
                            token: Some(token), ..
                        },
                    to: Destination::Evm(to),
                    amount,
                    ..
                } => {
                    let data = [
                        &[0xa9, 0x05, 0x9c, 0xbb][..],
                        &[0; 12],
                        &to[..],
                        &[0; 16],
                        &amount.to_be_bytes(),
                    ]
                    .concat();
                    chains
                        .evm_gas(chain_id, &a.text, &eip55(&token.into_array()), &data, 0)
                        .await
                        .unwrap_or(100_000)
                }
                _ => 21_000,
            };
            ExecutionParams::Evm {
                nonce,
                max_fee_per_gas: max_fee,
                max_priority_fee_per_gas: tip,
                gas_limit,
            }
        }
        Chain::Solana => {
            let blockhash = match a.sol_nonce {
                Some(nonce) => chains.sol_nonce(&sol_address(&nonce.into_array())).await?,
                None => chains.sol_blockhash().await?,
            };
            let (mint, owners) = match &p.intent {
                Intent::Transfer {
                    asset: AssetKey::Sol { mint },
                    to: Destination::Solana(owner),
                    ..
                } => (*mint, vec![*owner]),
                Intent::Batch {
                    asset: AssetKey::Sol { mint },
                    items,
                    ..
                } => (
                    *mint,
                    items
                        .iter()
                        .filter_map(|i| {
                            if let Destination::Solana(owner) = i.to {
                                Some(owner)
                            } else {
                                None
                            }
                        })
                        .collect(),
                ),
                _ => (None, vec![]),
            };
            let (decimals, create_ata) = if let Some(mint) = mint {
                let supply = chains
                    .sol(
                        "getTokenSupply",
                        json!([sol_address(&mint.into_array()),{"commitment":"confirmed"}]),
                    )
                    .await?;
                let decimals = supply["value"]["decimals"]
                    .as_u64()
                    .and_then(|d| u8::try_from(d).ok())
                    .ok_or(Error::IntegrityFailed)?;
                let mut missing = false;
                for owner in &owners {
                    let target =
                        decode::associated_token_account(&owner.into_array(), &mint.into_array());
                    missing |= !chains.sol_exists(&sol_address(&target)).await?;
                }
                (Some(decimals), missing)
            } else {
                (None, false)
            };
            ExecutionParams::Solana {
                blockhash,
                compute_unit_price: 1_000,
                compute_unit_limit: (if create_ata { 60_000u32 } else { 20_000u32 })
                    .saturating_mul(owners.len().max(1) as u32),
                decimals,
                create_ata,
            }
        }
        Chain::Icp => return Err(Error::InvalidInput("ICP needs no plan".into())),
    })
}

pub(crate) async fn registrations<H: Hold>(
    h: &H,
) -> Result<(AppRegistration, ProductRegistration)> {
    let init: dmsg_types::user::UserInit = home_config(h).await?;
    let commerce = init.commerce_canister;
    let ic = h.anonymous().await?;
    let batch: Result<CertifiedBatch> = ic
        .query(
            commerce,
            "integration_configuration_certificate",
            (WALLET_APP_ID, Some(WALLET_PRODUCT_ID)),
        )
        .await?;
    let batch = batch?;
    let root = ic.root_key();
    let now = now_ms();
    let app_key = digest("dmsg/registration/app/v1", &WALLET_APP_ID).to_vec();
    let product_key = digest("dmsg/registration/product/v2", &WALLET_PRODUCT_ID).to_vec();
    let app: AppRegistration = crate::codec::decode(
        &certified_leaf(&batch, &app_key, commerce, &root, now)?.ok_or(Error::NotFound)?,
        1 << 20,
    )?;
    let product: ProductRegistration = crate::codec::decode(
        &certified_leaf(&batch, &product_key, commerce, &root, now)?.ok_or(Error::NotFound)?,
        1 << 20,
    )?;
    validate_app(&app)?;
    validate_product(&product)?;
    ensure(
        app.environment == h.with(|c| Ok(c.config.environment.clone())).await?
            && app.origins.iter().any(|o| o == ORIGIN),
        Error::IntegrityFailed,
    )?;
    Ok((app, product))
}

pub(crate) async fn authorize<H: Hold>(
    h: &H,
    job: &mut WalletJob,
    hub: Principal,
    service: Principal,
    purpose: ApprovalPurpose,
    action_digest: Hash,
) -> Result<(Hash, ApplicationApproval)> {
    if job.approval.is_none() {
        let (app, _) = registrations(h).await?;
        let (actor, _) = identity(h).await?;
        let account = h.with(|c| Ok(c.account()?.clone())).await?;
        let state = crate::account::refreshed(h).await?;
        let device = state.active(&h.with(|c| Ok(c.workspace.meta.device_id)).await?)?;
        ensure(
            device
                .input
                .capabilities
                .contains(&Capability::WalletApprove),
            Error::Forbidden,
        )?;
        let now = now_ms();
        let application = ApplicationApproval {
            version: 1,
            environment: app.environment.clone(),
            app_id: app.app_id.clone(),
            app_config_version: app.config_version,
            origin: ORIGIN.into(),
            approving_account: account.id,
            service,
            beneficiary: wallet_beneficiary(hub, service),
            actor,
            purpose,
            action_digest,
            operation_id: digest("dmsg/wallet-operation/v1", &(&job.id, action_digest)),
            nonce: Hash::new(random_id()),
            expires_at_ms: now + APPROVAL_MS,
        };
        let mut approval = Approval {
            device_id: h.with(|c| Ok(c.workspace.meta.device_id)).await?,
            security_epoch: state.info.security_epoch,
            sequence: device.next_sequence,
            request_id: Hash::new(random_id()),
            expires_at: application.expires_at_ms,
            signature: Ed25519Signature::new([0; 64]),
        };
        let message = approval_message(
            account.home,
            &account.id,
            APPLICATION,
            &application,
            &approval,
        );
        approval.signature = h
            .with(|c| {
                Ok(Ed25519Signature::new(sign(
                    &c.keys()?.signing,
                    message.as_slice(),
                )))
            })
            .await?;
        job.approval = Some((application, approval));
        h.with(|c| c.save_wallet_job(job)).await?;
    }
    let (application, approval) = job.approval.clone().ok_or(Error::IntegrityFailed)?;
    let home = h.with(|c| Ok(c.account()?.home)).await?;
    let login = h.with(|c| Ok(c.session()?.ic.clone())).await?;
    let approved: Result<Hash> = login
        .update(home, "approve_application", (&application, &approval))
        .await?;
    approved?;
    Ok((approval.request_id, application))
}

pub(crate) async fn job_execute<H: Hold>(h: &H, id: &str) -> Result<()> {
    let mut job = h.with(|c| c.wallet_job(id)).await?;
    let result = match job.kind.clone() {
        JobKind::Create { quote, spec } => {
            let hub = h.with(|c| c.wallet_hub()).await?;
            let digest = activation_digest(&quote, &spec);
            let (approval_id, approval) =
                authorize(h, &mut job, hub, hub, ApprovalPurpose::WalletCreate, digest).await?;
            pay_quote(h, &quote).await?;
            let (_, ic) = identity(h).await?;
            let order: Result<ActivationOrder> = ic
                .update(
                    hub,
                    "create_wallet",
                    (CreateWallet {
                        quote_id: quote.quote_id,
                        spec: *spec,
                        approval_id,
                        approval,
                    },),
                )
                .await?;
            let order = order?;
            // A quote that created its wallet is spent.
            if order.status == ActivationStatus::Ready {
                h.with(|c| {
                    c.workspace
                        .delete_private(&format!("{QUOTE}{}", hex::encode(*quote.quote_id)))
                })
                .await?;
            }
            json!({
                "status": format!("{:?}", order.status),
                "wallet": order.wallet.map(|w| w.to_text()),
            })
        }
        JobKind::Join { wallet, .. } => {
            let hub = h.with(|c| c.wallet_hub()).await?;
            let (actor, _) = identity(h).await?;
            let account = h.with(|c| Ok(c.account()?.id)).await?;
            let (approval_id, approval) = authorize(
                h,
                &mut job,
                hub,
                wallet,
                ApprovalPurpose::WalletJoin,
                join_digest(wallet, &account, actor),
            )
            .await?;
            let (_, ic) = identity(h).await?;
            let member: Result<Member> = ic
                .update(
                    wallet,
                    "join",
                    (MemberApproval::Account {
                        approval_id,
                        approval,
                    },),
                )
                .await?;
            let member = member?;
            json!({"joined": true, "role": format!("{:?}", member.role)})
        }
        JobKind::Decide {
            wallet,
            proposal,
            decision,
            ..
        } => {
            let hub = h.with(|c| c.wallet_hub()).await?;
            let (approval_id, approval) = authorize(
                h,
                &mut job,
                hub,
                wallet,
                ApprovalPurpose::WalletProposal,
                decision_digest(&proposal.digest, decision),
            )
            .await?;
            let method = match decision {
                Decision::Approve => "approve_proposal",
                Decision::Reject => "reject_proposal",
                Decision::Veto => "veto_proposal",
            };
            let (_, ic) = identity(h).await?;
            let p: Result<Proposal> = ic
                .update(
                    wallet,
                    method,
                    (
                        proposal.id,
                        MemberApproval::Account {
                            approval_id,
                            approval,
                        },
                    ),
                )
                .await?;
            match p {
                Ok(p) => proposal_view(&p),
                // A retry after the decision counted finds it recorded.
                Err(Error::IdempotencyConflict) => json!({"recorded": true}),
                Err(e) => return Err(e),
            }
        }
        JobKind::Cosign {
            wallet,
            proposal,
            plan,
            cosigners,
            collect_only,
            ..
        } => {
            let view = state(h, wallet).await?;
            let current = proposal_raw(h, wallet, proposal.id).await?;
            ensure(current.digest == proposal.digest, Error::IntegrityFailed)?;
            if let Some(plan) = &plan {
                ensure(now_ms() < plan.expires_at_ms, Error::Expired)?;
                decode::verify_plan(&view, &proposal.intent, plan)?;
            }
            let seq = plan.as_ref().map_or(0, |p| p.seq);
            let digests = plan
                .as_ref()
                .map(|p| p.digests.clone())
                .unwrap_or_else(|| vec![proposal.digest]);
            let mut signatures = vec![];
            for cosigner in cosigners {
                signatures
                    .extend(local_cosigs(h, &view, cosigner, proposal.id, seq, &digests).await?);
            }
            let collected = packets::keep(h, &view, &proposal, plan.as_deref(), signatures).await?;
            if collect_only {
                collected
            } else {
                packets::execute(h, wallet, proposal.id, seq).await?
            }
        }
    };
    job.result = Some(result);
    h.with(|c| c.save_wallet_job(&job)).await?;
    let target = match &job.kind {
        JobKind::Create { .. } => job
            .result
            .as_ref()
            .and_then(|r| r["wallet"].as_str())
            .and_then(|s| parse_wallet(s).ok())
            .map(|w| (w, None)),
        JobKind::Join { wallet, .. } => Some((*wallet, None)),
        JobKind::Decide {
            wallet, proposal, ..
        }
        | JobKind::Cosign {
            wallet, proposal, ..
        } => Some((*wallet, Some(proposal.id))),
    };
    if let Some((wallet, proposal)) = target
        && let Err(error) = desktop::notify(h, wallet, proposal).await
    {
        if let Some(result) = job.result.as_mut() {
            result["notification_error"] = json!(error.to_string());
        }
        h.with(|c| c.save_wallet_job(&job)).await?;
    }
    Ok(())
}

pub(crate) async fn pay_quote<H: Hold>(h: &H, quote: &ActivationQuote) -> Result<u64> {
    let key = format!("{ACTIVATION}{}", hex::encode(*quote.quote_id));
    let hub = h.with(|c| c.wallet_hub()).await?;
    let reader = h.anonymous().await?;
    let config: HubInit = reader.query(hub, "get_hub_config", ()).await?;
    let now = now_ms();
    if h.with(|c| c.payment_journal(&key)).await?.is_none() {
        ensure(
            now + dmsg_types::MINUTE < quote.expires_at_ms,
            Error::Expired,
        )?;
    }
    let deposit: Account = quote.deposit;
    let amount = u128::from(quote.price_e8s);
    let created_at = now * 1_000_000;
    crate::payer::pay(
        h,
        &key,
        config.icp_ledger,
        u128::from(quote.ledger_fee_e8s),
        || {
            crate::payer::transfer_call(&icrc_ledger_types::icrc1::transfer::TransferArg {
                from_subaccount: None,
                to: deposit,
                fee: Some(u128::from(quote.ledger_fee_e8s).into()),
                created_at_time: Some(created_at),
                memo: Some(crate::payer::memo(&quote.quote_id[..])),
                amount: amount.into(),
            })
        },
    )
    .await
}

async fn home_config<H: Hold>(h: &H) -> Result<dmsg_types::user::UserInit> {
    let home = h.with(|c| Ok(c.account()?.home)).await?;
    h.reader().await?.query(home, "user_config", ()).await
}

async fn local_cosigs<H: Hold>(
    h: &H,
    view: &WalletView,
    cosigner: u32,
    proposal: u64,
    plan: u32,
    digests: &[Hash],
) -> Result<Vec<CosignerSig>> {
    h.with(|c| c.local_cosigs(view, cosigner, proposal, plan, digests))
        .await
}
