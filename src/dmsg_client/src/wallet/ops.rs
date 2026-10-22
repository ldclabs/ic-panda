//! Wallet operations wait on the network without holding the shared client.
use super::*;

/// Check an unpaid activation's balance before creating its approval request.
/// Confirmed or uncertain payments must continue through their original journal.
pub async fn create_request<H: Hold>(
    h: &H,
    source: &str,
    quote_id: &str,
    spec: WalletSpec,
) -> Result<Json> {
    let key = format!("{ACTIVATION}{quote_id}");
    let (quote, hub, payment) = h
        .with(|c| {
            let quote = c.wallet_saved_quote(quote_id)?;
            validate_spec(&spec, &c.wallet_member()?)?;
            ensure(quote.remaining > 0, Error::QuotaExceeded)?;
            ensure(now_ms() < quote.expires_at_ms, Error::Expired)?;
            Ok((quote, c.wallet_hub()?, c.payment_journal(&key)?))
        })
        .await?;
    let paid = payment
        .as_ref()
        .is_some_and(|p| matches!(p.stage, Stage::Confirmed | Stage::Unknown));
    let payer = if paid {
        None
    } else {
        let (payer, _) = crate::payer::payer(h).await?;
        let ic = h.anonymous().await?;
        let config: HubInit = ic.query(hub, "get_hub_config", ()).await?;
        let ledger = crate::payer::Ledger {
            ic: &ic,
            canister: config.icp_ledger,
        };
        let (balance, fee) = futures_util::try_join!(ledger.balance(payer), ledger.fee())?;
        ensure(fee == u128::from(quote.ledger_fee_e8s), Error::FeeBlocked)?;
        ensure(
            balance >= u128::from(quote.price_e8s) + fee,
            crate::insufficient_funds(),
        )?;
        Some(payer)
    };
    h.with(|c| {
        ensure(
            c.wallet_hub()? == hub
                && c.wallet_saved_quote(quote_id)? == quote
                && c.payment_journal(&key)? == payment,
            Error::VersionConflict,
        )?;
        if let Some(payer) = payer {
            c.ensure_payer(payer)?;
        }
        validate_spec(&spec, &c.wallet_member()?)?;
        ensure(now_ms() < quote.expires_at_ms, Error::Expired)?;
        c.wallet_request(
            source,
            JobKind::Create {
                quote: Box::new(quote),
                spec: Box::new(spec),
            },
        )
    })
    .await
}

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

/// Reuse the live unsigned plan so repeated requests and other devices sign
/// the same bytes. Only an expired or already signed plan needs a replacement.
async fn execution_plan<H: Hold>(
    h: &H,
    view: &WalletView,
    proposal: &Proposal,
) -> Result<ExecutionPlan> {
    let (from, chain) = external_source(&proposal.intent)
        .ok_or_else(|| dmsg_types::invalid("not an external-chain proposal"))?;
    let (_, ic) = identity(h).await?;
    if proposal.plans > 0 && !proposal.signed.iter().any(|s| s.plan == proposal.plans) {
        let existing: Result<ExecutionPlan> = ic
            .query(view.wallet, "get_plan", (proposal.id, proposal.plans))
            .await?;
        let existing = existing?;
        if now_ms() < existing.expires_at_ms {
            decode::verify_plan(view, &proposal.intent, &existing)?;
            return Ok(existing);
        }
    }
    let address = view
        .addresses
        .iter()
        .find(|a| a.id == from)
        .ok_or(Error::NotFound)?;
    let params = params(h, view, proposal, address, chain).await?;
    let plan: Result<ExecutionPlan> = ic
        .update(view.wallet, "prepare_execution", (proposal.id, params))
        .await?;
    let plan = plan?;
    decode::verify_plan(view, &proposal.intent, &plan)?;
    Ok(plan)
}

/// Prepare an external plan without requesting a signature or executing it.
pub async fn prepare<H: Hold>(h: &H, wallet: Principal, id: u64) -> Result<Json> {
    let p = proposal_raw(h, wallet, id).await?;
    let view = state(h, wallet).await?;
    execution_plan(h, &view, &p).await?;
    let mut out = super::proposal(h, wallet, id).await?;
    out["wallet"] = json!(wallet.to_text());
    Ok(out)
}

/// Execute an unguarded intent, or request the local signatures for its exact
/// plan. A device without an eligible key only prepares; it never sends an
/// empty signature set to an address that requires cosigning.
pub async fn execute<H: Hold>(h: &H, source: &str, wallet: Principal, id: u64) -> Result<Json> {
    let proposal = proposal_raw(h, wallet, id).await?;
    let view = state(h, wallet).await?;
    let plan = if external_source(&proposal.intent).is_some() {
        Some(execution_plan(h, &view, &proposal).await?)
    } else {
        None
    };
    let requirements = cosign_requirements(&proposal.intent, &view.addresses);
    if !requirements.is_empty() {
        let (view, proposal, plan, _, requirements) =
            packets::context(h, wallet, id, plan.as_ref().map_or(0, |p| p.seq)).await?;
        let cosigners = h
            .with(|c| c.local_cosigners_for(&view, &requirements))
            .await?;
        if cosigners.is_empty() {
            let mut out = proposal_view(&proposal);
            out["wallet"] = json!(wallet.to_text());
            out["cosign_required"] = json!(true);
            return Ok(out);
        }
        let collect_only = requirements.iter().any(|r| {
            cosigners
                .iter()
                .filter(|id| r.policy.cosigners.contains(id))
                .count()
                < usize::from(r.policy.threshold)
        });
        return h
            .with(|c| {
                c.wallet_request(
                    source,
                    JobKind::Cosign {
                        wallet,
                        name: view.name.clone(),
                        context: Box::new(view),
                        proposal: Box::new(proposal),
                        plan: plan.map(Box::new),
                        cosigners,
                        collect_only,
                    },
                )
            })
            .await;
    }
    match plan {
        Some(plan) => sign_and_send(h, wallet, &view, &proposal, &plan, vec![]).await,
        None => execute_icp(h, wallet, id, vec![]).await,
    }
}

/// Execute or resume an ICP proposal; the canister returns an existing result
/// before checking cosignatures when the proposal already started or settled.
pub(super) async fn execute_icp<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u64,
    cosigs: Vec<CosignerSig>,
) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let out: Result<ExecuteResult> = ic
        .update(
            wallet,
            "execute",
            (ExecuteArgs {
                proposal: id,
                plan: None,
                cosigs,
            },),
        )
        .await?;
    Ok(proposal_view(&out?.proposal))
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
            let current = proposal_raw(h, wallet, proposal.id).await?;
            ensure(current.digest == proposal.digest, Error::IntegrityFailed)?;
            if !collect_only
                && plan.is_none()
                && matches!(
                    current.status,
                    ProposalStatus::Executing | ProposalStatus::Settled
                )
            {
                // The previous execute may have committed before its reply was
                // lost. Resume it without signing again or importing a packet
                // for a proposal that is no longer awaiting execution.
                execute_icp(h, wallet, proposal.id, vec![]).await?
            } else {
                let seq = plan.as_ref().map_or(0, |p| p.seq);
                let mut signatures = vec![];
                for cosigner in &cosigners {
                    let (fresh, digests) =
                        cosign_context(h, wallet, &proposal, plan.as_deref(), &cosigners).await?;
                    signatures.extend(
                        local_cosigs(h, &fresh, *cosigner, proposal.id, seq, &digests).await?,
                    );
                }
                let (fresh, _) =
                    cosign_context(h, wallet, &proposal, plan.as_deref(), &cosigners).await?;
                let collected =
                    packets::keep(h, &fresh, &proposal, plan.as_deref(), signatures).await?;
                if collect_only {
                    collected
                } else {
                    packets::execute(h, wallet, proposal.id, seq).await?
                }
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

/// Re-read state before and after user presence. A cancelled request, changed
/// policy, removed key, frozen wallet or switched/locked account must not sign
/// or publish a packet under an earlier snapshot.
async fn cosign_context<H: Hold>(
    h: &H,
    wallet: Principal,
    approved: &Proposal,
    plan: Option<&ExecutionPlan>,
    cosigners: &[u32],
) -> Result<(WalletView, Vec<Hash>)> {
    let (view, p, current_plan, digests, requirements) =
        packets::context(h, wallet, approved.id, plan.map_or(0, |p| p.seq)).await?;
    ensure(
        p.digest == approved.digest && current_plan.as_ref() == plan,
        Error::IntegrityFailed,
    )?;
    let me = h
        .with(|c| {
            c.keys()?;
            c.wallet_member()
        })
        .await?;
    ensure(
        view.members
            .iter()
            .any(|m| m.id == me && m.status == MemberStatus::Active && m.role != Role::Viewer),
        Error::Forbidden,
    )?;
    for id in cosigners {
        ensure(
            requirements.iter().any(|r| r.policy.cosigners.contains(id))
                && view
                    .cosigners
                    .iter()
                    .any(|c| c.id == *id && (!view.policy.hardware_cosigners || c.input.hardware)),
            Error::Forbidden,
        )?;
    }
    Ok((view, digests))
}

pub(super) async fn local_cosigs<H: Hold>(
    h: &H,
    view: &WalletView,
    cosigner: u32,
    proposal: u64,
    plan: u32,
    digests: &[Hash],
) -> Result<Vec<CosignerSig>> {
    let key = view
        .cosigners
        .iter()
        .find(|c| c.id == cosigner)
        .ok_or(Error::NotFound)?;
    let signer = h.with(|c| c.prepare_cosign(&key.input.public_key)).await?;
    let messages = digests
        .iter()
        .enumerate()
        .map(|(i, d)| cosign_message(view.wallet, proposal, plan, i as u32, d))
        .collect();
    let signatures = signer.sign(messages).await?;
    h.with(|c| c.keys().map(|_| ())).await?;
    ensure(signatures.len() == digests.len(), Error::IntegrityFailed)?;
    Ok(signatures
        .into_iter()
        .enumerate()
        .map(|(i, signature)| CosignerSig {
            cosigner,
            digest_index: i as u32,
            signature: signature.into(),
            authenticator_data: None,
            client_data_json: None,
        })
        .collect())
}
