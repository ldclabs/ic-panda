//! Portable cosignatures bind one wallet, proposal and exact execution plan.
//! Importing a packet only collects signatures; executing is a separate action.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Packet {
    version: u8,
    wallet: Principal,
    proposal: u64,
    digest: Hash,
    plan: u32,
    plan_hash: Hash,
    signatures: Vec<CosignerSig>,
}

fn key(wallet: Principal, id: u64, plan: u32) -> String {
    format!("wallet-cosigs:{wallet}:{id}:{plan}")
}

pub(super) async fn context<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u64,
    seq: u32,
) -> Result<(
    WalletView,
    Proposal,
    Option<ExecutionPlan>,
    Vec<Hash>,
    Vec<CosignRequirement>,
)> {
    let view = state(h, wallet).await?;
    let p = proposal_raw(h, wallet, id).await?;
    let policy = signing_requirements(&view, &p, now_ms())?;
    let plan = if seq == 0 {
        ensure(external_source(&p.intent).is_none(), Error::IntegrityFailed)?;
        None
    } else {
        let (_, ic) = identity(h).await?;
        let read: Result<ExecutionPlan> = ic.query(wallet, "get_plan", (id, seq)).await?;
        let read = read?;
        ensure(
            read.proposal == id && read.seq == seq && now_ms() < read.expires_at_ms,
            Error::Expired,
        )?;
        decode::verify_plan(&view, &p.intent, &read)?;
        Some(read)
    };
    signing_requirements(&view, &p, now_ms())?;
    let digests = plan
        .as_ref()
        .map(|p| p.digests.clone())
        .unwrap_or_else(|| vec![p.digest]);
    Ok((view, p, plan, digests, policy))
}

/// Preconditions shared by request creation and execution after every wait.
pub(super) fn signing_requirements(
    view: &WalletView,
    p: &Proposal,
    now: u64,
) -> Result<Vec<CosignRequirement>> {
    ensure(!view.frozen, Error::Locked)?;
    ensure(
        matches!(
            p.status,
            ProposalStatus::Ready
                | ProposalStatus::Delayed
                | ProposalStatus::Signed
                | ProposalStatus::Broadcast
        ),
        Error::VersionConflict,
    )?;
    if matches!(p.status, ProposalStatus::Ready | ProposalStatus::Delayed) {
        ensure(
            p.policy_version == view.policy.version,
            Error::VersionConflict,
        )?;
        ensure(now < p.expires_at_ms, Error::Expired)?;
    }
    let requirements = cosign_requirements(&p.intent, &view.addresses);
    ensure(!requirements.is_empty(), Error::NotFound)?;
    Ok(requirements)
}

/// Check a packet's context, then keep only the signatures that verify
/// against the cosigners' current keys. Returns the cosigners whose
/// signatures were dropped, such as after their key changed: they sign again.
/// Signatures of cosigners the policy does not name are dropped silently.
fn validate(
    packet: &mut Packet,
    view: &WalletView,
    p: &Proposal,
    plan: Option<&ExecutionPlan>,
    digests: &[Hash],
    policy: &[CosignRequirement],
) -> Result<Vec<u32>> {
    let seq = plan.map_or(0, |p| p.seq);
    ensure(
        packet.version == 1
            && packet.wallet == view.wallet
            && packet.proposal == p.id
            && packet.digest == p.digest
            && packet.plan == seq
            && packet.plan_hash == digest("dmsg/wallet-cosign-packet/v1", &plan),
        Error::IntegrityFailed,
    )?;
    let keys: Vec<_> = view
        .cosigners
        .iter()
        .filter(|c| {
            (!view.policy.hardware_cosigners || c.input.hardware)
                && policy.iter().any(|r| r.policy.cosigners.contains(&c.id))
        })
        .cloned()
        .collect();
    let messages: Vec<_> = digests
        .iter()
        .enumerate()
        .map(|(i, d)| cosign_message(view.wallet, p.id, seq, i as u32, d))
        .collect();
    let mut groups = std::collections::BTreeMap::<u32, Vec<CosignerSig>>::new();
    for sig in packet.signatures.drain(..) {
        groups.entry(sig.cosigner).or_default().push(sig);
    }
    let mut stale = vec![];
    for (id, group) in groups {
        if verify_cosignatures(&keys, &messages, &group).is_ok() {
            packet.signatures.extend(group);
        } else if keys.iter().any(|k| k.id == id) {
            stale.push(id);
        }
    }
    Ok(stale)
}

fn merge(mut a: Packet, b: Packet) -> Packet {
    for sig in b.signatures {
        a.signatures
            .retain(|s| s.cosigner != sig.cosigner || s.digest_index != sig.digest_index);
        a.signatures.push(sig);
    }
    a
}

fn result(
    packet: &Packet,
    digests: &[Hash],
    policy: &[CosignRequirement],
    stale: &[u32],
) -> Result<Json> {
    let requirements: Vec<_> = policy.iter().map(|r| {
        let collected = r.policy.cosigners.iter().filter(|id| {
            (0..digests.len()).all(|i| packet.signatures.iter().any(|s| s.cosigner == **id && s.digest_index as usize == i))
        }).count();
        json!({"address":r.address,"collected":collected,"required":r.policy.threshold,"ready":collected>=usize::from(r.policy.threshold)})
    }).collect();
    Ok(
        json!({"wallet":packet.wallet.to_text(),"proposal":packet.proposal,"plan":packet.plan,
        "collected":requirements.iter().map(|r|r["collected"].as_u64().unwrap_or(0)).sum::<u64>(),
        "required":requirements.iter().map(|r|r["required"].as_u64().unwrap_or(0)).sum::<u64>(),
        "ready":requirements.iter().all(|r|r["ready"]==true),"requirements":requirements,"stale":stale,
        "packet":URL_SAFE_NO_PAD.encode(crate::codec::canonical(packet))}),
    )
}

/// Keep locally approved signatures, returning a packet another device may import.
pub(super) async fn keep<H: Hold>(
    h: &H,
    view: &WalletView,
    p: &Proposal,
    plan: Option<&ExecutionPlan>,
    signatures: Vec<CosignerSig>,
) -> Result<Json> {
    let seq = plan.map_or(0, |p| p.seq);
    let packet = Packet {
        version: 1,
        wallet: view.wallet,
        proposal: p.id,
        digest: p.digest,
        plan: seq,
        plan_hash: digest("dmsg/wallet-cosign-packet/v1", &plan),
        signatures,
    };
    import(
        h,
        view.wallet,
        p.id,
        seq,
        &URL_SAFE_NO_PAD.encode(crate::codec::canonical(&packet)),
    )
    .await
}

/// Collect a packet only after checking its context and every signature.
pub async fn import<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u64,
    seq: u32,
    text: &str,
) -> Result<Json> {
    ensure(text.len() <= 512_000, Error::QuotaExceeded)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| Error::IntegrityFailed)?;
    let mut incoming: Packet = crate::codec::decode(&bytes, 384_000)?;
    let (view, p, plan, digests, policy) = context(h, wallet, id, seq).await?;
    let mut stale = validate(&mut incoming, &view, &p, plan.as_ref(), &digests, &policy)?;
    let packet = h
        .with(|c| {
            let stored: Option<Packet> = c.workspace.private(c.keys()?, &key(wallet, id, seq))?;
            let mut packet = match stored {
                Some(mut old) => {
                    match validate(&mut old, &view, &p, plan.as_ref(), &digests, &policy) {
                        Ok(dropped) => {
                            stale.extend(dropped);
                            merge(old, incoming)
                        }
                        Err(_) => incoming,
                    }
                }
                None => incoming,
            };
            // Halves of one cosigner from both packets must verify together.
            stale.extend(validate(
                &mut packet,
                &view,
                &p,
                plan.as_ref(),
                &digests,
                &policy,
            )?);
            c.workspace
                .put_private(c.keys()?, &key(wallet, id, seq), &packet)?;
            Ok(packet)
        })
        .await?;
    // A cosigner that signed again is no longer stale.
    stale.retain(|id| packet.signatures.iter().all(|s| s.cosigner != *id));
    stale.sort();
    stale.dedup();
    result(&packet, &digests, &policy, &stale)
}

/// Inspect collected signatures for a plan, without signing or executing it.
pub async fn status<H: Hold>(h: &H, wallet: Principal, id: u64, seq: u32) -> Result<Json> {
    let (view, p, plan, digests, policy) = context(h, wallet, id, seq).await?;
    let mut packet = h
        .with(|c| {
            c.workspace
                .private::<Packet>(c.keys()?, &key(wallet, id, seq))
        })
        .await?
        .unwrap_or(Packet {
            version: 1,
            wallet,
            proposal: id,
            digest: p.digest,
            plan: seq,
            plan_hash: digest("dmsg/wallet-cosign-packet/v1", &plan),
            signatures: vec![],
        });
    let stale = validate(&mut packet, &view, &p, plan.as_ref(), &digests, &policy)?;
    let can_cosign = h
        .with(|c| {
            let me = c.wallet_member()?;
            Ok(view
                .members
                .iter()
                .any(|m| m.id == me && m.status == MemberStatus::Active && m.role != Role::Viewer)
                && !c.local_cosigners_for(&view, &policy)?.is_empty())
        })
        .await?;
    let mut out = result(&packet, &digests, &policy, &stale)?;
    out["can_cosign"] = json!(can_cosign);
    Ok(out)
}

/// Ask this device to cosign an existing plan rather than preparing a different one.
pub async fn request<H: Hold>(
    h: &H,
    source: &str,
    wallet: Principal,
    id: u64,
    seq: u32,
) -> Result<Json> {
    let (view, p, plan, _, policy) = context(h, wallet, id, seq).await?;
    let me = h.with(|c| c.wallet_member()).await?;
    ensure(
        view.members
            .iter()
            .any(|m| m.id == me && m.status == MemberStatus::Active && m.role != Role::Viewer),
        Error::Forbidden,
    )?;
    let cosigners = h
        .with(|c| {
            let local = c.wallet_cosigner_keys()?;
            Ok(view
                .cosigners
                .iter()
                .filter(|key| {
                    policy.iter().any(|r| r.policy.cosigners.contains(&key.id))
                        && (!view.policy.hardware_cosigners || key.input.hardware)
                        && local.iter().any(|k| k.public_key == key.input.public_key)
                })
                .map(|c| c.id)
                .collect::<Vec<_>>())
        })
        .await?;
    ensure(!cosigners.is_empty(), Error::NotFound)?;
    h.with(|c| {
        c.wallet_request(
            source,
            JobKind::Cosign {
                wallet,
                name: view.name.clone(),
                context: Box::new(view),
                proposal: Box::new(p),
                plan: plan.map(Box::new),
                cosigners,
                collect_only: true,
            },
        )
    })
    .await
}

/// Execute collected signatures only after an independent explicit user action.
pub async fn execute<H: Hold>(h: &H, wallet: Principal, id: u64, seq: u32) -> Result<Json> {
    let (view, p, plan, digests, policy) = context(h, wallet, id, seq).await?;
    let mut packet = h
        .with(|c| {
            c.workspace
                .private::<Packet>(c.keys()?, &key(wallet, id, seq))
        })
        .await?
        .ok_or(Error::NotFound)?;
    // Only signatures that still verify; the rest must be made again.
    let stale = validate(&mut packet, &view, &p, plan.as_ref(), &digests, &policy)?;
    ensure(
        result(&packet, &digests, &policy, &stale)?["ready"] == true,
        Error::Forbidden,
    )?;
    match plan {
        Some(plan) => ops::sign_and_send(h, wallet, &view, &p, &plan, packet.signatures).await,
        None => ops::execute_icp(h, wallet, id, packet.signatures).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};

    #[test]
    fn two_devices_collect_one_plan_and_reject_context_substitution() {
        let vectors: Json = serde_json::from_str(include_str!(
            "../../../../tests/vectors/wallet/plans-v1.json"
        ))
        .unwrap();
        let mut view: WalletView = dmsg_protocol::decode_canonical(
            &hex::decode(vectors["view_cbor_hex"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        let owner = MemberId::Principal(Principal::from_slice(&[1; 10]));
        let keys = [
            SigningKey::from_slice(&[1; 32]).unwrap(),
            SigningKey::from_slice(&[2; 32]).unwrap(),
        ];
        view.cosigners = keys
            .iter()
            .enumerate()
            .map(|(i, key)| Cosigner {
                id: i as u32 + 1,
                input: CosignerInput {
                    member: owner.clone(),
                    kind: CosignerKind::P256,
                    public_key: key
                        .verifying_key()
                        .to_sec1_point(true)
                        .as_bytes()
                        .to_vec()
                        .into(),
                    envelope: CosignerEnvelope::Raw,
                    hardware: false,
                    label: format!("device {i}"),
                },
                counter: 0,
                added_at_ms: 0,
            })
            .collect();
        let p = Proposal {
            id: 7,
            op_id: Hash::new([1; 32]),
            intent: Intent::Transfer {
                from: 0,
                asset: AssetKey::Icp,
                to: Destination::Icrc(Account {
                    owner: view.wallet,
                    subaccount: None,
                }),
                amount: 1,
                max_fee: 10,
                memo: None,
            },
            policy_version: 1,
            proposer: owner,
            via_agent: None,
            digest: Hash::new([3; 32]),
            created_at_ms: 0,
            expires_at_ms: u64::MAX,
            requirement: Requirement {
                approvals: 1,
                admin: false,
                delay_ms: 0,
                window_amount: 1,
            },
            status: ProposalStatus::Ready,
            votes: vec![],
            ready_at_ms: Some(0),
            plans: 0,
            signed: vec![],
            broadcasts: vec![],
            outcome: None,
            failure: None,
            updated_at_ms: 0,
        };
        let policy = vec![CosignRequirement {
            address: 0,
            policy: CosignPolicy {
                threshold: 2,
                cosigners: vec![1, 2],
            },
        }];
        view.addresses[0].id = 0;
        view.addresses[0].cosign = Some(policy[0].policy.clone());
        view.policy.version = p.policy_version;
        assert!(signing_requirements(&view, &p, 1).is_ok());
        for status in [
            ProposalStatus::Cancelled,
            ProposalStatus::Vetoed,
            ProposalStatus::Expired,
            ProposalStatus::Stale,
            ProposalStatus::Settled,
        ] {
            let mut stale = p.clone();
            stale.status = status;
            assert!(signing_requirements(&view, &stale, 1).is_err());
        }
        let mut expired = p.clone();
        expired.expires_at_ms = 1;
        assert_eq!(
            signing_requirements(&view, &expired, 1),
            Err(Error::Expired)
        );
        view.frozen = true;
        assert_eq!(signing_requirements(&view, &p, 1), Err(Error::Locked));
        view.frozen = false;
        view.policy.version += 1;
        assert_eq!(
            signing_requirements(&view, &p, 1),
            Err(Error::VersionConflict)
        );
        view.policy.version = p.policy_version;
        let packet = |i: usize| {
            let signature: Signature =
                keys[i].sign(&cosign_message(view.wallet, p.id, 0, 0, &p.digest)[..]);
            Packet {
                version: 1,
                wallet: view.wallet,
                proposal: p.id,
                digest: p.digest,
                plan: 0,
                plan_hash: digest(
                    "dmsg/wallet-cosign-packet/v1",
                    &Option::<&ExecutionPlan>::None,
                ),
                signatures: vec![CosignerSig {
                    cosigner: i as u32 + 1,
                    digest_index: 0,
                    signature: signature.to_bytes().to_vec().into(),
                    authenticator_data: None,
                    client_data_json: None,
                }],
            }
        };
        let check = |packet: &Packet, view: &WalletView| {
            let mut kept = packet.clone();
            validate(&mut kept, view, &p, None, &[p.digest], &policy).map(|stale| (kept, stale))
        };
        let a = packet(0);
        let b = packet(1);
        assert_eq!(check(&a, &view).unwrap(), (a.clone(), vec![]));
        assert_eq!(
            result(&a, &[p.digest], &policy, &[]).unwrap()["ready"],
            false
        );
        let both = merge(a.clone(), b.clone());
        assert_eq!(check(&both, &view).unwrap().1, Vec::<u32>::new());
        assert_eq!(
            result(&both, &[p.digest], &policy, &[]).unwrap()["ready"],
            true
        );
        assert_eq!(merge(both.clone(), b.clone()).signatures.len(), 2);
        let mut wrong = both.clone();
        wrong.proposal += 1;
        assert!(check(&wrong, &view).is_err());
        wrong = both.clone();
        wrong.plan_hash = Hash::new([9; 32]);
        assert!(check(&wrong, &view).is_err());
        // A signature that does not verify is dropped and its cosigner
        // signs again; the others stay.
        wrong = both.clone();
        wrong.signatures[0].signature = vec![0; 64].into();
        assert_eq!(check(&wrong, &view).unwrap(), (b.clone(), vec![1]));
        wrong = both.clone();
        wrong.signatures.push(wrong.signatures[0].clone());
        assert_eq!(check(&wrong, &view).unwrap(), (b.clone(), vec![1]));
        // After cosigner 1 takes a new key, its collected signature is stale.
        let mut rotated = view.clone();
        rotated.cosigners[0].input.public_key = SigningKey::from_slice(&[3; 32])
            .unwrap()
            .verifying_key()
            .to_sec1_point(true)
            .as_bytes()
            .to_vec()
            .into();
        let (kept, stale) = check(&both, &rotated).unwrap();
        assert_eq!((kept.signatures, stale), (b.signatures.clone(), vec![1]));
        assert_eq!(
            result(&b, &[p.digest], &policy, &[1]).unwrap()["stale"],
            json!([1])
        );
        // Keys the policy cannot use are dropped without asking them again.
        view.policy.hardware_cosigners = true;
        let (kept, stale) = check(&both, &view).unwrap();
        assert!(kept.signatures.is_empty() && stale.is_empty());
    }
}
