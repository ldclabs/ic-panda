//! The formal channel's public control contract (implementation design §7):
//! a signed genesis and consecutive signed control events define members,
//! roles, invitations, history grants and epochs. The relay orders and
//! enforces them; every client replays them independently with the rules
//! here, so a relay cannot invent a member or a role.

use crate::codec::digest;
use dmsg_types::{ensure, Error, Result, DAY};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Members of a channel, `direct` channels have two.
pub const MAX_MEMBERS: usize = 100;
/// Device recipients of one epoch.
pub const MAX_RECIPIENTS: usize = 600;
/// Open invitations and grants of one channel.
const MAX_OPEN: usize = 100;
/// Epochs one history grant or data grant may span.
const MAX_SPAN: u64 = 1000;

/// What a channel is for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    /// Two people.
    Direct,
    /// Every member may publish.
    Collaboration,
    /// Only owner, admins and publishers publish.
    Distribution,
}

/// A member's role.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The single owner.
    Owner,
    /// Manages ordinary members.
    Admin,
    /// Publishes in a distribution channel.
    Publisher,
    /// Reads; publishes in a collaboration channel.
    Member,
}

/// Where the channel's keys stand.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Membership changed: no new messages until a new epoch is active.
    RotationRequired,
    /// The current epoch is active.
    Active,
    /// Closed by its owner.
    Archived,
}

/// A member.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// Account ID text.
    pub account: String,
    /// Role.
    pub role: Role,
    /// The first epoch the member reads without a history grant.
    pub joined_epoch: u64,
    /// May coordinate an epoch rotation.
    pub can_rotate: bool,
    /// Older epoch ranges granted to the member, inclusive.
    pub history: Vec<(u64, u64)>,
}

/// An open invitation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    /// The invited account.
    pub account: String,
    /// `member` or `publisher`.
    pub role: Role,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// A data grant to an application.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DataGrant {
    /// Grant ID, hex.
    pub grant_id: String,
    /// The granting account.
    pub issuer: String,
    /// The application origin.
    pub origin: String,
    /// The application key's digest, hex.
    pub grantee_key: String,
    /// The channel or file, hex.
    pub resource: String,
    /// `channel.read`, `channel.publish` or `file.read`.
    pub actions: Vec<String>,
    /// First epoch.
    pub epoch_min: u64,
    /// Last epoch.
    pub epoch_max: u64,
    /// Unix milliseconds.
    pub expires_at: u64,
    /// Revoked by a later control.
    pub revoked: bool,
}

/// A channel as its control log defines it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    /// Channel ID, hex.
    pub channel_id: String,
    /// Channel type.
    #[serde(rename = "type")]
    pub kind: ChannelType,
    /// The owner's account.
    pub owner: String,
    /// SHA-256 of the latest control command, hex.
    pub head: String,
    /// Sequence of the latest control; 0 is the genesis.
    pub control_seq: u64,
    /// Version of the access list.
    pub acl_version: u64,
    /// The current epoch; 0 before the first.
    pub epoch: u64,
    /// Key status.
    pub status: Status,
    /// Unused; kept for the relay's state shape.
    pub manifest: Option<String>,
    /// Members by account.
    pub members: BTreeMap<String, Member>,
    /// Open invitations by ID.
    pub invitations: BTreeMap<String, Invitation>,
    /// Data grants by ID.
    pub grants: BTreeMap<String, DataGrant>,
}

/// `dmsg/channel/genesis/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Genesis {
    /// Channel ID, hex.
    pub channel_id: String,
    /// Random nonce, hex.
    pub nonce: String,
    /// Channel type.
    #[serde(rename = "type")]
    pub kind: ChannelType,
}

/// One control action.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Invite an account.
    Invite {
        /// Invitation ID, hex.
        invitation_id: String,
        /// The invited account.
        account: String,
        /// `member` or `publisher`.
        role: Role,
        /// Unix milliseconds, at most seven days ahead.
        expires_at: u64,
    },
    /// Accept an invitation; the invited account signs it.
    Accept {
        /// Invitation ID, hex.
        invitation_id: String,
    },
    /// Change a member's role; owner only.
    Role {
        /// The member.
        account: String,
        /// `admin`, `member` or `publisher`.
        role: Role,
        /// May coordinate rotations.
        can_rotate: bool,
    },
    /// Remove a member.
    Remove {
        /// The member.
        account: String,
    },
    /// Leave the channel.
    Leave {},
    /// Close the channel; owner only.
    Archive {},
    /// Hand ownership to a member, who accepted the exact request.
    Transfer {
        /// The new owner.
        account: String,
        /// The new owner's accepting device, hex.
        device: String,
        /// That device's acceptance signature, base64url.
        signature: String,
    },
    /// Give a member keys of older epochs.
    History {
        /// The member.
        account: String,
        /// First epoch.
        from_epoch: u64,
        /// Last epoch.
        to_epoch: u64,
        /// The upload holding the sealed keys.
        envelope_upload: String,
    },
    /// Grant an application access.
    Grant {
        /// The grant.
        grant: DataGrant,
    },
    /// Revoke a data grant.
    RevokeGrant {
        /// Grant ID, hex.
        grant_id: String,
    },
}

/// `dmsg/channel/control/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Control {
    /// The head it builds on.
    pub expected_head: String,
    /// The access-list version it builds on.
    pub expected_acl_version: u64,
    /// What it does.
    pub action: Action,
}

/// A device receiving an epoch key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Recipient {
    /// `device:<account>:<device>`.
    pub recipient: String,
    /// X25519 public key, hex.
    pub hpke_pub: String,
}

/// A rotation lease the relay grants one coordinator device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    /// Lease ID, hex.
    pub id: String,
    /// The coordinator's account.
    pub account: String,
    /// The coordinator's device, hex.
    pub device: String,
    /// The epoch to activate.
    pub epoch: u64,
    /// The head it rotates from.
    pub head: String,
    /// Digest of the exact recipients.
    pub recipients_digest: String,
    /// Security epoch of each member's account.
    pub security_versions: BTreeMap<String, u64>,
    /// Fencing counter.
    pub fencing: u64,
    /// Unix milliseconds.
    pub expires_at: u64,
}

/// Where one recipient's envelope is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeRef {
    /// `device:<account>:<device>`.
    pub recipient: String,
    /// SHA-256 of the envelope, hex.
    pub digest: String,
    /// Envelope length.
    pub size: u64,
    /// The upload holding it.
    pub upload_id: String,
    /// Its chunk index.
    pub chunk: u64,
}

/// `dmsg/channel/activate/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Activation {
    /// The lease.
    pub lease_id: String,
    /// The lease's fencing counter.
    pub fencing: u64,
    /// The head it rotates from.
    pub expected_head: String,
    /// The new epoch.
    pub epoch: u64,
    /// Digest of the exact recipients.
    pub recipients_digest: String,
    /// The upload holding the epoch manifest.
    pub manifest_upload: String,
    /// Key confirmation, hex.
    pub key_confirmation: String,
    /// One envelope per recipient.
    pub envelopes: Vec<EnvelopeRef>,
}

impl Activation {
    /// Check the payload's bounds.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` outside them.
    pub fn validate(&self) -> Result<()> {
        let hex = |s: &str| is_hash(s);
        ensure(
            hex(&self.lease_id)
                && self.fencing >= 1
                && hex(&self.expected_head)
                && self.epoch >= 1
                && hex(&self.recipients_digest)
                && hex(&self.manifest_upload)
                && hex(&self.key_confirmation)
                && !self.envelopes.is_empty()
                && self.envelopes.len() <= MAX_RECIPIENTS
                && self.envelopes.iter().all(|e| {
                    !e.recipient.is_empty()
                        && e.recipient.len() <= 220
                        && hex(&e.digest)
                        && (1..=2048).contains(&e.size)
                        && hex(&e.upload_id)
                        && e.chunk <= 127
                }),
            Error::IntegrityFailed,
        )
    }
}

/// `dmsg/channel/message/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MessagePayload {
    /// Channel ID, hex.
    pub channel_id: String,
    /// The epoch whose key sealed it.
    pub epoch: u64,
    /// The head it was written against.
    pub control_head: String,
    /// The access-list version it was written against.
    pub acl_version: u64,
    /// Message ID, hex.
    pub message_id: String,
    /// The author's clock, Unix milliseconds.
    pub client_created_at: u64,
    /// The sealed body, base64url.
    pub ciphertext: String,
}

/// `dmsg/channel/reactivate/v1`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reactivate {
    /// The head it builds on.
    pub expected_head: String,
    /// The billing version it builds on.
    pub expected_billing_version: u64,
}

/// A lowercase 64-character hex ID.
pub fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A canonical account ID text.
fn is_account(text: &str) -> bool {
    text.parse::<dmsg_types::AccountId>()
        .is_ok_and(|a| a.to_string() == text)
}

/// `dmsg/channel/recipients/v1` over sorted recipients, hex.
pub fn recipients_digest(recipients: &[Recipient]) -> String {
    hex::encode(*digest("dmsg/channel/recipients/v1", &recipients))
}

/// Whether `account` may read `epoch`: from the epoch it joined, or within
/// a granted history range.
pub fn readable(ledger: &Ledger, account: &str, epoch: u64) -> bool {
    ledger.members.get(account).is_some_and(|member| {
        epoch > 0
            && epoch <= ledger.epoch
            && (epoch >= member.joined_epoch
                || member
                    .history
                    .iter()
                    .any(|(a, b)| epoch >= *a && epoch <= *b))
    })
}

/// Whether `account` may publish now.
pub fn may_publish(ledger: &Ledger, account: &str) -> bool {
    ledger.status == Status::Active
        && ledger
            .members
            .get(account)
            .is_some_and(|m| ledger.kind != ChannelType::Distribution || m.role != Role::Member)
}

/// The channel a genesis by `creator` starts.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for malformed IDs.
pub fn start(genesis: &Genesis, creator: &str, head: &str) -> Result<Ledger> {
    ensure(
        is_hash(&genesis.channel_id) && is_hash(&genesis.nonce) && is_hash(head),
        Error::IntegrityFailed,
    )?;
    Ok(Ledger {
        channel_id: genesis.channel_id.clone(),
        kind: genesis.kind,
        owner: creator.into(),
        head: head.into(),
        control_seq: 0,
        acl_version: 0,
        epoch: 0,
        status: Status::RotationRequired,
        manifest: None,
        members: BTreeMap::from([(
            creator.to_string(),
            Member {
                account: creator.into(),
                role: Role::Owner,
                joined_epoch: 1,
                can_rotate: true,
                history: vec![],
            },
        )]),
        invitations: BTreeMap::new(),
        grants: BTreeMap::new(),
    })
}

fn check_control(control: &Control) -> Result<()> {
    let ok = is_hash(&control.expected_head)
        && match &control.action {
            Action::Invite {
                invitation_id,
                account,
                role,
                ..
            } => {
                is_hash(invitation_id)
                    && is_account(account)
                    && matches!(role, Role::Member | Role::Publisher)
            }
            Action::Accept { invitation_id } => is_hash(invitation_id),
            Action::Role { account, role, .. } => is_account(account) && *role != Role::Owner,
            Action::Remove { account } => is_account(account),
            Action::Leave {} | Action::Archive {} => true,
            Action::Transfer {
                account,
                device,
                signature,
            } => is_account(account) && is_hash(device) && signature.len() <= 90,
            Action::History {
                account,
                from_epoch,
                to_epoch,
                envelope_upload,
            } => {
                is_account(account)
                    && *from_epoch >= 1
                    && *to_epoch >= 1
                    && is_hash(envelope_upload)
            }
            Action::Grant { grant } => {
                is_hash(&grant.grant_id)
                    && is_account(&grant.issuer)
                    && grant.origin.len() <= 256
                    && url::Url::parse(&grant.origin).is_ok()
                    && is_hash(&grant.grantee_key)
                    && is_hash(&grant.resource)
                    && (1..=3).contains(&grant.actions.len())
                    && grant.actions.iter().all(|a| {
                        matches!(a.as_str(), "channel.read" | "channel.publish" | "file.read")
                    })
                    && grant.epoch_min >= 1
                    && grant.epoch_max >= 1
                    && !grant.revoked
            }
            Action::RevokeGrant { grant_id } => is_hash(grant_id),
        };
    ensure(ok, Error::IntegrityFailed)
}

/// The channel after `control` by `actor`, which the relay accepted at
/// `accepted_at` with head `head`. The caller has verified the event's
/// signature and, for a transfer, the target's acceptance.
///
/// # Errors
/// Returns `Error::IntegrityFailed` when it does not build on `previous`
/// and `Error::Forbidden` when the rules refuse it.
pub fn advance(
    previous: &Ledger,
    control: &Control,
    actor: &str,
    head: &str,
    accepted_at: u64,
) -> Result<Ledger> {
    check_control(control)?;
    ensure(
        previous.status != Status::Archived
            && control.expected_head == previous.head
            && control.expected_acl_version == previous.acl_version,
        Error::IntegrityFailed,
    )?;
    let mut state = previous.clone();
    state.invitations.retain(|_, v| v.expires_at > accepted_at);
    state
        .grants
        .retain(|_, v| !v.revoked && v.expires_at > accepted_at);
    let me = state.members.get(actor).cloned();
    ensure(
        matches!(control.action, Action::Accept { .. }) || me.is_some(),
        Error::Forbidden,
    )?;
    let role = me.as_ref().map(|m| m.role);
    let manager = matches!(role, Some(Role::Owner | Role::Admin));
    let forbid = |ok: bool| ensure(ok, Error::Forbidden);
    let remove_invites = |state: &mut Ledger, account: &str| {
        state.invitations.retain(|_, v| v.account != account);
    };
    match &control.action {
        Action::Invite {
            invitation_id,
            account,
            role,
            expires_at,
        } => {
            forbid(
                manager
                    && account != actor
                    && !state.members.contains_key(account)
                    && !state.invitations.contains_key(invitation_id)
                    && state.invitations.len() < MAX_OPEN
                    && *expires_at > accepted_at
                    && *expires_at <= accepted_at + 7 * DAY,
            )?;
            state.invitations.insert(
                invitation_id.clone(),
                Invitation {
                    account: account.clone(),
                    role: *role,
                    expires_at: *expires_at,
                },
            );
        }
        Action::Accept { invitation_id } => {
            let limit = if state.kind == ChannelType::Direct {
                2
            } else {
                MAX_MEMBERS
            };
            let invitation = state
                .invitations
                .get(invitation_id)
                .filter(|i| i.account == actor)
                .cloned();
            let Some(invitation) = invitation else {
                return Err(Error::Forbidden);
            };
            forbid(me.is_none() && state.members.len() < limit)?;
            state.members.insert(
                actor.into(),
                Member {
                    account: actor.into(),
                    role: invitation.role,
                    joined_epoch: state.epoch + 1,
                    can_rotate: false,
                    history: vec![],
                },
            );
            remove_invites(&mut state, actor);
            state.status = Status::RotationRequired;
        }
        Action::Role {
            account,
            role,
            can_rotate,
        } => {
            let target = state.members.get_mut(account);
            forbid(
                role_is(me.as_ref(), Role::Owner)
                    && target.as_ref().is_some_and(|t| t.role != Role::Owner),
            )?;
            let target = target.expect("checked");
            target.role = *role;
            target.can_rotate = *can_rotate;
        }
        Action::Remove { account } => {
            let target = state.members.get(account).map(|t| t.role);
            forbid(
                manager
                    && account != actor
                    && target.is_some_and(|t| {
                        t != Role::Owner
                            && (role == Some(Role::Owner)
                                || matches!(t, Role::Member | Role::Publisher))
                    }),
            )?;
            state.members.remove(account);
            remove_invites(&mut state, account);
            state.status = Status::RotationRequired;
        }
        Action::Leave {} => {
            forbid(role != Some(Role::Owner))?;
            state.members.remove(actor);
            remove_invites(&mut state, actor);
            state.status = Status::RotationRequired;
        }
        Action::Transfer { account, .. } => {
            forbid(
                role == Some(Role::Owner)
                    && account != actor
                    && state.members.contains_key(account),
            )?;
            let target = state.members.get_mut(account).expect("checked");
            target.role = Role::Owner;
            target.can_rotate = true;
            state.members.get_mut(actor).expect("member").role = Role::Admin;
            state.owner = account.clone();
        }
        Action::Archive {} => {
            forbid(role == Some(Role::Owner))?;
            state.status = Status::Archived;
        }
        Action::History {
            account,
            from_epoch,
            to_epoch,
            ..
        } => {
            forbid(
                manager
                    && state
                        .members
                        .get(account)
                        .is_some_and(|t| t.history.len() < 64)
                    && from_epoch <= to_epoch
                    && *to_epoch <= state.epoch
                    && to_epoch - from_epoch < MAX_SPAN,
            )?;
            for epoch in *from_epoch..=*to_epoch {
                forbid(readable(&state, actor, epoch))?;
            }
            state
                .members
                .get_mut(account)
                .expect("checked")
                .history
                .push((*from_epoch, *to_epoch));
        }
        Action::Grant { grant } => {
            forbid(
                manager
                    && grant.issuer == actor
                    && !state.grants.contains_key(&grant.grant_id)
                    && state.grants.len() < MAX_OPEN
                    && grant.epoch_min <= grant.epoch_max
                    && grant.epoch_max - grant.epoch_min < MAX_SPAN
                    && grant.expires_at > accepted_at
                    && grant.expires_at <= accepted_at + 30 * DAY,
            )?;
            for epoch in grant.epoch_min..=grant.epoch_max {
                forbid(readable(&state, actor, epoch))?;
            }
            state.grants.insert(grant.grant_id.clone(), grant.clone());
        }
        Action::RevokeGrant { grant_id } => {
            let grant = state.grants.get_mut(grant_id);
            forbid(manager && grant.is_some())?;
            grant.expect("checked").revoked = true;
        }
    }
    state.control_seq += 1;
    state.acl_version += 1;
    state.head = head.into();
    Ok(state)
}

fn role_is(member: Option<&Member>, role: Role) -> bool {
    member.is_some_and(|m| m.role == role)
}

#[cfg(test)]
#[path = "channel_log_tests.rs"]
mod tests;
