//! Executions: internal changes, ICRC transfers, canister calls, cycles
//! top-ups, new addresses and external-chain signing.
use crate::api::{check_state, close, now, save_closed};
use crate::chains::{self, btc::BtcSpend, Assembly};
use crate::ledger::{self, LedgerItem, LedgerJob, Notified, Sent, TopUp};
use crate::store::{self, EvmPending, Job, PlanRecord, SignSpec, SolLast, Wallet};
use candid::Principal;
use dmsg_protocol::wallet::*;
use dmsg_types::{wallet::*, *};
use ic_cdk::call::{Call, CallFailed, RejectCode};
use sha2::{Digest, Sha256};

/// Intents that change only the wallet record.
fn internal(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::SetCosign { .. }
            | Intent::AddCosigner(_)
            | Intent::RemoveCosigner(_)
            | Intent::AddMember(_)
            | Intent::RemoveMember(_)
            | Intent::UpdateMember(_)
            | Intent::SetPolicy(_)
            | Intent::AddressBook(_)
            | Intent::Unfreeze
            | Intent::AcceptUpgrade { .. }
            | Intent::LockCode
            | Intent::ReleaseEvmNonce { .. }
    )
}

fn settle(w: &mut Wallet, p: &mut Proposal, outcome: Outcome, at: u64) {
    p.status = ProposalStatus::Settled;
    p.outcome = Some(outcome);
    p.updated_at_ms = at;
    store::audit(w, at, None, Some(p.id), AuditKind::Settled);
}

/// A definite failure. The window spend is released only when nothing
/// moved and nothing still can: a partly paid batch or a signed transaction
/// keeps counting until the window passes.
fn fail(w: &mut Wallet, p: &mut Proposal, reason: String, release: bool, at: u64) {
    p.status = ProposalStatus::Failed;
    p.failure = Some(reason.clone());
    p.updated_at_ms = at;
    if release {
        w.spends.retain(|s| s.proposal != p.id);
    }
    store::audit(w, at, None, Some(p.id), AuditKind::Failed(reason));
}

/// Apply an internal intent the moment it is Ready.
pub(crate) fn apply_ready(w: &mut Wallet, p: &mut Proposal, at: u64) {
    if !internal(&p.intent) {
        return;
    }
    match check_state(w, &p.intent).and_then(|()| apply(w, &p.intent, at)) {
        Ok(outcome) => settle(w, p, outcome, at),
        Err(e) => fail(w, p, e.to_string(), true, at),
    }
}

/// Remove cosigners from every cosign policy; a policy left without enough
/// cosigners lowers its threshold, one left empty is dropped.
fn strip_cosigners(w: &mut Wallet, removed: &[u32]) {
    w.cosigners.retain(|c| !removed.contains(&c.id));
    for a in &mut w.addresses {
        if let Some(policy) = &mut a.cosign {
            policy.cosigners.retain(|c| !removed.contains(c));
            policy.threshold = policy.threshold.min(policy.cosigners.len() as u8);
            if policy.cosigners.is_empty() {
                a.cosign = None;
            }
        }
    }
}

fn apply(w: &mut Wallet, intent: &Intent, at: u64) -> Result<Outcome> {
    match intent {
        Intent::SetCosign { address, cosign } => {
            let a = w
                .addresses
                .iter_mut()
                .find(|a| a.id == *address)
                .ok_or(Error::NotFound)?;
            a.cosign = cosign.clone();
        }
        Intent::AddCosigner(input) => {
            let id = w.next_cosigner;
            w.next_cosigner += 1;
            w.cosigners.push(Cosigner {
                id,
                input: input.clone(),
                counter: 0,
                added_at_ms: at,
            });
            return Ok(Outcome::CosignerAdded(id));
        }
        Intent::RemoveCosigner(id) => strip_cosigners(w, &[*id]),
        Intent::AddMember(input) => {
            w.members.push(Member {
                id: input.id.clone(),
                role: input.role,
                label: input.label.clone(),
                agent_ids: input.agent_ids.clone(),
                status: MemberStatus::Invited,
                principals: vec![],
                added_at_ms: at,
            });
        }
        Intent::RemoveMember(id) => {
            w.members.retain(|m| m.id != *id);
            // A native Taproot address's script commits to its cosigner's
            // key, so that cosigner stays for the address to remain spendable.
            let owned: Vec<u32> = w
                .cosigners
                .iter()
                .filter(|c| c.input.member == *id && !w.taproot_cosigner(c.id))
                .map(|c| c.id)
                .collect();
            strip_cosigners(w, &owned);
        }
        Intent::UpdateMember(input) => {
            let m = w
                .members
                .iter_mut()
                .find(|m| m.id == input.id)
                .ok_or(Error::NotFound)?;
            m.role = input.role;
            m.label = input.label.clone();
            m.agent_ids = input.agent_ids.clone();
        }
        Intent::SetPolicy(policy) => w.policy = policy.clone(),
        Intent::AddressBook(change) => store::set_book(change, at),
        Intent::Unfreeze => w.frozen = false,
        Intent::AcceptUpgrade {
            version,
            module_hash,
        } => {
            w.code = CodeAcceptance::Upgrade {
                version: *version,
                module_hash: *module_hash,
            };
        }
        Intent::LockCode => w.code = CodeAcceptance::Lock,
        Intent::ReleaseEvmNonce { address, chain_id } => {
            w.evm_pending
                .retain(|e| !(e.address == *address && e.chain_id == *chain_id));
        }
        _ => return Err(invalid("not an internal intent")),
    }
    Ok(Outcome::Applied)
}

/// Check the cosign policy of `address` over `digests` of `plan`.
fn check_cosigs(
    w: &mut Wallet,
    address: &Address,
    proposal: u64,
    plan: u32,
    digests: &[Hash],
    cosigs: &[CosignerSig],
) -> Result<()> {
    let Some(policy) = &address.cosign else {
        return Ok(());
    };
    let me = ic_cdk::api::canister_self();
    let mut satisfied = 0;
    for id in &policy.cosigners {
        let Ok(c) = w.cosigner(*id) else {
            continue;
        };
        if w.policy.hardware_cosigners && !c.input.hardware {
            continue;
        }
        let mut c = c.clone();
        let mut complete = true;
        for (i, d) in digests.iter().enumerate() {
            let message = cosign_message(me, proposal, plan, i as u32, d);
            match cosigs
                .iter()
                .find(|s| s.cosigner == *id && s.digest_index == i as u32)
            {
                Some(sig) => c.counter = verify_cosign(&c, &message, sig)?,
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if complete {
            satisfied += 1;
            if let Some(stored) = w.cosigners.iter_mut().find(|x| x.id == *id) {
                stored.counter = c.counter;
            }
        }
    }
    ensure(satisfied >= policy.threshold, Error::Forbidden)
}

/// Native Taproot cosigner signatures over each input's sighash.
fn native_cosigs(
    w: &Wallet,
    record: &PlanRecord,
    cosigs: &[CosignerSig],
) -> Result<Vec<Option<Vec<u8>>>> {
    let Assembly::Bitcoin { spends, .. } = &record.assembly else {
        return Ok(vec![]);
    };
    spends
        .iter()
        .enumerate()
        .map(|(i, s)| match s {
            BtcSpend::Script {
                cosigner: Some(id), ..
            } => {
                let c = w.cosigner(*id)?;
                let sig = cosigs
                    .iter()
                    .find(|s| s.cosigner == *id && s.digest_index == i as u32)
                    .ok_or(Error::Forbidden)?;
                verify_cosign(c, &record.plan.digests[i], sig)?;
                Ok(Some(sig.signature.to_vec()))
            }
            _ => Ok(None),
        })
        .collect()
}

fn source(intent: &Intent) -> Option<u32> {
    match intent {
        Intent::Transfer { from, .. }
        | Intent::Batch { from, .. }
        | Intent::Refuel { from, .. } => Some(*from),
        _ => chains::chain_source(intent).map(|(a, _)| a),
    }
}

fn result(p: Proposal, signed: Option<SignedTx>) -> ExecuteResult {
    ExecuteResult {
        proposal: p,
        signed,
    }
}

#[ic_cdk::update]
async fn execute(args: ExecuteArgs) -> Result<ExecuteResult> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(args.proposal)?;
    let member = w.acting(caller)?.clone();
    ensure(member.role != Role::Viewer, Error::Forbidden)?;
    if close(&mut p, &w, at) {
        save_closed(&mut w, &p, at);
        store::save(w.clone());
    }
    ensure(
        !w.frozen || matches!(p.intent, Intent::Unfreeze) || is_membership(&p.intent),
        Error::Locked,
    )?;
    if chains::chain_source(&p.intent).is_some() {
        let seq = args.plan.ok_or_else(|| invalid("plan"))?;
        return sign_plan(w, p, seq, &args.cosigs, member.id, at).await;
    }
    match p.status {
        ProposalStatus::Ready => {}
        ProposalStatus::Executing => return resume(p.id).await,
        ProposalStatus::Settled => return Ok(result(p, None)),
        _ => return Err(Error::VersionConflict),
    }
    ensure(at < p.expires_at_ms, Error::Expired)?;
    if let Some(from) = source(&p.intent) {
        let address = w.address(from)?.clone();
        check_cosigs(&mut w, &address, p.id, 0, &[p.digest], &args.cosigs)?;
    }
    match p.intent.clone() {
        intent if internal(&intent) => {
            apply_ready(&mut w, &mut p, at);
            store::save_proposal(&p);
            store::save(w);
            Ok(result(p, None))
        }
        Intent::Transfer { .. } | Intent::Batch { .. } => start_ledger(w, p, at).await,
        Intent::CanisterCall {
            canister,
            method,
            arg,
            cycles,
            ..
        } => call(w, p, canister, method, arg.into_vec(), cycles, at).await,
        Intent::Refuel { from, icp_e8s } => {
            let job = TopUp {
                from: icp_subaccount(from).map(Hash::new),
                amount: icp_e8s,
                created_at_time: ic_cdk::api::time(),
                block: None,
                cycles: None,
            };
            begin(&mut w, &mut p, Job::TopUp(job), at);
            top_up(p.id).await
        }
        Intent::AddAddress {
            chain,
            kind,
            label,
            cosign,
        } => add_address(w, p, chain, kind, label, cosign, at).await,
        _ => Err(invalid("intent")),
    }
}

/// Mark a proposal executing under `job` and persist both.
fn begin(w: &mut Wallet, p: &mut Proposal, job: Job, at: u64) {
    p.status = ProposalStatus::Executing;
    p.updated_at_ms = at;
    store::audit(w, at, None, Some(p.id), AuditKind::Executing);
    store::save_job(p.id, &job);
    store::save_proposal(p);
    store::save(w.clone());
}

/// Undo `begin` after a call that proved nothing happened.
fn abandon(id: u64, status: ProposalStatus) {
    if let Ok(mut p) = store::proposal(id) {
        p.status = status;
        p.updated_at_ms = now();
        store::save_proposal(&p);
    }
    store::remove_job(id);
}

async fn start_ledger(mut w: Wallet, mut p: Proposal, at: u64) -> Result<ExecuteResult> {
    let (from, asset, outputs, max_fee, memo) = match &p.intent {
        Intent::Transfer {
            from,
            asset,
            to,
            amount,
            max_fee,
            memo,
        } => (
            *from,
            asset.clone(),
            vec![(to.clone(), *amount)],
            *max_fee,
            memo.clone(),
        ),
        Intent::Batch {
            from,
            asset,
            items,
            max_fee,
        } => (
            *from,
            asset.clone(),
            items.iter().map(|i| (i.to.clone(), i.amount)).collect(),
            *max_fee,
            None,
        ),
        _ => unreachable!("transfer"),
    };
    let ledger = match asset {
        AssetKey::Icp => w.settings.icp_ledger,
        AssetKey::Icrc(ledger) => ledger,
        _ => unreachable!("ledger asset"),
    };
    let items = outputs
        .into_iter()
        .enumerate()
        .map(|(i, (to, amount))| {
            let Destination::Icrc(to) = to else {
                unreachable!("validated destination")
            };
            // Distinct memos keep equal batch items apart in deduplication.
            let memo = memo.clone().unwrap_or_else(|| {
                Sha256::digest([&p.digest[..], &(i as u32).to_be_bytes()].concat())
                    .to_vec()
                    .into()
            });
            LedgerItem {
                to,
                amount,
                memo: Some(memo),
                block: None,
            }
        })
        .collect::<Vec<_>>();
    let count = items.len() as u128;
    let job = LedgerJob {
        ledger,
        from: icp_subaccount(from).map(Hash::new),
        fee: None,
        created_at_time: None,
        items,
    };
    begin(&mut w, &mut p, Job::Ledger(job), at);
    let fee = ledger::fee(ledger).await;
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(p.id)?;
    let Some(Job::Ledger(mut job)) = store::job(p.id) else {
        return Err(Error::Pending);
    };
    let fee = match fee {
        Ok(fee) => fee,
        Err(e) => {
            abandon(p.id, ProposalStatus::Ready);
            return Err(e);
        }
    };
    if fee.saturating_mul(count) > max_fee {
        fail(&mut w, &mut p, "FeeBlocked".into(), true, at);
        store::remove_job(p.id);
        store::save_proposal(&p);
        store::save(w);
        return Err(Error::FeeBlocked);
    }
    job.fee = Some(fee);
    job.created_at_time = Some(ic_cdk::api::time());
    store::save_job(p.id, &Job::Ledger(job));
    process_ledger(p.id).await
}

/// Send the remaining transfers of a ledger job in order.
async fn process_ledger(id: u64) -> Result<ExecuteResult> {
    loop {
        let Some(Job::Ledger(job)) = store::job(id) else {
            return Err(Error::NotFound);
        };
        ensure(job.created_at_time.is_some(), Error::Pending)?;
        let Some(i) = job.items.iter().position(|x| x.block.is_none()) else {
            let at = now();
            let mut w = store::wallet();
            let mut p = store::proposal(id)?;
            let blocks = job.items.iter().filter_map(|x| x.block).collect();
            settle(&mut w, &mut p, Outcome::Ledger { blocks }, at);
            store::remove_job(id);
            store::save_proposal(&p);
            store::save(w);
            return Ok(result(p, None));
        };
        // A freeze stops the batch before its next transfer.
        ensure(!store::with(|w| w.frozen), Error::Locked)?;
        let sent = ledger::send(job.ledger, job.arg(&job.items[i])).await;
        let at = now();
        let Some(Job::Ledger(mut job)) = store::job(id) else {
            return Err(Error::NotFound);
        };
        match sent {
            Sent::Block(block) => {
                job.items[i].block = Some(block);
                store::save_job(id, &Job::Ledger(job));
            }
            Sent::Refused(reason) => {
                let mut w = store::wallet();
                let mut p = store::proposal(id)?;
                let blocks: Vec<u64> = job.items.iter().filter_map(|x| x.block).collect();
                // A resend past the ledger's deduplication window proves nothing.
                let unknown = reason.starts_with("TooOld");
                let reason = if unknown {
                    format!("item {i}: outcome unknown after the deduplication window; check the ledger")
                } else {
                    format!("item {i}: {reason}")
                };
                let release = blocks.is_empty() && !unknown;
                p.outcome = Some(Outcome::Ledger { blocks });
                fail(&mut w, &mut p, reason.clone(), release, at);
                store::remove_job(id);
                store::save_proposal(&p);
                store::save(w);
                return Err(Error::InvalidInput(reason));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
}

async fn call(
    mut w: Wallet,
    mut p: Proposal,
    canister: Principal,
    method: String,
    arg: Vec<u8>,
    cycles: u128,
    at: u64,
) -> Result<ExecuteResult> {
    begin(&mut w, &mut p, Job::Call, at);
    let reply = Call::bounded_wait(canister, &method)
        .with_raw_args(&arg)
        .with_cycles(cycles)
        .await;
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(p.id)?;
    let outcome = match reply {
        Ok(response) => {
            let bytes = response.into_bytes();
            Outcome::Replied {
                reply_hash: Hash::new(Sha256::digest(&bytes).into()),
                reply: bytes[..bytes.len().min(MAX_CALL_BYTES)].to_vec().into(),
            }
        }
        Err(CallFailed::CallRejected(r)) if r.reject_code() == Ok(RejectCode::SysUnknown) => {
            Outcome::CallUnknown
        }
        Err(CallFailed::CallRejected(r)) => Outcome::CallRejected {
            code: r.raw_reject_code(),
            message: r.reject_message().chars().take(1_024).collect(),
        },
        Err(e) => {
            abandon(p.id, ProposalStatus::Ready);
            return Err(Error::Unavailable(e.to_string()));
        }
    };
    store::remove_job(p.id);
    if let Outcome::CallRejected { message, .. } = &outcome {
        let reason = message.clone();
        p.outcome = Some(outcome);
        fail(&mut w, &mut p, reason, true, at);
    } else {
        settle(&mut w, &mut p, outcome, at);
    }
    store::save_proposal(&p);
    store::save(w);
    Ok(result(p, None))
}

/// Send the top-up transfer once, then notify the cycles minting canister.
async fn top_up(id: u64) -> Result<ExecuteResult> {
    let (cmc, ledger) = store::with(|w| (w.settings.cmc, w.settings.icp_ledger));
    let me = ic_cdk::api::canister_self();
    let Some(Job::TopUp(mut job)) = store::job(id) else {
        return Err(Error::NotFound);
    };
    if job.block.is_none() {
        let sent = ledger::send(ledger, job.arg(cmc, me)).await;
        let at = now();
        match sent {
            Sent::Block(block) => {
                job.block = Some(block);
                store::save_job(id, &Job::TopUp(job.clone()));
            }
            Sent::Refused(reason) => {
                let mut w = store::wallet();
                let mut p = store::proposal(id)?;
                fail(&mut w, &mut p, reason.clone(), true, at);
                store::remove_job(id);
                store::save_proposal(&p);
                store::save(w);
                return Err(Error::InvalidInput(reason));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
    let block = job.block.expect("sent");
    let notified = ledger::notify_top_up(cmc, block, me).await;
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(id)?;
    match notified {
        Notified::Cycles(cycles) => {
            store::audit(&mut w, at, None, Some(id), AuditKind::Refueled { block });
            settle(&mut w, &mut p, Outcome::Refueled { block, cycles }, at);
        }
        Notified::Refunded(reason) => fail(&mut w, &mut p, reason, true, at),
        Notified::Pending => return Err(Error::Pending),
    }
    store::remove_job(id);
    store::save_proposal(&p);
    store::save(w);
    Ok(result(p, None))
}

#[allow(clippy::too_many_arguments)]
async fn add_address(
    mut w: Wallet,
    mut p: Proposal,
    chain: Chain,
    kind: AddressKind,
    label: String,
    cosign: Option<CosignPolicy>,
    at: u64,
) -> Result<ExecuteResult> {
    check_state(&w, &p.intent)?;
    let me = ic_cdk::api::canister_self();
    let id = w.next_address;
    w.next_address += 1;
    store::save(w.clone());
    let algorithm = key_algorithm(chain, &kind)?;
    let key = match algorithm {
        None => vec![],
        Some(algorithm) => {
            begin(&mut w, &mut p, Job::Address, at);
            let key = crate::keys::public_key(
                &w.settings.key_name,
                algorithm,
                derivation_path(chain, &kind, id)?,
            )
            .await;
            match key {
                Ok((key, _)) => key,
                Err(e) => {
                    abandon(p.id, ProposalStatus::Ready);
                    return Err(e);
                }
            }
        }
    };
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(p.id)?;
    let cosigner = match &kind {
        AddressKind::Taproot(control) => Some(chains::btc::native_cosigner(&w, control)?),
        _ => None,
    };
    let text = address_text(
        me,
        w.settings.btc_network,
        id,
        chain,
        &kind,
        &key,
        cosigner.as_ref(),
    )?;
    w.addresses.push(Address {
        id,
        chain,
        kind,
        public_key: key.into(),
        text,
        label,
        cosign,
        sol_nonce: None,
        created_at_ms: at,
    });
    settle(&mut w, &mut p, Outcome::AddressCreated(id), at);
    store::remove_job(p.id);
    store::save_proposal(&p);
    store::save(w);
    Ok(result(p, None))
}

/// Resume a proposal whose execution was interrupted.
async fn resume(id: u64) -> Result<ExecuteResult> {
    match store::job(id) {
        Some(Job::Ledger(_)) => process_ledger(id).await,
        Some(Job::TopUp(_)) => top_up(id).await,
        Some(_) => Err(Error::Pending),
        None => Err(Error::NotFound),
    }
}

/// Resend the exact arguments of an interrupted transfer or top-up.
#[ic_cdk::update]
async fn reconcile(id: u64) -> Result<ExecuteResult> {
    let caller = ic_cdk::api::msg_caller();
    store::with(|w| w.acting(caller).map(|_| ()))?;
    resume(id).await
}

fn waiting_status(p: &Proposal) -> ProposalStatus {
    if p.signed.is_empty() {
        ProposalStatus::Ready
    } else if p.broadcasts.is_empty() {
        ProposalStatus::Signed
    } else {
        ProposalStatus::Broadcast
    }
}

/// What a plan must share with the proposal's first signed plan and with
/// the address's other transactions, so at most one version of a payment
/// can land. Checked when a plan is built and again when it is signed: plans
/// built before either is signed are checked against whichever signs first.
/// Returns the unsettled nonce an EVM cancellation replaces.
fn check_versions(
    w: &Wallet,
    p: &Proposal,
    address: u32,
    params: &ExecutionParams,
    at: u64,
) -> Result<Option<u64>> {
    let first = match p.signed.first() {
        Some(s) => Some(store::plan(p.id, s.plan)?.plan.params),
        None => None,
    };
    match params {
        ExecutionParams::Evm { nonce, .. } => {
            // A re-sign keeps the nonce, so only one version can be mined.
            if let Some(first) = &first {
                ensure(
                    matches!(first, ExecutionParams::Evm { nonce: n, .. } if n == nonce),
                    Error::Forbidden,
                )?;
            }
            let (chain_id, cancel) = match &p.intent {
                Intent::Transfer {
                    asset: AssetKey::Evm { chain_id, .. },
                    ..
                } => (*chain_id, false),
                Intent::CancelEvmNonce { chain_id, .. } => (*chain_id, true),
                _ => return Err(invalid("evm intent")),
            };
            let pending = w
                .evm_pending
                .iter()
                .find(|e| e.address == address && e.chain_id == chain_id);
            if cancel {
                return Ok(pending.filter(|e| e.proposal != p.id).map(|e| e.nonce));
            }
            // One unsettled transaction per address and chain.
            if let Some(e) = pending {
                ensure(e.proposal == p.id && e.nonce == *nonce, Error::Forbidden)?;
            }
        }
        ExecutionParams::Bitcoin { utxos, .. } => {
            if let Some(first) = &first {
                let ExecutionParams::Bitcoin { utxos: signed, .. } = first else {
                    return Err(Error::IntegrityFailed);
                };
                // A replacement spends exactly the first plan's inputs at
                // the same values.
                let inputs = |u: &[Utxo]| {
                    let mut v: Vec<(Hash, u32, u64)> =
                        u.iter().map(|u| (u.txid, u.vout, u.value)).collect();
                    v.sort();
                    v
                };
                ensure(inputs(signed) == inputs(utxos), Error::Forbidden)?;
            }
        }
        ExecutionParams::Solana { blockhash, .. } => {
            if w.address(address)?.sol_nonce.is_some() {
                match &first {
                    // A nonce value lands once, so every version shares it.
                    Some(ExecutionParams::Solana { blockhash: b, .. }) => {
                        ensure(b == blockhash, Error::Forbidden)?;
                    }
                    Some(_) => return Err(Error::IntegrityFailed),
                    // A new payment needs a value the address did not sign with last.
                    None => ensure(
                        w.sol_last
                            .iter()
                            .all(|s| s.address != address || s.blockhash != *blockhash),
                        Error::Forbidden,
                    )?,
                }
            } else if let Some(last) = p.signed.last() {
                // A blockhash dies after 150 blocks; sign again only after that.
                ensure(at >= last.signed_at_ms + 150 * SECOND, Error::Forbidden)?;
            }
        }
    }
    Ok(None)
}

/// Build a plan from the executor's chain parameters. Nothing is signed.
#[ic_cdk::update]
fn prepare_execution(id: u64, params: ExecutionParams) -> Result<ExecutionPlan> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(id)?;
    ensure(w.acting(caller)?.role != Role::Viewer, Error::Forbidden)?;
    if close(&mut p, &w, at) {
        save_closed(&mut w, &p, at);
        store::save(w);
        return Err(Error::Expired);
    }
    let (address, _) =
        chains::chain_source(&p.intent).ok_or_else(|| invalid("not an external-chain intent"))?;
    ensure(!w.frozen, Error::Locked)?;
    ensure(
        matches!(
            p.status,
            ProposalStatus::Ready | ProposalStatus::Signed | ProposalStatus::Broadcast
        ),
        Error::VersionConflict,
    )?;
    ensure(
        p.status != ProposalStatus::Ready || at < p.expires_at_ms,
        Error::Expired,
    )?;
    ensure(p.plans < MAX_PLANS, Error::QuotaExceeded)?;
    let pending_nonce = check_versions(&w, &p, address, &params, at)?;
    let built = chains::build(&w, &p.intent, &params, pending_nonce)?;
    let seq = p.plans + 1;
    let plan = ExecutionPlan {
        proposal: id,
        seq,
        params,
        unsigned: built.unsigned.into(),
        digests: built.digests,
        summary: built.summary,
        created_at_ms: at,
        expires_at_ms: at + PLAN_TTL_MS,
    };
    store::save_plan(&PlanRecord {
        plan: plan.clone(),
        signs: built.signs,
        assembly: built.assembly,
    });
    p.plans = seq;
    p.updated_at_ms = at;
    store::save_proposal(&p);
    Ok(plan)
}

async fn sign_plan(
    mut w: Wallet,
    mut p: Proposal,
    seq: u32,
    cosigs: &[CosignerSig],
    member: MemberId,
    at: u64,
) -> Result<ExecuteResult> {
    if let Some(signed) = p.signed.iter().find(|s| s.plan == seq) {
        let signed = signed.clone();
        return Ok(result(p, Some(signed)));
    }
    ensure(
        matches!(
            p.status,
            ProposalStatus::Ready | ProposalStatus::Signed | ProposalStatus::Broadcast
        ),
        if p.status == ProposalStatus::Executing {
            Error::Pending
        } else {
            Error::VersionConflict
        },
    )?;
    let record = store::plan(p.id, seq)?;
    ensure(at < record.plan.expires_at_ms, Error::Expired)?;
    let (address, _) = chains::chain_source(&p.intent).expect("chain intent");
    // Another plan may have been signed since this one was built.
    check_versions(&w, &p, address, &record.plan.params, at)?;
    let address = w.address(address)?.clone();
    let digests = &record.plan.digests;
    w.roll_day(at);
    ensure(
        w.signatures_today as usize + record.signs.len() <= w.policy.daily_signatures as usize,
        Error::QuotaExceeded,
    )?;
    check_cosigs(&mut w, &address, p.id, seq, digests, cosigs)?;
    let native = native_cosigs(&w, &record, cosigs)?;
    let back = waiting_status(&p);
    begin(
        &mut w,
        &mut p,
        Job::Signing {
            plan: seq,
            signatures: vec![],
        },
        at,
    );
    let key_name = w.settings.key_name.clone();
    let mut signatures = vec![];
    for (i, spec) in record.signs.iter().enumerate() {
        let message = match spec {
            SignSpec::Ed25519 { .. } => record.plan.unsigned.to_vec(),
            _ => digests[i].to_vec(),
        };
        let sig = crate::keys::sign(&key_name, spec, &message).await;
        // A freeze during signing stops the remaining signatures.
        let frozen = store::with(|w| w.frozen);
        match sig {
            Ok(sig) if !frozen => {
                signatures.push(sig);
                let mut w = store::wallet();
                w.roll_day(now());
                w.signatures_today += 1;
                store::save(w);
            }
            Ok(_) => {
                abandon(p.id, back);
                return Err(Error::Locked);
            }
            Err(e) => {
                abandon(p.id, back);
                return Err(e);
            }
        }
    }
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(p.id)?;
    let (raw, txid) = chains::assemble(&record.assembly, &signatures, &native)?;
    let signed = SignedTx {
        plan: seq,
        raw: raw.into(),
        txid: txid.clone(),
        signed_at_ms: at,
    };
    p.signed.push(signed.clone());
    p.status = ProposalStatus::Signed;
    p.updated_at_ms = at;
    match (&record.plan.params, &p.intent) {
        (ExecutionParams::Evm { nonce, .. }, intent) => {
            let chain_id = match intent {
                Intent::Transfer {
                    asset: AssetKey::Evm { chain_id, .. },
                    ..
                }
                | Intent::CancelEvmNonce { chain_id, .. } => *chain_id,
                _ => unreachable!("evm intent"),
            };
            if !matches!(intent, Intent::CancelEvmNonce { .. }) {
                w.evm_pending
                    .retain(|e| !(e.address == address.id && e.chain_id == chain_id));
                w.evm_pending.push(EvmPending {
                    address: address.id,
                    chain_id,
                    nonce: *nonce,
                    proposal: p.id,
                });
            }
        }
        (ExecutionParams::Solana { blockhash, .. }, _) => {
            w.sol_last.retain(|s| s.address != address.id);
            w.sol_last.push(SolLast {
                address: address.id,
                blockhash: *blockhash,
            });
        }
        _ => {}
    }
    store::audit(
        &mut w,
        at,
        Some(member),
        Some(p.id),
        AuditKind::Signed { plan: seq, txid },
    );
    store::remove_job(p.id);
    store::save_proposal(&p);
    store::save(w);
    Ok(result(p, Some(signed)))
}

/// Record a member's report of a signed transaction.
#[ic_cdk::update]
fn report(args: ChainReport) -> Result<Proposal> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(args.proposal)?;
    let member = w.acting(caller)?.clone();
    ensure(member.role != Role::Viewer, Error::Forbidden)?;
    ensure_valid(args.note.len() <= 256, "note")?;
    ensure(
        matches!(p.status, ProposalStatus::Signed | ProposalStatus::Broadcast),
        Error::VersionConflict,
    )?;
    let txid = p
        .signed
        .iter()
        .find(|s| s.plan == args.plan)
        .ok_or(Error::NotFound)?
        .txid
        .clone();
    p.broadcasts.push(BroadcastReport {
        plan: args.plan,
        outcome: args.outcome,
        txid: txid.clone(),
        member: member.id.clone(),
        at_ms: at,
        note: args.note,
    });
    p.updated_at_ms = at;
    match args.outcome {
        ChainOutcome::Broadcast => {
            p.status = ProposalStatus::Broadcast;
            store::audit(
                &mut w,
                at,
                Some(member.id),
                Some(p.id),
                AuditKind::Broadcast {
                    plan: args.plan,
                    txid,
                },
            );
        }
        ChainOutcome::Settled => {
            w.evm_pending.retain(|e| e.proposal != p.id);
            match &p.intent {
                Intent::InitSolNonce { address, .. } => {
                    if let Some(a) = w.addresses.iter_mut().find(|a| a.id == *address) {
                        let key: [u8; 32] = a
                            .public_key
                            .as_slice()
                            .try_into()
                            .map_err(|_| Error::IntegrityFailed)?;
                        a.sol_nonce = Some(Hash::new(chains::sol::nonce_account(&key)));
                    }
                }
                Intent::CancelEvmNonce {
                    address, chain_id, ..
                } => {
                    // The replaced transaction can no longer land.
                    if let Some(e) = w
                        .evm_pending
                        .iter()
                        .find(|e| e.address == *address && e.chain_id == *chain_id)
                        .cloned()
                    {
                        w.evm_pending.retain(|x| *x != e);
                        if let Ok(mut replaced) = store::proposal(e.proposal) {
                            fail(
                                &mut w,
                                &mut replaced,
                                format!("nonce replaced by proposal {}", p.id),
                                true,
                                at,
                            );
                            store::save_proposal(&replaced);
                        }
                    }
                }
                _ => {}
            }
            settle(
                &mut w,
                &mut p,
                Outcome::ChainSettled {
                    plan: args.plan,
                    txid,
                },
                at,
            );
        }
        ChainOutcome::Failed => {
            w.evm_pending.retain(|e| e.proposal != p.id);
            // The signed transaction may still land, so its spend keeps counting.
            fail(
                &mut w,
                &mut p,
                format!("reported failed by {:?}", member.id),
                false,
                at,
            );
        }
    }
    store::save_proposal(&p);
    store::save(w);
    Ok(p)
}

/// Top up cycles under the policy's refuel rule; anyone may call it.
#[ic_cdk::update]
async fn refuel() -> Result<Option<u128>> {
    let at = now();
    let mut w = store::wallet();
    w.roll_day(at);
    let me = ic_cdk::api::canister_self();
    if w.refuel.is_none() {
        let Some(rule) = w.policy.refuel.clone() else {
            return Ok(None);
        };
        if ic_cdk::api::canister_cycle_balance() >= rule.min_cycles {
            return Ok(None);
        }
        ensure(w.refuels_today < rule.max_per_day, Error::QuotaExceeded)?;
        w.refuels_today += 1;
        w.refuel = Some(TopUp {
            from: icp_subaccount(rule.from).map(Hash::new),
            amount: rule.icp_e8s,
            created_at_time: ic_cdk::api::time(),
            block: None,
            cycles: None,
        });
        store::save(w);
    }
    let (cmc, ledger, mut job) = store::with(|w| {
        (
            w.settings.cmc,
            w.settings.icp_ledger,
            w.refuel.clone().expect("job"),
        )
    });
    if job.block.is_none() {
        let sent = ledger::send(ledger, job.arg(cmc, me)).await;
        let mut w = store::wallet();
        match sent {
            Sent::Block(block) => {
                job.block = Some(block);
                w.refuel = Some(job.clone());
                store::save(w);
            }
            Sent::Refused(reason) => {
                w.refuel = None;
                store::save(w);
                return Err(Error::InvalidInput(reason));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
    let block = job.block.expect("sent");
    let notified = ledger::notify_top_up(cmc, block, me).await;
    let at = now();
    let mut w = store::wallet();
    match notified {
        Notified::Cycles(cycles) => {
            w.refuel = None;
            store::audit(&mut w, at, None, None, AuditKind::Refueled { block });
            store::save(w);
            Ok(Some(cycles))
        }
        Notified::Refunded(reason) => {
            w.refuel = None;
            store::save(w);
            Err(Error::InvalidInput(reason))
        }
        Notified::Pending => Err(Error::Pending),
    }
}
