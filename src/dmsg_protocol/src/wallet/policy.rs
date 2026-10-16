//! Roles, intent classes, approval requirements and shape checks. A client
//! evaluates a draft with the same functions the wallet applies.
use dmsg_types::{wallet::*, *};

/// Approval class of an intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentClass {
    /// Moves an amount of an asset; tiers and limits apply.
    Spend {
        /// Asset.
        asset: AssetKey,
        /// Amount, all outputs together.
        amount: u128,
    },
    /// Spends nothing measured by tiers; needs the Approver threshold.
    Operate,
    /// Changes the wallet; needs the Admin threshold after `delay_ms`.
    Admin {
        /// Veto delay.
        delay_ms: u64,
    },
}

/// The class of `intent` under `policy`.
pub fn classify(intent: &Intent, policy: &Policy) -> IntentClass {
    match intent {
        Intent::Transfer { asset, amount, .. } => IntentClass::Spend {
            asset: asset.clone(),
            amount: *amount,
        },
        Intent::Batch { asset, items, .. } => IntentClass::Spend {
            asset: asset.clone(),
            amount: items
                .iter()
                .map(|i| i.amount)
                .fold(0u128, u128::saturating_add),
        },
        Intent::Refuel { icp_e8s, .. } => IntentClass::Spend {
            asset: AssetKey::Icp,
            amount: u128::from(*icp_e8s),
        },
        Intent::CanisterCall { .. }
        | Intent::CancelEvmNonce { .. }
        | Intent::InitSolNonce { .. }
        | Intent::AddressBook(_) => IntentClass::Operate,
        Intent::AddAddress { .. }
        | Intent::AddCosigner(_)
        | Intent::AddMember(_)
        | Intent::UpdateMember(_) => IntentClass::Admin { delay_ms: 0 },
        Intent::SetPolicy(_)
        | Intent::RemoveMember(_)
        | Intent::SetCosign { .. }
        | Intent::RemoveCosigner(_)
        | Intent::AcceptUpgrade { .. }
        | Intent::LockCode
        | Intent::ReleaseEvmNonce { .. } => IntentClass::Admin {
            delay_ms: SENSITIVE_DELAY_MS,
        },
        Intent::Unfreeze => IntentClass::Admin {
            delay_ms: policy.unfreeze_delay_ms,
        },
    }
}

/// Whether the intent changes membership, the only kind Recovery proposes.
pub fn is_membership(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::AddMember(_) | Intent::RemoveMember(_) | Intent::UpdateMember(_)
    )
}

/// Whether `role` may propose `intent`.
pub fn can_propose(role: Role, intent: &Intent) -> bool {
    match role {
        Role::Admin | Role::Approver | Role::Proposer => true,
        Role::Recovery => is_membership(intent),
        Role::Viewer => false,
    }
}

/// Whether a decision by `role` counts toward a requirement.
pub fn counts(role: Role, requirement: &Requirement) -> bool {
    if requirement.admin {
        role == Role::Admin
    } else {
        matches!(role, Role::Admin | Role::Approver)
    }
}

/// Whether `role` may veto during a delay or freeze the wallet.
pub fn can_veto(role: Role) -> bool {
    matches!(role, Role::Admin | Role::Approver)
}

/// Active members able to approve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Voters {
    /// Admins.
    pub admins: u8,
    /// Admins and Approvers.
    pub approvers: u8,
}

impl Voters {
    /// Count the active voters among `members`.
    pub fn of(members: &[Member]) -> Self {
        let active = |r: &[Role]| {
            members
                .iter()
                .filter(|m| m.status == MemberStatus::Active && r.contains(&m.role))
                .count()
                .min(255) as u8
        };
        Self {
            admins: active(&[Role::Admin]),
            approvers: active(&[Role::Admin, Role::Approver]),
        }
    }
}

/// The requirement of a proposal by a `proposer` member.
///
/// `window_amount` is the asset's rolling 24-hour amount of approved and
/// executed proposals plus this one; `in_book` says whether every recipient
/// is an address-book entry past its cooldown.
///
/// # Errors
/// Returns `Error::Forbidden` when the role may not propose it, the asset is
/// not spendable, or a book-only tier meets a recipient outside the book.
pub fn requirement(
    policy: &Policy,
    intent: &Intent,
    proposer: Role,
    voters: Voters,
    window_amount: u128,
    in_book: bool,
) -> Result<Requirement> {
    ensure(can_propose(proposer, intent), Error::Forbidden)?;
    if proposer == Role::Recovery {
        // Social recovery: no approvals, a long delay any Approver may veto.
        return Ok(Requirement {
            approvals: 0,
            admin: true,
            delay_ms: RECOVERY_DELAY_MS,
            window_amount: 0,
        });
    }
    Ok(match classify(intent, policy) {
        IntentClass::Spend { asset, .. } => {
            ensure(policy.spendable_assets.contains(&asset), Error::Forbidden)?;
            let tiers: Vec<&Tier> = policy.tiers.iter().filter(|t| t.asset == asset).collect();
            let ceiling = voters.approvers.max(1);
            let (mut approvals, mut delay_ms, book_only) =
                match tiers.iter().find(|t| t.up_to >= window_amount) {
                    Some(t) => (t.approvals, t.delay_ms, t.book_only),
                    None => match tiers.last() {
                        Some(top) => (
                            top.approvals
                                .saturating_add(1)
                                .min(ceiling)
                                .max(top.approvals),
                            top.delay_ms,
                            top.book_only,
                        ),
                        None => (policy.approver_threshold, 0, false),
                    },
                };
            if policy
                .daily_limits
                .iter()
                .any(|l| l.asset == asset && window_amount > l.amount)
            {
                approvals = approvals.max(ceiling);
                delay_ms = tiers
                    .iter()
                    .map(|t| t.delay_ms)
                    .max()
                    .unwrap_or(0)
                    .max(delay_ms);
            }
            ensure(!book_only || in_book, Error::Forbidden)?;
            Requirement {
                approvals: approvals.max(1),
                admin: false,
                delay_ms,
                window_amount,
            }
        }
        IntentClass::Operate => Requirement {
            approvals: policy.approver_threshold,
            admin: false,
            delay_ms: 0,
            window_amount: 0,
        },
        IntentClass::Admin { delay_ms } => Requirement {
            approvals: policy.admin_threshold,
            admin: true,
            delay_ms,
            window_amount: 0,
        },
    })
}

fn label(text: &str) -> Result<()> {
    ensure_valid(text.len() <= 64, "label")
}

/// Check a member as proposed.
///
/// # Errors
/// Returns `Error::InvalidInput` for an oversized label or agent list.
pub fn validate_member(input: &MemberInput) -> Result<()> {
    label(&input.label)?;
    ensure_valid(
        input.agent_ids.len() <= MAX_MEMBER_AGENTS
            && input
                .agent_ids
                .iter()
                .all(|a| !a.is_empty() && a.len() <= 128),
        "member agents",
    )?;
    if let MemberId::Principal(p) = &input.id {
        crate::authenticated(*p)?;
    }
    Ok(())
}

/// Check a policy; `admins` and `approvers` count every member, invited or
/// active, with such a role, so thresholds can be met once all join.
///
/// # Errors
/// Returns `Error::InvalidInput` naming the first violated bound.
pub fn validate_policy(policy: &Policy, admins: usize, approvers: usize) -> Result<()> {
    ensure_valid(
        policy.admin_threshold >= 1 && usize::from(policy.admin_threshold) <= admins,
        "admin threshold",
    )?;
    ensure_valid(
        policy.approver_threshold >= 1 && usize::from(policy.approver_threshold) <= approvers,
        "approver threshold",
    )?;
    ensure_valid(
        policy.tiers.len() <= 64
            && policy.daily_limits.len() <= 32
            && policy.spendable_assets.len() <= 64
            && policy.call_targets.len() <= 64
            && policy.evm_chains.len() <= 16,
        "policy lists",
    )?;
    for (i, t) in policy.tiers.iter().enumerate() {
        ensure_valid(
            t.approvals >= 1 && usize::from(t.approvals) <= approvers,
            "tier approvals",
        )?;
        ensure_valid(t.delay_ms <= 30 * DAY, "tier delay")?;
        // Tiers of an asset ascend strictly, so the first match is the band.
        ensure_valid(
            policy.tiers[..i]
                .iter()
                .filter(|p| p.asset == t.asset)
                .all(|p| p.up_to < t.up_to && p.approvals <= t.approvals),
            "tier order",
        )?;
    }
    ensure_valid(
        (MINUTE * 60..=90 * DAY).contains(&policy.proposal_ttl_ms)
            && policy.unfreeze_delay_ms <= 30 * DAY
            && policy.address_book_cooldown_ms <= 30 * DAY
            && (1..=1_000).contains(&policy.daily_signatures),
        "policy bounds",
    )?;
    if let Some(r) = &policy.refuel {
        ensure_valid(
            r.icp_e8s > 0 && (1..=24).contains(&r.max_per_day),
            "refuel rule",
        )?;
    }
    for c in &policy.call_targets {
        crate::authenticated(*c)?;
    }
    Ok(())
}

/// Check a wallet to create: its name, members besides `creator`, who
/// becomes an Admin, and its first policy.
///
/// # Errors
/// Returns `Error::InvalidInput` naming the first violated bound.
pub fn validate_spec(spec: &WalletSpec, creator: &MemberId) -> Result<()> {
    ensure_valid(
        !spec.name.is_empty() && spec.name.len() <= 64,
        "wallet name",
    )?;
    ensure_valid(spec.members.len() < MAX_MEMBERS, "members")?;
    ensure_valid(
        spec.channel_hint.as_ref().is_none_or(|c| c.len() <= 128),
        "channel hint",
    )?;
    for (i, m) in spec.members.iter().enumerate() {
        validate_member(m)?;
        ensure_valid(
            m.id != *creator && spec.members[..i].iter().all(|x| x.id != m.id),
            "duplicate member",
        )?;
    }
    let role = |r: &[Role]| 1 + spec.members.iter().filter(|m| r.contains(&m.role)).count();
    ensure_valid(spec.policy.version == 1, "policy version")?;
    validate_policy(
        &spec.policy,
        role(&[Role::Admin]),
        role(&[Role::Admin, Role::Approver]),
    )
}

/// Whether a destination suits an asset.
pub fn destination_fits(asset: &AssetKey, to: &Destination) -> bool {
    matches!(
        (asset, to),
        (AssetKey::Icp | AssetKey::Icrc(_), Destination::Icrc(_))
            | (AssetKey::Btc, Destination::Bitcoin(_))
            | (AssetKey::Evm { .. }, Destination::Evm(_))
            | (AssetKey::Sol { .. }, Destination::Solana(_))
    )
}

/// The chain of an asset.
pub fn asset_chain(asset: &AssetKey) -> Chain {
    match asset {
        AssetKey::Icp | AssetKey::Icrc(_) => Chain::Icp,
        AssetKey::Btc => Chain::Bitcoin,
        AssetKey::Evm { .. } => Chain::Evm,
        AssetKey::Sol { .. } => Chain::Solana,
    }
}

/// Check the shape of an intent, independent of wallet state.
///
/// # Errors
/// Returns `Error::InvalidInput` naming the first violated bound.
pub fn validate_intent(intent: &Intent) -> Result<()> {
    match intent {
        Intent::Transfer {
            asset,
            to,
            amount,
            memo,
            ..
        } => {
            ensure_valid(*amount > 0 && destination_fits(asset, to), "transfer")?;
            ensure_valid(
                memo.as_ref().is_none_or(|m| m.len() <= 32)
                    && (memo.is_none() || matches!(asset, AssetKey::Icp | AssetKey::Icrc(_))),
                "memo",
            )?;
            validate_destination(to)
        }
        Intent::Batch { asset, items, .. } => {
            ensure_valid(
                !items.is_empty()
                    && items.len() <= MAX_BATCH_ITEMS
                    && !matches!(asset, AssetKey::Evm { .. })
                    && items
                        .iter()
                        .all(|i| i.amount > 0 && destination_fits(asset, &i.to)),
                "batch",
            )?;
            items.iter().try_for_each(|i| validate_destination(&i.to))
        }
        Intent::CanisterCall {
            canister,
            method,
            arg,
            ..
        } => {
            crate::authenticated(*canister)?;
            ensure_valid(
                !method.is_empty() && method.len() <= 128 && arg.len() <= MAX_CALL_BYTES,
                "canister call",
            )
        }
        Intent::AddAddress {
            chain,
            kind,
            label: text,
            cosign,
        } => {
            crate::wallet::key_algorithm(*chain, kind)?;
            label(text)?;
            cosign.as_ref().map_or(Ok(()), validate_cosign_policy)
        }
        Intent::SetCosign { cosign, .. } => cosign.as_ref().map_or(Ok(()), validate_cosign_policy),
        Intent::AddCosigner(input) => crate::wallet::validate_cosigner(input),
        Intent::AddMember(input) | Intent::UpdateMember(input) => validate_member(input),
        Intent::AddressBook(BookChange::Add {
            destination,
            label: text,
        }) => {
            label(text)?;
            validate_destination(destination)
        }
        Intent::Refuel { icp_e8s, .. } => ensure_valid(*icp_e8s > 0, "refuel"),
        _ => Ok(()),
    }
}

fn validate_cosign_policy(p: &CosignPolicy) -> Result<()> {
    ensure_valid(
        p.threshold >= 1
            && usize::from(p.threshold) <= p.cosigners.len()
            && p.cosigners.len() <= 16
            && p.cosigners
                .iter()
                .enumerate()
                .all(|(i, c)| !p.cosigners[..i].contains(c)),
        "cosign policy",
    )
}

fn validate_destination(to: &Destination) -> Result<()> {
    match to {
        Destination::Icrc(account) => crate::authenticated(account.owner),
        Destination::Bitcoin(text) => {
            ensure_valid(!text.is_empty() && text.len() <= 90, "bitcoin address")
        }
        Destination::Evm(address) => ensure_valid(address.iter().any(|b| *b != 0), "evm address"),
        Destination::Solana(key) => ensure_valid(key.iter().any(|b| *b != 0), "solana address"),
    }
}
