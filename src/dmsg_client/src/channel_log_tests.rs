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
    let invited = apply(
        ledger,
        OWNER,
        Action::Invite {
            invitation_id: h(id),
            account: account.into(),
            role: Role::Member,
            expires_at: 2_000,
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
