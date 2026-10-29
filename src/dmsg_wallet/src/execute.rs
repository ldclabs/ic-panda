//! Executions: internal changes, ICRC transfers, canister calls, cycles
//! top-ups, new addresses and external-chain signing.
use crate::api::{check_state, execution_authorized, now, refresh, unlocked};
use crate::chains::{self, Assembly, btc::BtcSpend};
use crate::ledger::{self, LedgerItem, LedgerJob, Notified, Sent, TopUp};
use crate::store::{self, EvmPending, Job, PlanRecord, REFUEL_JOB, SignSpec, SolLast, Wallet};
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
            | Intent::ReplaceCosigner { .. }
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

/// Persist a proposal whose execution step ended, with the wallet, and
/// clear its job.
fn finish(w: Wallet, p: &Proposal) {
    store::remove_job(p.id);
    store::save_proposal(p);
    store::save(w);
}

/// Apply an internal intent the moment it is Ready.
pub(crate) fn apply_ready(w: &mut Wallet, p: &mut Proposal, at: u64) {
    if !internal(&p.intent) {
        return;
    }
    match check_state(w, &p.intent).and_then(|()| apply(w, p.id, &p.intent, at)) {
        Ok(outcome) => settle(w, p, outcome, at),
        Err(e) => fail(w, p, e.to_string(), true, at),
    }
}

/// References are rejected by check_state; removal never weakens a policy.
fn strip_cosigners(w: &mut Wallet, removed: &[u32]) {
    w.cosigners.retain(|c| !removed.contains(&c.id));
}

fn apply(w: &mut Wallet, proposal: u64, intent: &Intent, at: u64) -> Result<Outcome> {
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
        Intent::ReplaceCosigner { id, input } => {
            let c = w
                .cosigners
                .iter_mut()
                .find(|c| c.id == *id)
                .ok_or(Error::NotFound)?;
            let kind = AuditKind::CosignerRotated {
                id: *id,
                old_key: cosigner_key(&c.input.public_key),
                new_key: cosigner_key(&input.public_key),
            };
            c.input = input.clone();
            c.counter = 0;
            store::audit(w, at, None, Some(proposal), kind);
        }
        Intent::AddMember(input) => {
            w.members.push(Member {
                id: input.id.clone(),
                role: input.role,
                label: input.label.clone(),
                agent_ids: input.agent_ids.clone(),
                status: MemberStatus::Invited,
                bindings: vec![],
                added_at_ms: at,
            });
            w.policy.version += 1;
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
            w.policy.version += 1;
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
            w.policy.version += 1;
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
        Intent::ReleaseEvmNonce {
            address,
            chain_id,
            proposal,
            nonce,
        } => {
            w.evm_pending.retain(|e| {
                !(e.address == *address
                    && e.chain_id == *chain_id
                    && e.proposal == *proposal
                    && e.nonce == *nonce)
            });
        }
        _ => return Err(invalid("not an internal intent")),
    }
    Ok(Outcome::Applied)
}

/// Check the cosign policy of `address` over `digests` of `plan`.
fn check_cosigs(
    w: &mut Wallet,
    me: Principal,
    requirements: &[CosignRequirement],
    proposal: u64,
    plan: u32,
    digests: &[Hash],
    cosigs: &[CosignerSig],
) -> Result<()> {
    if requirements.is_empty() {
        return Ok(());
    }
    let keys: Vec<_> = w
        .cosigners
        .iter()
        .filter(|c| {
            (!w.policy.hardware_cosigners || c.input.hardware)
                && requirements
                    .iter()
                    .any(|r| r.policy.cosigners.contains(&c.id))
        })
        .cloned()
        .collect();
    let messages: Vec<_> = digests
        .iter()
        .enumerate()
        .map(|(i, d)| cosign_message(me, proposal, plan, i as u32, d))
        .collect();
    let complete = verify_cosignatures(&keys, &messages, cosigs)?;
    for r in requirements {
        ensure(
            complete
                .iter()
                .filter(|(id, _)| r.policy.cosigners.contains(id))
                .count()
                >= usize::from(r.policy.threshold),
            Error::Forbidden,
        )?;
    }
    for (id, counter) in complete {
        if let Some(stored) = w.cosigners.iter_mut().find(|c| c.id == id) {
            stored.counter = counter;
        }
    }
    Ok(())
}

/// Native Taproot cosigner signatures over each input's sighash.
fn native_cosigs(
    w: &Wallet,
    record: &PlanRecord,
    cosigs: &[CosignerSig],
) -> Result<Vec<Option<Vec<u8>>>> {
    let Assembly::Bitcoin {
        spend: BtcSpend::Script {
            cosigner: Some(id), ..
        },
        ..
    } = &record.assembly
    else {
        return Ok(vec![]);
    };
    let c = w.cosigner(*id)?;
    record
        .plan
        .digests
        .iter()
        .enumerate()
        .map(|(i, digest)| {
            let sig = cosigs
                .iter()
                .find(|s| s.cosigner == *id && s.digest_index == i as u32)
                .ok_or(Error::Forbidden)?;
            verify_cosign(c, digest, sig)?;
            Ok(Some(sig.signature.to_vec()))
        })
        .collect()
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
    let me = ic_cdk::api::canister_self();
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(args.proposal)?;
    let member = w.acting(caller)?.clone();
    ensure(member.role != Role::Viewer, Error::Forbidden)?;
    refresh(&mut w, &mut p, at)?;
    unlocked(&w, &p.intent)?;
    if p.status == ProposalStatus::Ready {
        execution_authorized(&w, &p, at)?;
    }
    if chains::chain_source(&p.intent).is_some() {
        let seq = args.plan.ok_or_else(|| invalid("plan"))?;
        return sign_plan(w, p, seq, &args.cosigs, member.id, me, at).await;
    }
    match p.status {
        ProposalStatus::Ready => {}
        ProposalStatus::Executing => return resume(p.id, me, at).await,
        ProposalStatus::Settled => return Ok(result(p, None)),
        _ => return Err(Error::VersionConflict),
    }
    let requirements = cosign_requirements(&p.intent, &w.addresses);
    check_cosigs(
        &mut w,
        me,
        &requirements,
        p.id,
        0,
        &[p.digest],
        &args.cosigs,
    )?;
    match p.intent.clone() {
        intent if internal(&intent) => {
            apply_ready(&mut w, &mut p, at);
            finish(w, &p);
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
                created_at_time: at * NANOS_PER_MILLISECOND,
                block: None,
            };
            w.commit_spend(p.id, at);
            begin(&mut w, &mut p, Job::TopUp(job), at);
            top_up(p.id, me).await
        }
        Intent::AddAddress { .. } => add_address(w, p, me, at).await,
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
fn abandon(p: &mut Proposal, status: ProposalStatus, at: u64) {
    p.status = status;
    p.updated_at_ms = at;
    if p.signed.is_empty() {
        let mut w = store::wallet();
        if let Some(spend) = w.spends.iter_mut().find(|s| s.proposal == p.id) {
            spend.executed_at_ms = None;
            store::save(w);
        }
    }
    store::save_proposal(p);
    store::remove_job(p.id);
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
                unsent: true,
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
            abandon(&mut p, ProposalStatus::Ready, at);
            return Err(e);
        }
    };
    if fee.saturating_mul(count) > max_fee {
        fail(&mut w, &mut p, "FeeBlocked".into(), true, at);
        finish(w, &p);
        return Err(Error::FeeBlocked);
    }
    job.fee = Some(fee);
    job.created_at_time = Some(at * NANOS_PER_MILLISECOND);
    // No transfer has been sent yet; changed authority cannot authorize one.
    if let Err(error) = execution_authorized(&w, &p, at) {
        if error == Error::Locked {
            abandon(&mut p, ProposalStatus::Ready, at);
            return Err(error);
        }
        fail(
            &mut w,
            &mut p,
            "authorization changed before transfer".into(),
            true,
            at,
        );
        finish(w, &p);
        return Err(error);
    }
    store::save_job(p.id, &Job::Ledger(job));
    process_ledger(p.id, at).await
}

/// Send the remaining transfers of a ledger job in order.
async fn process_ledger(id: u64, mut at: u64) -> Result<ExecuteResult> {
    loop {
        let Some(Job::Ledger(mut job)) = store::job(id) else {
            return Err(Error::NotFound);
        };
        ensure(job.created_at_time.is_some(), Error::Pending)?;
        let Some(i) = job.items.iter().position(|x| x.block.is_none()) else {
            let mut w = store::wallet();
            let mut p = store::proposal(id)?;
            let blocks = job.items.iter().filter_map(|x| x.block).collect();
            settle(&mut w, &mut p, Outcome::Ledger { blocks }, at);
            finish(w, &p);
            return Ok(result(p, None));
        };
        // A freeze stops the batch before its next transfer.
        ensure(!store::with(|w| w.frozen), Error::Locked)?;
        let mut w = store::wallet();
        w.commit_spend(id, at);
        store::save(w);
        let first_attempt = job.items[i].unsent;
        if first_attempt {
            job.items[i].unsent = false;
            store::save_job(id, &Job::Ledger(job.clone()));
        }
        let sent = ledger::send(job.ledger, job.arg(&job.items[i])).await;
        at = now();
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
                // A first attempt rejected as TooOld never moved funds. Only
                // retries can hide a payment outside the deduplication window.
                let unknown = !first_attempt && reason.starts_with("TooOld");
                if unknown {
                    p.failure = Some(format!(
                        "item {i}: outcome unknown after the deduplication window; check the ledger"
                    ));
                    p.updated_at_ms = at;
                    store::save_proposal(&p);
                    return Err(Error::ExecutionUnknown);
                }
                let reason = format!("item {i}: {reason}");
                let release = blocks.is_empty();
                p.outcome = Some(Outcome::Ledger { blocks });
                fail(&mut w, &mut p, reason.clone(), release, at);
                finish(w, &p);
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
            abandon(&mut p, ProposalStatus::Ready, at);
            return Err(Error::Unavailable(e.to_string()));
        }
    };
    if let Outcome::CallRejected { message, .. } = &outcome {
        let reason = message.clone();
        p.outcome = Some(outcome);
        fail(&mut w, &mut p, reason, true, at);
    } else {
        settle(&mut w, &mut p, outcome, at);
    }
    finish(w, &p);
    Ok(result(p, None))
}

/// How a top-up ended.
enum TopUpEnd {
    /// Cycles minted from the ICP sent in `block`.
    Minted { block: u64, cycles: u128 },
    /// The ledger refused the transfer or the cycles minting canister
    /// returned the ICP.
    Refused(String),
}

/// Run the top-up job under `key`: send its transfer once, then notify the
/// cycles minting canister. An unknown or pending outcome keeps the job, so
/// a retry resends the same arguments; `None` means another call finished
/// the job meanwhile.
async fn run_top_up(key: u64, me: Principal) -> Result<Option<TopUpEnd>> {
    let (cmc, ledger) = store::with(|w| (w.settings.cmc, w.settings.icp_ledger));
    let Some(Job::TopUp(mut job)) = store::job(key) else {
        return Err(Error::NotFound);
    };
    if job.block.is_none() {
        let sent = ledger::send(ledger, job.arg(cmc, me)).await;
        let Some(Job::TopUp(stored)) = store::job(key) else {
            return Ok(None);
        };
        job = stored;
        match sent {
            Sent::Block(block) => {
                job.block = Some(block);
                store::save_job(key, &Job::TopUp(job.clone()));
            }
            Sent::Refused(reason) => {
                store::remove_job(key);
                return Ok(Some(TopUpEnd::Refused(reason)));
            }
            Sent::Unknown => return Err(Error::ExecutionUnknown),
        }
    }
    let block = job.block.expect("sent");
    let end = match ledger::notify_top_up(cmc, block, me).await {
        Notified::Cycles(cycles) => TopUpEnd::Minted { block, cycles },
        Notified::Refunded(reason) => TopUpEnd::Refused(reason),
        Notified::Pending => return Err(Error::Pending),
    };
    if store::job(key).is_none() {
        return Ok(None);
    }
    store::remove_job(key);
    Ok(Some(end))
}

/// The top-up of a Refuel proposal.
async fn top_up(id: u64, me: Principal) -> Result<ExecuteResult> {
    let end = run_top_up(id, me).await?;
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(id)?;
    match end {
        None => return Ok(result(p, None)),
        Some(TopUpEnd::Minted { block, cycles }) => {
            store::audit(&mut w, at, None, Some(id), AuditKind::Refueled { block });
            settle(&mut w, &mut p, Outcome::Refueled { block, cycles }, at);
        }
        Some(TopUpEnd::Refused(reason)) => {
            fail(&mut w, &mut p, reason.clone(), true, at);
            finish(w, &p);
            return Err(Error::InvalidInput(reason));
        }
    }
    finish(w, &p);
    Ok(result(p, None))
}

async fn add_address(
    mut w: Wallet,
    mut p: Proposal,
    me: Principal,
    at: u64,
) -> Result<ExecuteResult> {
    let Intent::AddAddress {
        chain,
        kind,
        label,
        cosign,
    } = p.intent.clone()
    else {
        unreachable!("address intent")
    };
    check_state(&w, &p.intent)?;
    let id = w.next_address;
    w.next_address += 1;
    let (key, at) = match key_algorithm(chain, &kind)? {
        None => (vec![], at),
        Some(algorithm) => {
            let path = derivation_path(chain, &kind, id)?;
            let key_name = w.settings.key_name.clone();
            begin(&mut w, &mut p, Job::Address, at);
            let key = crate::keys::public_key(&key_name, algorithm, path).await;
            let at = now();
            w = store::wallet();
            p = store::proposal(p.id)?;
            match key {
                Ok((key, _)) => (key, at),
                Err(e) => {
                    abandon(&mut p, ProposalStatus::Ready, at);
                    return Err(e);
                }
            }
        }
    };
    // A cosigner removed while the key was read fails the proposal.
    let text = ensure(
        p.policy_version == w.policy.version && at < p.expires_at_ms,
        Error::VersionConflict,
    )
    .and_then(|()| unlocked(&w, &p.intent))
    .and_then(|()| check_state(&w, &p.intent))
    .and_then(|()| match &kind {
        AddressKind::Taproot(control) => chains::btc::native_cosigner(&w, control).map(Some),
        _ => Ok(None),
    })
    .and_then(|cosigner| {
        address_text(
            me,
            w.settings.btc_network,
            id,
            chain,
            &kind,
            &key,
            cosigner.as_ref(),
        )
    });
    match text {
        Ok(text) => {
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
        }
        Err(e) => fail(&mut w, &mut p, e.to_string(), true, at),
    }
    finish(w, &p);
    Ok(result(p, None))
}

/// Resume a proposal whose execution was interrupted.
async fn resume(id: u64, me: Principal, at: u64) -> Result<ExecuteResult> {
    match store::job(id) {
        Some(Job::Ledger(_)) => process_ledger(id, at).await,
        Some(Job::TopUp(_)) => top_up(id, me).await,
        Some(_) => Err(Error::Pending),
        None => Err(Error::NotFound),
    }
}

/// Resend the exact arguments of an interrupted transfer or top-up.
#[ic_cdk::update]
async fn reconcile(id: u64) -> Result<ExecuteResult> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    let at = now();
    store::with(|w| {
        let member = w.acting(caller)?;
        ensure(member.role != Role::Viewer, Error::Forbidden)
    })?;
    ensure(
        store::proposal(id)?.status == ProposalStatus::Executing,
        Error::VersionConflict,
    )?;
    resume(id, me, at).await
}

/// Recover an unknown transfer from an authenticated ledger block. The caller
/// supplies only its index, never an assertion that money moved.
#[ic_cdk::update]
async fn reconcile_transfer(id: u64, item: u16, block: u64) -> Result<ExecuteResult> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    store::with(|w| ensure(w.acting(caller)?.role != Role::Viewer, Error::Forbidden))?;
    let Some(Job::Ledger(job)) = store::job(id) else {
        return Err(Error::NotFound);
    };
    ensure(job.created_at_time.is_some(), Error::Pending)?;
    let index = usize::from(item);
    let expected = job.arg(job.items.get(index).ok_or(Error::NotFound)?);
    let transfer = dmsg_runtime::ledger::read_transfer(job.ledger, block).await?;
    let at = now();
    store::with(|w| ensure(w.acting(caller)?.role != Role::Viewer, Error::Forbidden))?;
    ensure(
        transfer.from
            == icrc_ledger_types::icrc1::account::Account {
                owner: me,
                subaccount: expected.from_subaccount,
            }
            && transfer.to == expected.to
            && expected.amount == transfer.amount
            && transfer.fee == job.fee
            && transfer.created_at_time == expected.created_at_time
            && transfer.memo == expected.memo.as_ref().map(|m| m.0.to_vec())
            && transfer.spender.is_none(),
        Error::IntegrityFailed,
    )?;
    let Some(Job::Ledger(mut current)) = store::job(id) else {
        let p = store::proposal(id)?;
        ensure(
            matches!(&p.outcome, Some(Outcome::Ledger { blocks }) if blocks.get(index) == Some(&block)),
            Error::VersionConflict,
        )?;
        return Ok(result(p, None));
    };
    let stored = current.items.get(index).ok_or(Error::NotFound)?;
    ensure(
        current.ledger == job.ledger && current.arg(stored) == expected,
        Error::VersionConflict,
    )?;
    ensure(
        stored.block.is_none_or(|b| b == block),
        Error::IntegrityFailed,
    )?;
    current.items[index].block = Some(block);
    store::save_job(id, &Job::Ledger(current));
    let mut p = store::proposal(id)?;
    p.failure = None;
    store::save_proposal(&p);
    process_ledger(id, at).await
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
) -> Result<Option<u64>> {
    let first_record = match p.signed.first() {
        Some(s) => Some(store::plan(p.id, s.plan)?),
        None => None,
    };
    let first = first_record.as_ref().map(|r| &r.plan.params);
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
                let Intent::CancelEvmNonce {
                    proposal,
                    nonce: expected,
                    ..
                } = &p.intent
                else {
                    unreachable!()
                };
                ensure(
                    *nonce == *expected
                        && pending.is_some_and(|e| e.proposal == *proposal && e.nonce == *expected),
                    Error::VersionConflict,
                )?;
                return Ok(Some(*expected));
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
            let nonce = w.address(address)?.sol_nonce;
            if let Some(record) = &first_record {
                // A recent blockhash expiring does not undo an already mined
                // payment. Only the same durable nonce can authorize a replacement.
                ensure(
                    record.sol_nonce.is_some() && record.sol_nonce == nonce,
                    Error::Forbidden,
                )?;
            }
            if nonce.is_some() {
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
    refresh(&mut w, &mut p, at)?;
    let (address, _) =
        chains::chain_source(&p.intent).ok_or_else(|| invalid("not an external-chain intent"))?;
    unlocked(&w, &p.intent)?;
    ensure(
        matches!(
            p.status,
            ProposalStatus::Ready | ProposalStatus::Signed | ProposalStatus::Broadcast
        ),
        Error::VersionConflict,
    )?;
    // All devices collect signatures for the same live unsigned plan.
    if p.plans > 0 && !p.signed.iter().any(|s| s.plan == p.plans) {
        let existing = store::plan(id, p.plans)?.plan;
        if at < existing.expires_at_ms {
            return Ok(existing);
        }
    }
    ensure(p.plans < MAX_PLANS, Error::QuotaExceeded)?;
    let pending_nonce = check_versions(&w, &p, address, &params)?;
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
        sign: built.sign,
        assembly: built.assembly,
        sol_nonce: w.address(address)?.sol_nonce,
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
    me: Principal,
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
    check_versions(&w, &p, address, &record.plan.params)?;
    let address = w.address(address)?.clone();
    if address.chain == Chain::Solana {
        ensure(
            record.sol_nonce == address.sol_nonce,
            Error::VersionConflict,
        )?;
    }
    let digests = &record.plan.digests;
    let count = digests.len() as u32;
    w.roll_day(at);
    ensure(
        w.signatures_today.saturating_add(count) <= w.policy.daily_signatures,
        Error::QuotaExceeded,
    )?;
    let requirements = cosign_requirements(&p.intent, &w.addresses);
    check_cosigs(&mut w, me, &requirements, p.id, seq, digests, cosigs)?;
    let native = native_cosigs(&w, &record, cosigs)?;
    let back = waiting_status(&p);
    // Signatures count when signing starts, so concurrent signings share
    // the daily limit and a stopped signing still counts.
    w.signatures_today += count;
    let key_name = w.settings.key_name.clone();
    w.commit_spend(p.id, at);
    begin(&mut w, &mut p, Job::Signing, at);
    let mut signatures = vec![];
    for digest in digests {
        let message = match record.sign {
            SignSpec::Ed25519 { .. } => &record.plan.unsigned[..],
            _ => &digest[..],
        };
        let sig = crate::keys::sign(&key_name, &record.sign, message).await;
        // A freeze during signing stops the remaining signatures.
        let stop = match sig {
            Ok(_) if store::with(|w| w.frozen) => Some(Error::Locked),
            Ok(sig) => {
                signatures.push(sig);
                None
            }
            Err(e) => Some(e),
        };
        if let Some(e) = stop {
            let mut p = store::proposal(p.id)?;
            abandon(&mut p, back, now());
            return Err(e);
        }
    }
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(p.id)?;
    // Another proposal may have signed for the address meanwhile.
    let assembled = check_versions(&w, &p, address.id, &record.plan.params).and_then(|_| {
        chains::assemble(
            &record.assembly,
            &record.plan.unsigned,
            digests,
            &signatures,
            &native,
        )
    });
    let (raw, txid) = match assembled {
        Ok(tx) => tx,
        Err(e) => {
            abandon(&mut p, back, at);
            return Err(e);
        }
    };
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
    finish(w, &p);
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
    // A plan's broadcast is recorded once; Settled and Failed end the proposal.
    if args.outcome == ChainOutcome::Broadcast
        && p.broadcasts
            .iter()
            .any(|b| b.plan == args.plan && b.outcome == ChainOutcome::Broadcast)
    {
        return Ok(p);
    }
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
                    address,
                    chain_id,
                    proposal,
                    nonce,
                    ..
                } => {
                    // A report is not chain evidence. It may unblock this
                    // exact nonce but never releases the original spend.
                    if let Some(e) = w
                        .evm_pending
                        .iter()
                        .find(|e| {
                            e.address == *address
                                && e.chain_id == *chain_id
                                && e.proposal == *proposal
                                && e.nonce == *nonce
                        })
                        .cloned()
                    {
                        w.evm_pending.retain(|x| *x != e);
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
    let me = ic_cdk::api::canister_self();
    if store::job(REFUEL_JOB).is_none() {
        let at = now();
        let mut w = store::wallet();
        let Some(rule) = w.policy.refuel.clone() else {
            return Ok(None);
        };
        if ic_cdk::api::canister_cycle_balance() >= rule.min_cycles {
            return Ok(None);
        }
        w.roll_day(at);
        ensure(w.refuels_today < rule.max_per_day, Error::QuotaExceeded)?;
        w.refuels_today += 1;
        let job = TopUp {
            from: icp_subaccount(rule.from).map(Hash::new),
            amount: rule.icp_e8s,
            created_at_time: at * NANOS_PER_MILLISECOND,
            block: None,
        };
        store::save_job(REFUEL_JOB, &Job::TopUp(job));
        store::save(w);
    }
    match run_top_up(REFUEL_JOB, me).await? {
        None => Ok(None),
        Some(TopUpEnd::Minted { block, cycles }) => {
            let at = now();
            let mut w = store::wallet();
            store::audit(&mut w, at, None, None, AuditKind::Refueled { block });
            store::save(w);
            Ok(Some(cycles))
        }
        Some(TopUpEnd::Refused(reason)) => Err(Error::InvalidInput(reason)),
    }
}
