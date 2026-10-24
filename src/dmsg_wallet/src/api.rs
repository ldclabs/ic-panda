//! Install, bootstrap, reads, membership and proposal decisions.
use crate::store::{self, STABLE_SCHEMA, Settings, Spend, Wallet};
use candid::Principal;
use dmsg_protocol::{
    agent::{account_home, check_user_home, validate_user_homes},
    authenticated, canonical, validate_namespace,
    wallet::*,
};
use dmsg_types::{
    integration::{ApplicationApproval, ApplicationAuthorization, ApprovalPurpose},
    wallet::*,
    *,
};
use std::time::Duration;

/// Proposals kept before the oldest finished ones are removed.
const RETAINED_PROPOSALS: u64 = 10_000;
/// Oldest proposals one prune looks at.
const PRUNE_SCAN: usize = 200;
/// Newest proposals `wallet_stats` counts the unfinished ones of.
const STATS_SCAN: usize = 200;

pub(crate) fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

/// Addresses 1 to 3: Bitcoin Taproot, EVM and Solana, created by `bootstrap`.
const DEFAULT_ADDRESSES: [(u32, Chain, &str); 3] = [
    (1, Chain::Bitcoin, "Bitcoin"),
    (2, Chain::Evm, "EVM"),
    (3, Chain::Solana, "Solana"),
];

fn role_counts(members: &[Member]) -> (usize, usize) {
    let admins = members.iter().filter(|m| m.role == Role::Admin).count();
    let approvers = members
        .iter()
        .filter(|m| matches!(m.role, Role::Admin | Role::Approver))
        .count();
    (admins, approvers)
}

#[ic_cdk::init]
fn init(args: WalletInit) {
    let at = now();
    validate_namespace(&args.issuer_namespace).expect("issuer namespace");
    validate_user_homes(&args.environment, &args.issuer_namespace, &args.user_homes)
        .expect("user homes");
    authenticated(args.hub).expect("hub");
    authenticated(args.creator_principal).expect("creator principal");
    assert!(!args.name.is_empty() && args.name.len() <= 64, "name");
    assert!(args.members.len() < MAX_MEMBERS, "members");
    let me = ic_cdk::api::canister_self();
    let mut members = vec![Member {
        id: args.creator.clone(),
        role: Role::Admin,
        label: "creator".into(),
        agent_ids: vec![],
        status: MemberStatus::Active,
        principals: vec![args.creator_principal],
        added_at_ms: at,
    }];
    for m in &args.members {
        validate_member(m).expect("member");
        assert!(members.iter().all(|x| x.id != m.id), "duplicate member");
        members.push(invited(m, at));
    }
    let (admins, approvers) = role_counts(&members);
    assert_eq!(args.policy.version, 1, "policy version");
    validate_policy(&args.policy, admins, approvers).expect("policy");
    let mut w = Wallet {
        schema: STABLE_SCHEMA,
        settings: Settings {
            environment: args.environment,
            hub: args.hub,
            key_name: args.key_name,
            btc_network: args.btc_network,
            icp_ledger: args.icp_ledger,
            cmc: args.cmc,
            issuer_namespace: args.issuer_namespace,
            user_homes: args.user_homes,
            name: args.name,
            channel_hint: args.channel_hint,
        },
        created_at_ms: at,
        members,
        policy: args.policy,
        addresses: vec![Address {
            id: 0,
            chain: Chain::Icp,
            kind: AddressKind::ChainKey,
            public_key: Default::default(),
            text: icp_account(me, 0).to_string(),
            label: "ICP".into(),
            cosign: None,
            sol_nonce: None,
            created_at_ms: at,
        }],
        cosigners: vec![],
        next_address: 4,
        next_cosigner: 1,
        next_proposal: 1,
        frozen: false,
        code: CodeAcceptance::None,
        code_busy: false,
        key_roots: KeyRoots::default(),
        audit_head: Hash::new([0; 32]),
        day: at / DAY,
        signatures_today: 0,
        refuels_today: 0,
        evm_pending: vec![],
        sol_last: vec![],
        spends: vec![],
    };
    store::audit(&mut w, at, Some(args.creator), None, AuditKind::Created);
    store::save(w);
    ensure_bootstrap();
}

fn invited(m: &MemberInput, at: u64) -> Member {
    Member {
        id: m.id.clone(),
        role: m.role,
        label: m.label.clone(),
        agent_ids: m.agent_ids.clone(),
        status: MemberStatus::Invited,
        principals: vec![],
        added_at_ms: at,
    }
}

/// The hub passes the version it installs; it clears a matching acceptance.
#[ic_cdk::post_upgrade]
fn post_upgrade(version: Option<u32>) {
    let mut w = store::wallet();
    assert_eq!(
        w.schema, STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    if matches!(w.code, CodeAcceptance::Upgrade { version: v, .. } if Some(v) == version) {
        w.code = CodeAcceptance::None;
    }
    w.code_busy = false;
    for spend in &mut w.spends {
        if spend.executed_at_ms.is_none()
            && let Ok(p) = store::proposal(spend.proposal)
            && !is_waiting(p.status)
        {
            spend.executed_at_ms = Some(
                p.signed
                    .first()
                    .map_or(p.updated_at_ms, |s| s.signed_at_ms)
                    .max(spend.at_ms),
            );
        }
    }
    // The view's encoding may change with the code, so its leaf is certified
    // again; saving also republishes the root.
    store::save(w);
    ensure_bootstrap();
}

/// Whether the chain-key roots and the default addresses exist.
fn bootstrapped(w: &Wallet) -> bool {
    w.key_roots.ecdsa.is_some()
        && w.key_roots.bip340.is_some()
        && w.key_roots.ed25519.is_some()
        && DEFAULT_ADDRESSES
            .iter()
            .all(|(id, ..)| w.address(*id).is_ok())
}

/// Bootstrap in a timer, retrying every minute until it succeeds.
fn ensure_bootstrap() {
    if !store::with(bootstrapped) {
        ic_cdk_timers::set_timer(Duration::ZERO, retry_bootstrap());
    }
}

async fn retry_bootstrap() {
    if bootstrap_keys().await.is_err() {
        ic_cdk_timers::set_timer(Duration::from_secs(60), retry_bootstrap());
    }
}

/// Read the chain-key roots and create the default chain addresses. A timer
/// runs it after install and upgrades until it succeeds; anyone may run it
/// sooner.
#[ic_cdk::update]
async fn bootstrap() -> Result<KeyRoots> {
    bootstrap_keys().await
}

async fn bootstrap_keys() -> Result<KeyRoots> {
    let (key_name, mut roots, done) = store::with(|w| {
        (
            w.settings.key_name.clone(),
            w.key_roots.clone(),
            bootstrapped(w),
        )
    });
    if done {
        return Ok(roots);
    }
    for algorithm in [
        KeyAlgorithm::Ecdsa,
        KeyAlgorithm::Bip340,
        KeyAlgorithm::Ed25519,
    ] {
        let slot = match algorithm {
            KeyAlgorithm::Ecdsa => &mut roots.ecdsa,
            KeyAlgorithm::Bip340 => &mut roots.bip340,
            KeyAlgorithm::Ed25519 => &mut roots.ed25519,
        };
        if slot.is_none() {
            let (public_key, chain_code) =
                crate::keys::public_key(&key_name, algorithm, vec![]).await?;
            *slot = Some(KeyRoot {
                public_key: public_key.into(),
                chain_code: Hash::new(chain_code.try_into().map_err(|_| Error::IntegrityFailed)?),
            });
        }
    }
    let mut created = vec![];
    for (id, chain, label) in DEFAULT_ADDRESSES {
        if store::with(|w| w.address(id).is_ok()) {
            continue;
        }
        let kind = AddressKind::ChainKey;
        let algorithm = key_algorithm(chain, &kind)?.expect("signing chain");
        let (key, _) =
            crate::keys::public_key(&key_name, algorithm, derivation_path(chain, &kind, id)?)
                .await?;
        created.push((id, chain, label, key));
    }
    // Keys never change, so only the records written meanwhile are reread.
    let at = now();
    let mut w = store::wallet();
    w.key_roots = roots.clone();
    let me = ic_cdk::api::canister_self();
    for (id, chain, label, key) in created {
        if w.address(id).is_err() {
            let text = address_text(
                me,
                w.settings.btc_network,
                id,
                chain,
                &AddressKind::ChainKey,
                &key,
                None,
            )?;
            w.addresses.push(Address {
                id,
                chain,
                kind: AddressKind::ChainKey,
                public_key: key.into(),
                text,
                label: label.into(),
                cosign: None,
                sol_nonce: None,
                created_at_ms: at,
            });
        }
    }
    w.addresses.sort_by_key(|a| a.id);
    store::save(w);
    Ok(roots)
}

fn check_reader(w: &Wallet, caller: Principal) -> Result<()> {
    ensure(w.readable_by(caller), Error::Forbidden)
}

#[ic_cdk::query]
fn get_wallet() -> Result<WalletView> {
    store::with(|w| {
        check_reader(w, ic_cdk::api::msg_caller())?;
        Ok(w.view())
    })
}

#[ic_cdk::query]
fn get_wallet_certified() -> Result<CertifiedBatch> {
    let me = ic_cdk::api::canister_self();
    let view = store::with(|w| {
        check_reader(w, ic_cdk::api::msg_caller())?;
        Ok::<_, Error>(canonical(&w.view()))
    })?;
    store::certified(me, vec![b"wallet".to_vec()], |_| Some(view.clone()))
}

/// A proposal as stored, with deadlines applied.
fn current(mut p: Proposal, w: &Wallet, at: u64) -> Proposal {
    close(&mut p, w, at);
    p
}

#[ic_cdk::query]
fn get_proposal(id: u64) -> Result<Proposal> {
    let at = now();
    store::with(|w| {
        check_reader(w, ic_cdk::api::msg_caller())?;
        Ok(current(store::proposal(id)?, w, at))
    })
}

/// The stored proposal record; deadlines that passed since are not applied.
#[ic_cdk::query]
fn get_proposal_certified(id: u64) -> Result<CertifiedBatch> {
    let me = ic_cdk::api::canister_self();
    store::with(|w| check_reader(w, ic_cdk::api::msg_caller()))?;
    let bytes = canonical(&store::proposal(id)?);
    store::certified(me, vec![store::proposal_key(id)], |_| Some(bytes.clone()))
}

#[ic_cdk::query]
fn list_proposals(before: Option<u64>, limit: u16) -> Result<ProposalPage> {
    let at = now();
    store::with(|w| {
        check_reader(w, ic_cdk::api::msg_caller())?;
        let limit = usize::from(limit.clamp(1, 100));
        let proposals: Vec<Proposal> = store::proposals_before(before, limit)
            .into_iter()
            .map(|p| current(p, w, at))
            .collect();
        let next = (proposals.len() == limit)
            .then(|| proposals.last().map(|p| p.id))
            .flatten();
        Ok(ProposalPage { proposals, next })
    })
}

#[ic_cdk::query]
fn get_plan(id: u64, seq: u32) -> Result<ExecutionPlan> {
    store::with(|w| check_reader(w, ic_cdk::api::msg_caller()))?;
    Ok(store::plan(id, seq)?.plan)
}

#[ic_cdk::query]
fn address_book(after: Option<Destination>, limit: u16) -> Result<Vec<BookEntry>> {
    store::with(|w| check_reader(w, ic_cdk::api::msg_caller()))?;
    Ok(store::book_page(after, usize::from(limit.clamp(1, 200))))
}

#[ic_cdk::query]
fn audit(from: u64, limit: u16) -> Result<Vec<AuditEvent>> {
    store::with(|w| check_reader(w, ic_cdk::api::msg_caller()))?;
    Ok(store::audit_page(from, u64::from(limit.clamp(1, 200))))
}

#[ic_cdk::query]
fn wallet_stats() -> WalletStats {
    let at = now();
    store::with(|w| WalletStats {
        cycles: ic_cdk::api::canister_cycle_balance(),
        signatures_today: if w.day == at / DAY {
            w.signatures_today
        } else {
            0
        },
        proposals: store::proposal_count(),
        active_proposals: store::proposals_before(None, STATS_SCAN)
            .into_iter()
            .map(|p| current(p, w, at))
            .filter(|p| !is_terminal(p.status))
            .count() as u64,
        stable_pages: store::stable_pages(),
    })
}

/// Explain the same reservations and rolling window used for approvals.
#[ic_cdk::query]
fn policy_usage() -> Result<Vec<SpendUsage>> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    store::with(|w| {
        check_reader(w, caller)?;
        let mut usage: Vec<SpendUsage> = vec![];
        for spend in w.spends.iter().filter(|s| w.retains_spend(s, at)) {
            let index = usage.iter().position(|u| u.asset == spend.asset);
            let row = match index {
                Some(index) => &mut usage[index],
                None => {
                    usage.push(SpendUsage {
                        asset: spend.asset.clone(),
                        reserved: 0,
                        executed: 0,
                    });
                    usage.last_mut().expect("inserted")
                }
            };
            if spend.executed_at_ms.is_some() {
                row.executed = row.executed.saturating_add(spend.amount);
            } else {
                row.reserved = row.reserved.saturating_add(spend.amount);
            }
        }
        Ok(usage)
    })
}

/// What the hub may install, read by the hub before an upgrade or lock.
#[ic_cdk::query]
fn accepted_upgrade() -> CodeAcceptance {
    store::with(|w| w.code.clone())
}

/// The hub claims the exact accepted action before changing canister state.
/// Retrying the same claim is safe after an interrupted hub operation.
#[ic_cdk::update]
fn claim_code(expected: CodeAcceptance) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let mut w = store::wallet();
    ensure(caller == w.settings.hub, Error::Forbidden)?;
    ensure(
        expected != CodeAcceptance::None && w.code == expected,
        Error::VersionConflict,
    )?;
    w.code_busy = true;
    store::save(w);
    Ok(())
}

/// Release an interrupted claim only after the hub has restarted the wallet.
#[ic_cdk::update]
fn release_code() -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let mut w = store::wallet();
    ensure(caller == w.settings.hub, Error::Forbidden)?;
    w.code_busy = false;
    store::save(w);
    Ok(())
}

/// Members for the hub's index.
#[ic_cdk::query]
fn member_ids() -> Result<Vec<MemberId>> {
    store::with(|w| {
        check_reader(w, ic_cdk::api::msg_caller())?;
        Ok(w.members.iter().map(|m| m.id.clone()).collect())
    })
}

/// The user home of an account member, named by its ID's fingerprint.
fn home_of(w: &Wallet, account: &AccountId) -> Result<Principal> {
    account_home(
        &w.settings.environment,
        &w.settings.issuer_namespace,
        &w.settings.user_homes,
        account,
    )
    .ok_or(Error::NotFound)
}

/// The user home of an account member. When none matches, the homes the
/// hub listed since are appended once and the lookup runs again.
async fn member_home(account: &AccountId) -> Result<Principal> {
    let (home, hub) = store::with(|w| (home_of(w, account), w.settings.hub));
    if home.is_ok() {
        return home;
    }
    let homes: Vec<Principal> = dmsg_runtime::call(hub, "get_user_homes", ()).await?;
    let mut w = store::wallet();
    let mut changed = false;
    for home in homes {
        if check_user_home(
            &w.settings.environment,
            &w.settings.issuer_namespace,
            &w.settings.user_homes,
            home,
        )? {
            w.settings.user_homes.push(home);
            changed = true;
        }
    }
    let home = home_of(&w, account);
    if changed {
        store::save(w);
    }
    home
}

/// Verify an account member's device approval at its user home.
async fn verify_account(
    home: Principal,
    approval_id: Hash,
    approval: &ApplicationApproval,
) -> Result<ApplicationAuthorization> {
    let reply: Result<ApplicationAuthorization> = dmsg_runtime::call(
        home,
        "verify_application_authorization",
        (approval_id, approval.clone()),
    )
    .await?;
    reply
}

fn expected<'a>(
    w: &'a Wallet,
    me: Principal,
    purpose: ApprovalPurpose,
    action_digest: Hash,
    account: &'a AccountId,
    actor: Principal,
) -> ExpectedApproval<'a> {
    ExpectedApproval {
        environment: &w.settings.environment,
        hub: w.settings.hub,
        service: me,
        purpose,
        action_digest,
        account,
        actor,
    }
}

/// Confirm an invitation and bind the calling Principal; an account member
/// may call it again to bind another login Principal.
#[ic_cdk::update]
async fn join(evidence: MemberApproval) -> Result<Member> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    let mut at = now();
    authenticated(caller)?;
    let id = match &evidence {
        MemberApproval::Principal => MemberId::Principal(caller),
        MemberApproval::Account {
            approval_id,
            approval,
        } => {
            let account = approval.approving_account;
            store::with(|w| {
                w.member(&MemberId::Account(account))
                    .ok_or(Error::Forbidden)?;
                check_wallet_approval(
                    approval,
                    &expected(
                        w,
                        me,
                        ApprovalPurpose::WalletJoin,
                        join_digest(me, &account, caller),
                        &account,
                        caller,
                    ),
                    at,
                )
            })?;
            let home = member_home(&account).await?;
            verify_account(home, *approval_id, approval).await?;
            at = now();
            MemberId::Account(account)
        }
    };
    let mut w = store::wallet();
    let member = w.member(&id).ok_or(Error::Forbidden)?;
    if member.status == MemberStatus::Active && member.principals.contains(&caller) {
        return Ok(member.clone());
    }
    // A Principal acts as one member only.
    ensure(
        w.members.iter().all(|m| {
            m.id == id || (m.id != MemberId::Principal(caller) && !m.principals.contains(&caller))
        }),
        Error::IdempotencyConflict,
    )?;
    let member = w.members.iter_mut().find(|m| m.id == id).expect("member");
    if !member.principals.contains(&caller) {
        ensure(
            member.principals.len() < MAX_MEMBER_PRINCIPALS,
            Error::QuotaExceeded,
        )?;
        member.principals.push(caller);
    }
    let activating = member.status != MemberStatus::Active;
    member.status = MemberStatus::Active;
    let member = member.clone();
    if activating {
        w.policy.version += 1;
    }
    store::audit(&mut w, at, Some(id), None, AuditKind::Joined);
    store::save(w);
    Ok(member)
}

/// Stop executions at once. Any Admin or Approver may freeze; unfreezing
/// is an Admin proposal with a delay.
#[ic_cdk::update]
fn freeze() -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut w = store::wallet();
    let member = w.acting(caller)?.clone();
    ensure(can_veto(member.role), Error::Forbidden)?;
    if !w.frozen {
        w.frozen = true;
        store::audit(&mut w, at, Some(member.id), None, AuditKind::Frozen);
        store::save(w);
    }
    Ok(())
}

/// Close a proposal whose lifetime ended or whose policy changed, and move
/// a delayed proposal past its veto period to Ready. Returns whether the
/// status changed.
pub(crate) fn close(p: &mut Proposal, w: &Wallet, at: u64) -> bool {
    let waiting = is_waiting(p.status);
    let status = if waiting && at >= p.expires_at_ms {
        ProposalStatus::Expired
    } else if waiting && p.policy_version != w.policy.version {
        ProposalStatus::Stale
    } else if p.status == ProposalStatus::Delayed && p.ready_at_ms.is_some_and(|r| at >= r) {
        ProposalStatus::Ready
    } else {
        return false;
    };
    p.status = status;
    p.updated_at_ms = at;
    true
}

/// Apply `close` to a stored proposal and persist a change; a proposal whose
/// lifetime ended or whose policy changed is refused.
pub(crate) fn refresh(w: &mut Wallet, p: &mut Proposal, at: u64) -> Result<()> {
    if close(p, w, at) {
        save_closed(w, p, at);
        store::save(w.clone());
    }
    ensure(
        !matches!(p.status, ProposalStatus::Expired | ProposalStatus::Stale),
        Error::Expired,
    )
}

/// Refuse what a freeze stops: only Unfreeze and membership changes proceed.
pub(crate) fn unlocked(w: &Wallet, intent: &Intent) -> Result<()> {
    ensure(
        !w.frozen || matches!(intent, Intent::Unfreeze) || is_membership(intent),
        Error::Locked,
    )
}

/// Validate the authority for a first execution, including recovery's current
/// role. An already signed transaction cannot be revoked by changing members.
pub(crate) fn execution_authorized(w: &Wallet, p: &Proposal, at: u64) -> Result<()> {
    ensure(
        p.policy_version == w.policy.version && at < p.expires_at_ms,
        Error::VersionConflict,
    )?;
    unlocked(w, &p.intent)?;
    if p.requirement.approvals == 0 {
        ensure(
            is_membership(&p.intent)
                && w.member(&p.proposer)
                    .is_some_and(|m| m.status == MemberStatus::Active && m.role == Role::Recovery),
            Error::Forbidden,
        )
    } else {
        let approvals = p
            .votes
            .iter()
            .filter(|v| {
                v.decision == Decision::Approve
                    && w.member(&v.member).is_some_and(|m| {
                        m.status == MemberStatus::Active && counts(m.role, &p.requirement)
                    })
            })
            .count();
        ensure(
            approvals >= usize::from(p.requirement.approvals),
            Error::Forbidden,
        )
    }
}

/// Persist a closed proposal, release its window spend and audit the end.
pub(crate) fn save_closed(w: &mut Wallet, p: &Proposal, at: u64) {
    if matches!(
        p.status,
        ProposalStatus::Expired
            | ProposalStatus::Stale
            | ProposalStatus::Vetoed
            | ProposalStatus::Rejected
            | ProposalStatus::Cancelled
    ) {
        w.spends.retain(|s| s.proposal != p.id);
        store::audit(w, at, None, Some(p.id), AuditKind::Closed(p.status));
    }
    store::save_proposal(p);
}

fn window_and_book(w: &Wallet, intent: &Intent, at: u64) -> (u128, bool) {
    let class = classify(intent, &w.policy);
    let IntentClass::Spend { asset, amount } = class else {
        return (0, true);
    };
    let recipients: Vec<&Destination> = match intent {
        Intent::Transfer { to, .. } => vec![to],
        Intent::Batch { items, .. } => items.iter().map(|i| &i.to).collect(),
        _ => vec![],
    };
    let in_book = recipients.iter().all(|d| {
        store::book_entry(d)
            .is_some_and(|e| e.added_at_ms + w.policy.address_book_cooldown_ms <= at)
    });
    (w.window(&asset, at).saturating_add(amount), in_book)
}

/// State checks of an intent beyond its shape.
pub(crate) fn check_state(w: &Wallet, intent: &Intent) -> Result<()> {
    let address = |id: u32, chain: Chain| -> Result<&Address> {
        let a = w.address(id)?;
        ensure_valid(a.chain == chain, "address chain")?;
        Ok(a)
    };
    match intent {
        Intent::Transfer { from, asset, .. } | Intent::Batch { from, asset, .. } => {
            address(*from, asset_chain(asset))?;
            if let AssetKey::Evm { chain_id, .. } = asset {
                ensure(w.policy.evm_chains.contains(chain_id), Error::Forbidden)?;
            }
            if let Intent::Batch { items, .. } = intent {
                ensure_valid(
                    !matches!(asset, AssetKey::Btc) || items.len() <= 16,
                    "batch",
                )?;
            }
        }
        Intent::CanisterCall { canister, .. } => {
            ensure(w.policy.call_targets.contains(canister), Error::Forbidden)?;
        }
        Intent::AddAddress { kind, cosign, .. } => {
            ensure(w.addresses.len() < MAX_ADDRESSES, Error::QuotaExceeded)?;
            if let AddressKind::Taproot(control) = kind {
                ensure_valid(
                    cosign.is_none() && control.chain_key_after >= 1,
                    "native taproot",
                )?;
                crate::chains::btc::native_cosigner(w, control)?;
            }
            if let Some(policy) = cosign {
                check_cosigners(w, &policy.cosigners)?;
            }
        }
        Intent::SetCosign {
            address: id,
            cosign,
        } => {
            let a = w.address(*id)?;
            ensure_valid(!matches!(a.kind, AddressKind::Taproot(_)), "native taproot")?;
            ensure_valid(a.cosign != *cosign, "cosign policy unchanged")?;
            if let Some(policy) = cosign {
                check_cosigners(w, &policy.cosigners)?;
            }
        }
        Intent::AddCosigner(input) => {
            ensure(w.cosigners.len() < MAX_COSIGNERS, Error::QuotaExceeded)?;
            w.member(&input.member).ok_or(Error::NotFound)?;
            ensure(
                !w.policy.hardware_cosigners || input.hardware,
                Error::Forbidden,
            )?;
            ensure(
                w.cosigners
                    .iter()
                    .all(|c| c.input.public_key != input.public_key),
                Error::IdempotencyConflict,
            )?;
        }
        Intent::RemoveCosigner(id) => {
            w.cosigner(*id)?;
            ensure_valid(!w.taproot_cosigner(*id), "cosigner of a native address")?;
            check_cosigner_removal(w, &[*id])?;
        }
        Intent::AddMember(input) => {
            ensure(w.members.len() < MAX_MEMBERS, Error::QuotaExceeded)?;
            ensure(w.member(&input.id).is_none(), Error::IdempotencyConflict)?;
        }
        Intent::RemoveMember(id) => {
            w.member(id).ok_or(Error::NotFound)?;
            let owned: Vec<_> = w
                .cosigners
                .iter()
                .filter(|c| c.input.member == *id && !w.taproot_cosigner(c.id))
                .map(|c| c.id)
                .collect();
            check_cosigner_removal(w, &owned)?;
            let rest: Vec<Member> = w.members.iter().filter(|m| m.id != *id).cloned().collect();
            let (admins, approvers) = role_counts(&rest);
            validate_policy(&w.policy, admins, approvers)?;
        }
        Intent::UpdateMember(input) => {
            w.member(&input.id).ok_or(Error::NotFound)?;
            let mut rest = w.members.clone();
            if let Some(m) = rest.iter_mut().find(|m| m.id == input.id) {
                m.role = input.role;
            }
            let (admins, approvers) = role_counts(&rest);
            validate_policy(&w.policy, admins, approvers)?;
        }
        Intent::SetPolicy(policy) => {
            ensure(
                policy.version == w.policy.version + 1,
                Error::VersionConflict,
            )?;
            let (admins, approvers) = role_counts(&w.members);
            validate_policy(policy, admins, approvers)?;
            for a in &w.addresses {
                if let Some(required) = &a.cosign {
                    let eligible = w
                        .cosigners
                        .iter()
                        .filter(|c| {
                            required.cosigners.contains(&c.id)
                                && (!policy.hardware_cosigners || c.input.hardware)
                        })
                        .count();
                    ensure_valid(
                        eligible >= usize::from(required.threshold),
                        "hardware cosigner threshold unreachable",
                    )?;
                }
            }
            if let Some(r) = &policy.refuel {
                address(r.from, Chain::Icp)?;
            }
        }
        Intent::AddressBook(BookChange::Add { destination, .. }) => {
            ensure(
                store::book_len() < MAX_BOOK_ENTRIES as u64
                    || store::book_entry(destination).is_some(),
                Error::QuotaExceeded,
            )?;
        }
        Intent::AddressBook(BookChange::Remove { .. }) => {}
        Intent::Unfreeze => ensure_valid(w.frozen, "not frozen")?,
        Intent::Refuel { from, .. } => {
            address(*from, Chain::Icp)?;
        }
        Intent::AcceptUpgrade { version, .. } => {
            ensure(!w.code_busy, Error::Pending)?;
            ensure_valid(*version > 0 && w.code != CodeAcceptance::Lock, "upgrade")?;
        }
        Intent::LockCode => {
            ensure(!w.code_busy, Error::Pending)?;
            ensure_valid(w.code != CodeAcceptance::Lock, "locked")?;
        }
        Intent::CancelEvmNonce {
            address: id,
            chain_id,
            proposal,
            nonce,
            ..
        }
        | Intent::ReleaseEvmNonce {
            address: id,
            chain_id,
            proposal,
            nonce,
        } => {
            address(*id, Chain::Evm)?;
            ensure(
                w.evm_pending.iter().any(|e| {
                    e.address == *id
                        && e.chain_id == *chain_id
                        && e.proposal == *proposal
                        && e.nonce == *nonce
                }),
                Error::NotFound,
            )?;
        }
        Intent::InitSolNonce { address: id, .. } => {
            ensure_valid(
                address(*id, Chain::Solana)?.sol_nonce.is_none(),
                "nonce account exists",
            )?;
        }
    }
    Ok(())
}

fn check_cosigner_removal(w: &Wallet, ids: &[u32]) -> Result<()> {
    ensure_valid(
        !w.addresses.iter().any(|a| {
            a.cosign
                .as_ref()
                .is_some_and(|p| p.cosigners.iter().any(|id| ids.contains(id)))
        }),
        "cosigner still used by address",
    )
}

fn check_cosigners(w: &Wallet, ids: &[u32]) -> Result<()> {
    for id in ids {
        let c = w.cosigner(*id)?;
        ensure(
            !w.policy.hardware_cosigners || c.input.hardware,
            Error::Forbidden,
        )?;
    }
    Ok(())
}

#[derive(candid::CandidType, serde::Serialize)]
struct ConsentRequest {
    method: String,
    arg: serde_bytes::ByteBuf,
    user_preferences: ConsentSpec,
}

#[derive(candid::CandidType, serde::Serialize)]
struct ConsentSpec {
    metadata: ConsentMetadata,
    device_spec: Option<DeviceSpec>,
}

#[derive(candid::CandidType, serde::Serialize, serde::Deserialize)]
struct ConsentMetadata {
    language: String,
    utc_offset_minutes: Option<i16>,
}

#[derive(candid::CandidType, serde::Serialize)]
enum DeviceSpec {
    GenericDisplay,
}

#[derive(candid::CandidType, serde::Deserialize)]
enum ConsentMessage {
    GenericDisplayMessage(String),
}

#[derive(candid::CandidType, serde::Deserialize)]
struct ConsentInfo {
    consent_message: ConsentMessage,
    metadata: ConsentMetadata,
}

/// The target's ICRC-21 consent text, if it gives a generic one.
async fn consent(canister: Principal, method: &str, arg: &[u8]) -> Option<ConsentRecord> {
    let request = ConsentRequest {
        method: method.into(),
        arg: arg.to_vec().into(),
        user_preferences: ConsentSpec {
            metadata: ConsentMetadata {
                language: "en".into(),
                utc_offset_minutes: None,
            },
            device_spec: Some(DeviceSpec::GenericDisplay),
        },
    };
    let reply: std::result::Result<ConsentInfo, candid::Reserved> =
        dmsg_runtime::call(canister, "icrc21_canister_call_consent_message", (request,))
            .await
            .ok()?;
    let info = reply.ok()?;
    let ConsentMessage::GenericDisplayMessage(message) = info.consent_message;
    Some(ConsentRecord {
        language: info.metadata.language,
        message: message.chars().take(4_096).collect(),
    })
}

/// Preview the current policy without creating a proposal or reserving funds.
#[ic_cdk::query]
fn preview_intent(intent: Intent) -> Result<Requirement> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    validate_intent(&intent)?;
    store::with(|w| {
        let member = w.acting(caller)?;
        check_state(w, &intent)?;
        unlocked(w, &intent)?;
        let (window, in_book) = window_and_book(w, &intent, at);
        requirement(
            &w.policy,
            &intent,
            member.role,
            Voters::of(&w.members),
            window,
            in_book,
        )
    })
}

#[ic_cdk::update]
async fn propose(args: ProposeArgs) -> Result<Proposal> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    let ProposeArgs {
        mut intent,
        op_id,
        via_agent,
    } = args;
    validate_intent(&intent)?;
    dmsg_protocol::nonzero(&op_id[..])?;
    let proposer = store::with(|w| w.acting(caller).map(|m| m.id.clone()))?;
    if let Some(id) = store::op(&op_id) {
        let p = store::proposal(id)?;
        ensure(
            p.proposer == proposer && p.via_agent == via_agent && same_intent(&p.intent, &intent),
            Error::IdempotencyConflict,
        )?;
        return Ok(p);
    }
    if let Intent::CanisterCall {
        canister,
        method,
        arg,
        consent: c,
        ..
    } = &mut intent
    {
        // Only a listed target is asked; every approver sees its own
        // description of the call.
        ensure(
            store::with(|w| w.policy.call_targets.contains(canister)),
            Error::Forbidden,
        )?;
        *c = consent(*canister, method, arg).await;
    }
    // Membership, policy and the operation index are reread after the call.
    let at = now();
    let mut w = store::wallet();
    let member = w.acting(caller)?.clone();
    if let Some(agent) = &via_agent {
        ensure(member.agent_ids.contains(agent), Error::Forbidden)?;
    }
    ensure(store::op(&op_id).is_none(), Error::IdempotencyConflict)?;
    check_state(&w, &intent)?;
    unlocked(&w, &intent)?;
    let (window, in_book) = window_and_book(&w, &intent, at);
    let requirement = requirement(
        &w.policy,
        &intent,
        member.role,
        Voters::of(&w.members),
        window,
        in_book,
    )?;
    let id = w.next_proposal;
    w.next_proposal += 1;
    let digest = proposal_digest(
        &w.settings.environment,
        me,
        &op_id,
        &intent,
        w.policy.version,
        &member.id,
        &via_agent,
    );
    let mut p = Proposal {
        id,
        op_id,
        intent,
        policy_version: w.policy.version,
        proposer: member.id.clone(),
        via_agent,
        digest,
        created_at_ms: at,
        expires_at_ms: at + requirement.delay_ms + w.policy.proposal_ttl_ms,
        requirement,
        status: ProposalStatus::Open,
        votes: vec![],
        ready_at_ms: None,
        plans: 0,
        signed: vec![],
        broadcasts: vec![],
        outcome: None,
        failure: None,
        updated_at_ms: at,
    };
    store::audit(
        &mut w,
        at,
        Some(member.id.clone()),
        Some(id),
        AuditKind::Proposed,
    );
    if p.requirement.approvals == 0 {
        approve(&mut w, &mut p, at);
    }
    store::save_op(&op_id, id);
    store::save_proposal(&p);
    // Each new proposal clears a little of the history beyond the retained count.
    prune_finished(&mut w, at, 2);
    store::save(w);
    Ok(p)
}

/// Equal intents, ignoring the consent text the wallet filled in.
fn same_intent(stored: &Intent, asked: &Intent) -> bool {
    match (stored, asked) {
        (
            Intent::CanisterCall {
                canister,
                method,
                arg,
                cycles,
                ..
            },
            Intent::CanisterCall {
                canister: c,
                method: m,
                arg: a,
                cycles: y,
                ..
            },
        ) => canister == c && method == m && arg == a && cycles == y,
        _ => stored == asked,
    }
}

/// Requirement met: start the veto delay or become Ready, and count a
/// spend in the rolling window.
fn approve(w: &mut Wallet, p: &mut Proposal, at: u64) {
    if let IntentClass::Spend { asset, amount } = classify(&p.intent, &w.policy) {
        w.spends = w
            .spends
            .iter()
            .filter(|s| w.retains_spend(s, at))
            .cloned()
            .collect();
        w.spends.push(Spend {
            asset,
            amount,
            at_ms: at,
            proposal: p.id,
            executed_at_ms: None,
        });
    }
    if p.requirement.delay_ms > 0 {
        p.status = ProposalStatus::Delayed;
        p.ready_at_ms = Some(at + p.requirement.delay_ms);
    } else {
        p.status = ProposalStatus::Ready;
        p.ready_at_ms = Some(at);
    }
    p.updated_at_ms = at;
    store::audit(w, at, None, Some(p.id), AuditKind::Approved);
}

/// Raise a spend's requirement to what its rolling window asks now: spends
/// approved since it was proposed count, so a payment split into several
/// proposals still meets the tier of their sum. The delay it gains extends
/// its lifetime.
fn raise_requirement(w: &Wallet, p: &mut Proposal, at: u64) -> Result<()> {
    let IntentClass::Spend { asset, .. } = classify(&p.intent, &w.policy) else {
        return Ok(());
    };
    let (window, in_book) = window_and_book(w, &p.intent, at);
    let fresh = spend_requirement(&w.policy, &asset, Voters::of(&w.members), window, in_book)?;
    let r = &p.requirement;
    if fresh.approvals > r.approvals || fresh.delay_ms > r.delay_ms {
        p.requirement = Requirement {
            approvals: fresh.approvals.max(r.approvals),
            admin: false,
            delay_ms: fresh.delay_ms.max(r.delay_ms),
            window_amount: fresh.window_amount,
        };
        p.expires_at_ms = p
            .expires_at_ms
            .max(p.created_at_ms + p.requirement.delay_ms + w.policy.proposal_ttl_ms);
    }
    Ok(())
}

/// The member deciding, checked against the proposal's current state.
fn check_decision(
    w: &Wallet,
    p: &Proposal,
    member: &MemberId,
    caller: Principal,
    decision: Decision,
) -> Result<Role> {
    let m = w.member(member).ok_or(Error::Forbidden)?;
    ensure(
        m.status == MemberStatus::Active && m.principals.contains(&caller),
        Error::Forbidden,
    )?;
    match decision {
        Decision::Approve | Decision::Reject => {
            ensure(p.status == ProposalStatus::Open, Error::VersionConflict)?;
            ensure(counts(m.role, &p.requirement), Error::Forbidden)?;
            ensure(
                p.votes.iter().all(|v| v.member != *member),
                Error::IdempotencyConflict,
            )?;
            if decision == Decision::Approve {
                unlocked(w, &p.intent)?;
            }
        }
        Decision::Veto => {
            ensure(p.status == ProposalStatus::Delayed, Error::VersionConflict)?;
            ensure(can_veto(m.role), Error::Forbidden)?;
        }
    }
    Ok(m.role)
}

async fn decide(id: u64, evidence: MemberApproval, decision: Decision) -> Result<Proposal> {
    let caller = ic_cdk::api::msg_caller();
    let me = ic_cdk::api::canister_self();
    let mut at = now();
    let member = match &evidence {
        MemberApproval::Principal => MemberId::Principal(caller),
        MemberApproval::Account { approval, .. } => MemberId::Account(approval.approving_account),
    };
    if let MemberApproval::Account {
        approval_id,
        approval,
    } = &evidence
    {
        let account = approval.approving_account;
        let mut w = store::wallet();
        let mut p = store::proposal(id)?;
        refresh(&mut w, &mut p, at)?;
        check_decision(&w, &p, &member, caller, decision)?;
        check_wallet_approval(
            approval,
            &expected(
                &w,
                me,
                ApprovalPurpose::WalletProposal,
                decision_digest(&p.digest, decision),
                &account,
                caller,
            ),
            at,
        )?;
        let home = member_home(&account).await?;
        verify_account(home, *approval_id, approval).await?;
        // Time, membership and the proposal are reread after the home's reply.
        at = now();
    }
    let mut w = store::wallet();
    let mut p = store::proposal(id)?;
    refresh(&mut w, &mut p, at)?;
    check_decision(&w, &p, &member, caller, decision)?;
    if decision == Decision::Approve {
        raise_requirement(&w, &mut p, at)?;
    }
    p.votes.push(Vote {
        member: member.clone(),
        decision,
        at_ms: at,
    });
    p.updated_at_ms = at;
    store::audit(
        &mut w,
        at,
        Some(member),
        Some(id),
        AuditKind::Decided(decision),
    );
    let counted = |d: Decision| {
        p.votes
            .iter()
            .filter(|v| v.decision == d)
            .filter(|v| {
                w.member(&v.member).is_some_and(|m| {
                    m.status == MemberStatus::Active && counts(m.role, &p.requirement)
                })
            })
            .count()
    };
    match decision {
        Decision::Approve if counted(Decision::Approve) >= usize::from(p.requirement.approvals) => {
            approve(&mut w, &mut p, at);
            if p.status == ProposalStatus::Ready {
                crate::execute::apply_ready(&mut w, &mut p, at);
            }
        }
        Decision::Reject => {
            let eligible = w
                .members
                .iter()
                .filter(|m| m.status == MemberStatus::Active && counts(m.role, &p.requirement))
                .count();
            if eligible.saturating_sub(counted(Decision::Reject))
                < usize::from(p.requirement.approvals)
            {
                p.status = ProposalStatus::Rejected;
            }
        }
        Decision::Veto => p.status = ProposalStatus::Vetoed,
        Decision::Approve => {}
    }
    save_closed(&mut w, &p, at);
    store::save(w);
    Ok(p)
}

#[ic_cdk::update]
async fn approve_proposal(id: u64, evidence: MemberApproval) -> Result<Proposal> {
    decide(id, evidence, Decision::Approve).await
}

#[ic_cdk::update]
async fn reject_proposal(id: u64, evidence: MemberApproval) -> Result<Proposal> {
    decide(id, evidence, Decision::Reject).await
}

#[ic_cdk::update]
async fn veto_proposal(id: u64, evidence: MemberApproval) -> Result<Proposal> {
    decide(id, evidence, Decision::Veto).await
}

/// The proposer withdraws a proposal not yet executing.
#[ic_cdk::update]
fn cancel_proposal(id: u64) -> Result<Proposal> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut w = store::wallet();
    let mut p = store::proposal(id)?;
    let member = w.acting(caller)?.id.clone();
    ensure(p.proposer == member, Error::Forbidden)?;
    refresh(&mut w, &mut p, at)?;
    ensure(is_waiting(p.status), Error::VersionConflict)?;
    p.status = ProposalStatus::Cancelled;
    p.updated_at_ms = at;
    save_closed(&mut w, &p, at);
    store::save(w);
    Ok(p)
}

/// Remove up to `limit` of the oldest finished proposals beyond the retained
/// count, applying deadlines that passed first; a proposal still in flight
/// is skipped.
pub(crate) fn prune_finished(w: &mut Wallet, at: u64, limit: u64) -> u32 {
    let excess = store::proposal_count()
        .saturating_sub(RETAINED_PROPOSALS)
        .min(limit);
    let mut removed = 0;
    if excess == 0 {
        return removed;
    }
    for id in store::oldest_ids(PRUNE_SCAN) {
        let Ok(mut p) = store::proposal(id) else {
            continue;
        };
        if close(&mut p, w, at) {
            save_closed(w, &p, at);
        }
        if is_terminal(p.status) {
            store::remove_proposal(id);
            removed += 1;
            if u64::from(removed) == excess {
                break;
            }
        }
    }
    removed
}

/// Remove up to 100 of the oldest finished proposals beyond the retained count.
#[ic_cdk::update]
fn prune() -> u32 {
    if store::proposal_count() <= RETAINED_PROPOSALS {
        return 0;
    }
    let at = now();
    let mut w = store::wallet();
    let removed = prune_finished(&mut w, at, 100);
    store::save(w);
    removed
}
