//! Portable cosignatures bind one wallet, proposal and exact execution plan.
//! Importing a packet only collects signatures; executing is a separate action.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

#[derive(Clone, Serialize, Deserialize)]
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

async fn context<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u64,
    seq: u32,
) -> Result<(
    WalletView,
    Proposal,
    Option<ExecutionPlan>,
    Vec<Hash>,
    CosignPolicy,
)> {
    let view = state(h, wallet).await?;
    let p = proposal_raw(h, wallet, id).await?;
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
    let from = source_address(&p.intent)
        .or_else(|| external_source(&p.intent).map(|v| v.0))
        .ok_or(Error::Forbidden)?;
    let a = view
        .addresses
        .iter()
        .find(|a| a.id == from)
        .ok_or(Error::NotFound)?;
    let policy = a.cosign.clone().ok_or(Error::NotFound)?;
    let plan = if seq == 0 {
        ensure(a.chain == Chain::Icp, Error::IntegrityFailed)?;
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
    let digests = plan
        .as_ref()
        .map(|p| p.digests.clone())
        .unwrap_or_else(|| vec![p.digest]);
    Ok((view, p, plan, digests, policy))
}

fn validate(
    packet: &Packet,
    view: &WalletView,
    p: &Proposal,
    plan: Option<&ExecutionPlan>,
    digests: &[Hash],
    policy: &CosignPolicy,
) -> Result<()> {
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
    ensure(packet.signatures.len() <= 64 * 32, Error::QuotaExceeded)?;
    let mut seen = std::collections::BTreeSet::new();
    for sig in &packet.signatures {
        ensure(
            seen.insert((sig.cosigner, sig.digest_index))
                && policy.cosigners.contains(&sig.cosigner),
            Error::IntegrityFailed,
        )?;
        let c = view
            .cosigners
            .iter()
            .find(|c| c.id == sig.cosigner)
            .ok_or(Error::NotFound)?;
        ensure(
            !view.policy.hardware_cosigners || c.input.hardware,
            Error::Forbidden,
        )?;
        let d = digests
            .get(sig.digest_index as usize)
            .ok_or(Error::IntegrityFailed)?;
        verify_cosign(
            c,
            &cosign_message(view.wallet, p.id, seq, sig.digest_index, d),
            sig,
        )?;
    }
    Ok(())
}

fn merge(mut a: Packet, b: Packet) -> Packet {
    for sig in b.signatures {
        if !a
            .signatures
            .iter()
            .any(|s| s.cosigner == sig.cosigner && s.digest_index == sig.digest_index)
        {
            a.signatures.push(sig);
        }
    }
    a
}

fn result(packet: &Packet, digests: &[Hash], policy: &CosignPolicy) -> Result<Json> {
    let complete = policy
        .cosigners
        .iter()
        .filter(|id| {
            (0..digests.len()).all(|i| {
                packet
                    .signatures
                    .iter()
                    .any(|s| s.cosigner == **id && s.digest_index as usize == i)
            })
        })
        .count();
    Ok(
        json!({"wallet":packet.wallet.to_text(),"proposal":packet.proposal,"plan":packet.plan,"collected":complete,"required":policy.threshold,"ready":complete>=usize::from(policy.threshold),"packet":URL_SAFE_NO_PAD.encode(crate::codec::canonical(packet))}),
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
    let incoming: Packet = crate::codec::decode(&bytes, 384_000)?;
    let (view, p, plan, digests, policy) = context(h, wallet, id, seq).await?;
    validate(&incoming, &view, &p, plan.as_ref(), &digests, &policy)?;
    let packet = h
        .with(|c| {
            let stored: Option<Packet> = c.workspace.private(c.keys()?, &key(wallet, id, seq))?;
            let packet = match stored {
                Some(old)
                    if validate(&old, &view, &p, plan.as_ref(), &digests, &policy).is_ok() =>
                {
                    merge(old, incoming)
                }
                _ => incoming,
            };
            c.workspace
                .put_private(c.keys()?, &key(wallet, id, seq), &packet)?;
            Ok(packet)
        })
        .await?;
    result(&packet, &digests, &policy)
}

/// Inspect collected signatures for a plan, without signing or executing it.
pub async fn status<H: Hold>(h: &H, wallet: Principal, id: u64, seq: u32) -> Result<Json> {
    let (view, p, plan, digests, policy) = context(h, wallet, id, seq).await?;
    let packet = h
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
    validate(&packet, &view, &p, plan.as_ref(), &digests, &policy)?;
    result(&packet, &digests, &policy)
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
                    policy.cosigners.contains(&key.id)
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
    let packet = h
        .with(|c| {
            c.workspace
                .private::<Packet>(c.keys()?, &key(wallet, id, seq))
        })
        .await?
        .ok_or(Error::NotFound)?;
    validate(&packet, &view, &p, plan.as_ref(), &digests, &policy)?;
    ensure(
        result(&packet, &digests, &policy)?["ready"] == true,
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
        let policy = CosignPolicy {
            threshold: 2,
            cosigners: vec![1, 2],
        };
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
        let a = packet(0);
        let b = packet(1);
        validate(&a, &view, &p, None, &[p.digest], &policy).unwrap();
        assert_eq!(result(&a, &[p.digest], &policy).unwrap()["ready"], false);
        let both = merge(a.clone(), b.clone());
        validate(&both, &view, &p, None, &[p.digest], &policy).unwrap();
        assert_eq!(result(&both, &[p.digest], &policy).unwrap()["ready"], true);
        assert_eq!(merge(both.clone(), b).signatures.len(), 2);
        let mut wrong = both.clone();
        wrong.proposal += 1;
        assert!(validate(&wrong, &view, &p, None, &[p.digest], &policy).is_err());
        wrong = both.clone();
        wrong.plan_hash = Hash::new([9; 32]);
        assert!(validate(&wrong, &view, &p, None, &[p.digest], &policy).is_err());
        wrong = both.clone();
        wrong.signatures[0].signature = vec![0; 64].into();
        assert!(validate(&wrong, &view, &p, None, &[p.digest], &policy).is_err());
        wrong = both.clone();
        wrong.signatures.push(wrong.signatures[0].clone());
        assert!(validate(&wrong, &view, &p, None, &[p.digest], &policy).is_err());
        view.policy.hardware_cosigners = true;
        assert!(validate(&both, &view, &p, None, &[p.digest], &policy).is_err());
    }
}
