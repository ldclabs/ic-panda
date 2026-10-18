use super::*;
use crate::vectors::{load, text};

const OWNER: &str = "aaaaaaaaaaaaaaaaaaa0";
const MEMBER: &str = "bbbbbbbbbbbbbbbbbbb0";
const OTHER: &str = "ccccccccccccccccccc0";

fn h(n: u8) -> String {
    hex::encode([n; 32])
}

fn genesis(kind: ChannelType) -> Ledger {
    start(
        &Genesis {
            channel_id: h(1),
            nonce: h(2),
            kind,
        },
        OWNER,
        &h(3),
    )
    .unwrap()
}

fn apply(ledger: &Ledger, actor: &str, action: Action, head: u8) -> Result<Ledger> {
    advance(
        ledger,
        &Control {
            expected_head: ledger.head.clone(),
            expected_acl_version: ledger.acl_version,
            action,
        },
        actor,
        &h(head),
        1_000,
    )
}

fn activated(ledger: &Ledger) -> Ledger {
    Ledger {
        epoch: ledger.epoch + 1,
        status: Status::Active,
        control_seq: ledger.control_seq + 1,
        ..ledger.clone()
    }
}

fn invite(ledger: &Ledger, account: &str, id: u8) -> Ledger {
    invite_sealed(ledger, account, id, None)
}

/// An invitation and its acceptance; `envelope` seals the current epoch.
fn invite_sealed(ledger: &Ledger, account: &str, id: u8, envelope: Option<String>) -> Ledger {
    let invited = apply(
        ledger,
        OWNER,
        Action::Invite {
            invitation_id: h(id),
            account: account.into(),
            role: Role::Member,
            expires_at: 2_000,
            envelope_upload: envelope,
        },
        id + 100,
    )
    .unwrap();
    apply(
        &invited,
        account,
        Action::Accept {
            invitation_id: h(id),
        },
        id + 101,
    )
    .unwrap()
}

#[test]
fn members_join_the_next_epoch_and_rotation_follows() {
    let ledger = activated(&genesis(ChannelType::Collaboration));
    assert!(readable(&ledger, OWNER, 1));
    let joined = invite(&ledger, MEMBER, 10);
    assert_eq!(joined.status, Status::RotationRequired);
    assert_eq!(joined.members[MEMBER].joined_epoch, 2);
    assert_eq!((joined.control_seq, joined.acl_version), (3, 2));
    assert!(joined.invitations.is_empty());
    let next = activated(&joined);
    assert!(!readable(&next, MEMBER, 1) && readable(&next, MEMBER, 2));
    // History for epoch 1 makes it readable.
    let granted = apply(
        &next,
        OWNER,
        Action::History {
            account: MEMBER.into(),
            from_epoch: 1,
            to_epoch: 1,
            envelope_upload: h(9),
        },
        30,
    )
    .unwrap();
    assert!(readable(&granted, MEMBER, 1));
    assert!(may_publish(&granted, MEMBER));
}

#[test]
fn a_sealed_invitation_joins_the_current_epoch_without_a_rotation() {
    let ledger = activated(&genesis(ChannelType::Collaboration));
    let joined = invite_sealed(&ledger, MEMBER, 10, Some(h(9)));
    assert_eq!(joined.status, Status::Active);
    assert_eq!(joined.members[MEMBER].joined_epoch, 1);
    assert!(readable(&joined, MEMBER, 1) && may_publish(&joined, MEMBER));
    // An invitation is accepted only in the epoch it was issued in.
    let invited = apply(
        &ledger,
        OWNER,
        Action::Invite {
            invitation_id: h(11),
            account: OTHER.into(),
            role: Role::Member,
            expires_at: 2_000,
            envelope_upload: Some(h(9)),
        },
        12,
    )
    .unwrap();
    assert_eq!(invited.invitations[&h(11)].epoch, 1);
    let accept = Action::Accept {
        invitation_id: h(11),
    };
    assert_eq!(
        apply(&activated(&invited), OTHER, accept.clone(), 13),
        Err(Error::Forbidden)
    );
    assert!(apply(&invited, OTHER, accept, 13).is_ok());
    // Nothing is sealed before the first epoch.
    assert_eq!(
        apply(
            &genesis(ChannelType::Collaboration),
            OWNER,
            Action::Invite {
                invitation_id: h(11),
                account: OTHER.into(),
                role: Role::Member,
                expires_at: 2_000,
                envelope_upload: Some(h(9)),
            },
            12,
        ),
        Err(Error::Forbidden)
    );
}

#[test]
fn owners_admins_and_allowed_members_rekey() {
    let ledger = activated(&invite_sealed(
        &invite_sealed(
            &activated(&genesis(ChannelType::Collaboration)),
            MEMBER,
            10,
            Some(h(9)),
        ),
        OTHER,
        20,
        Some(h(9)),
    ));
    assert!(may_rotate(&ledger, OWNER));
    assert!(!may_rotate(&ledger, MEMBER));
    assert_eq!(
        apply(&ledger, MEMBER, Action::Rekey {}, 30),
        Err(Error::Forbidden)
    );
    let rekeyed = apply(&ledger, OWNER, Action::Rekey {}, 30).unwrap();
    assert_eq!(rekeyed.status, Status::RotationRequired);
    assert_eq!(rekeyed.epoch, ledger.epoch);
    // An admin may, and so may a member the owner allows.
    let admin = apply(
        &ledger,
        OWNER,
        Action::Role {
            account: MEMBER.into(),
            role: Role::Admin,
            can_rotate: false,
        },
        31,
    )
    .unwrap();
    assert!(may_rotate(&admin, MEMBER));
    let allowed = apply(
        &admin,
        OWNER,
        Action::Role {
            account: OTHER.into(),
            role: Role::Member,
            can_rotate: true,
        },
        32,
    )
    .unwrap();
    assert!(may_rotate(&allowed, OTHER));
    assert!(apply(&allowed, OTHER, Action::Rekey {}, 33).is_ok());
    // A member waiting for the next epoch does not coordinate it.
    let waiting = invite(&ledger, "ddddddddddddddddddd0", 40);
    assert!(!may_rotate(
        &Ledger {
            members: BTreeMap::from([(
                "ddddddddddddddddddd0".to_string(),
                Member {
                    role: Role::Admin,
                    ..waiting.members["ddddddddddddddddddd0"].clone()
                },
            )]),
            ..waiting.clone()
        },
        "ddddddddddddddddddd0"
    ));
}

#[test]
fn the_rules_refuse_what_the_extension_refused() {
    let ledger = activated(&genesis(ChannelType::Distribution));
    let joined = activated(&invite(&ledger, MEMBER, 10));
    // A member cannot invite, a member does not publish in a distribution.
    assert_eq!(
        apply(
            &joined,
            MEMBER,
            Action::Invite {
                invitation_id: h(11),
                account: OTHER.into(),
                role: Role::Member,
                expires_at: 2_000,
                envelope_upload: None,
            },
            12,
        ),
        Err(Error::Forbidden)
    );
    assert!(!may_publish(&joined, MEMBER));
    // An invitation beyond seven days, or for another account, is refused.
    assert_eq!(
        apply(
            &joined,
            OWNER,
            Action::Invite {
                invitation_id: h(11),
                account: OTHER.into(),
                role: Role::Member,
                expires_at: 1_000 + 8 * DAY,
                envelope_upload: None,
            },
            12,
        ),
        Err(Error::Forbidden)
    );
    let invited = apply(
        &joined,
        OWNER,
        Action::Invite {
            invitation_id: h(11),
            account: OTHER.into(),
            role: Role::Publisher,
            expires_at: 2_000,
            envelope_upload: None,
        },
        12,
    )
    .unwrap();
    assert_eq!(
        apply(
            &invited,
            MEMBER,
            Action::Accept {
                invitation_id: h(11)
            },
            13
        ),
        Err(Error::Forbidden)
    );
    // The owner neither leaves nor is removed; only the owner archives.
    assert_eq!(
        apply(&joined, OWNER, Action::Leave {}, 14),
        Err(Error::Forbidden)
    );
    assert_eq!(
        apply(
            &joined,
            MEMBER,
            Action::Remove {
                account: OWNER.into()
            },
            14
        ),
        Err(Error::Forbidden)
    );
    assert_eq!(
        apply(&joined, MEMBER, Action::Archive {}, 14),
        Err(Error::Forbidden)
    );
    let archived = apply(&joined, OWNER, Action::Archive {}, 14).unwrap();
    assert_eq!(archived.status, Status::Archived);
    assert_eq!(
        apply(&archived, MEMBER, Action::Leave {}, 15),
        Err(Error::IntegrityFailed)
    );
    // A stale head never applies.
    let stale = Control {
        expected_head: h(77),
        expected_acl_version: joined.acl_version,
        action: Action::Leave {},
    };
    assert_eq!(
        advance(&joined, &stale, MEMBER, &h(16), 1_000),
        Err(Error::IntegrityFailed)
    );
    // History beyond the current epoch or of an unread epoch is refused.
    assert_eq!(
        apply(
            &joined,
            OWNER,
            Action::History {
                account: MEMBER.into(),
                from_epoch: 1,
                to_epoch: 3,
                envelope_upload: h(9),
            },
            17,
        ),
        Err(Error::Forbidden)
    );
}

#[test]
fn ownership_moves_and_a_removed_member_leaves_the_access_list() {
    let ledger = activated(&invite(
        &activated(&genesis(ChannelType::Collaboration)),
        MEMBER,
        10,
    ));
    let moved = apply(
        &ledger,
        OWNER,
        Action::Transfer {
            account: MEMBER.into(),
            device: h(5),
            signature: "c2ln".into(),
        },
        20,
    )
    .unwrap();
    assert_eq!(moved.owner, MEMBER);
    assert_eq!(moved.members[OWNER].role, Role::Admin);
    assert_eq!(moved.members[MEMBER].role, Role::Owner);
    assert_eq!(moved.status, Status::Active);
    let removed = apply(
        &moved,
        MEMBER,
        Action::Remove {
            account: OWNER.into(),
        },
        21,
    )
    .unwrap();
    assert!(!removed.members.contains_key(OWNER));
    assert_eq!(removed.status, Status::RotationRequired);
    assert!(!readable(&removed, OWNER, 1));
}

#[test]
fn control_payloads_keep_the_wire_shape() {
    let control: Control = serde_json::from_value(serde_json::json!({
        "expected_head": h(1),
        "expected_acl_version": 0,
        "action": {"type": "leave"}
    }))
    .unwrap();
    assert_eq!(control.action, Action::Leave {});
    let rekey: Action = serde_json::from_value(serde_json::json!({"type": "rekey"})).unwrap();
    assert_eq!(rekey, Action::Rekey {});
    // An invitation without an envelope keeps the shape it had.
    let invite = Action::Invite {
        invitation_id: h(2),
        account: MEMBER.into(),
        role: Role::Member,
        expires_at: 1,
        envelope_upload: None,
    };
    assert!(
        serde_json::to_value(&invite)
            .unwrap()
            .get("envelope_upload")
            .is_none()
    );
    let bytes = crate::codec::canonical(&control);
    let back: Control = crate::codec::decode(&bytes, 4096).unwrap();
    assert_eq!(back, control);
    // Unknown fields are refused.
    assert!(
        serde_json::from_value::<Control>(serde_json::json!({
            "expected_head": h(1),
            "expected_acl_version": 0,
            "action": {"type": "leave", "extra": 1}
        }))
        .is_err()
    );
}

#[test]
fn recipients_digest_matches_the_vector() {
    let v = load("channel-v1.json");
    let recipients: Vec<Recipient> = serde_json::from_value(v["recipients"].clone()).unwrap();
    assert_eq!(
        recipients_digest(&recipients),
        text(&v["lease"]["recipients_digest"])
    );
}
