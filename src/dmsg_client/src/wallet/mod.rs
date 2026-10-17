//! Team wallets (`dmsg-wallet/1`): one canister per team, created through
//! the hub. Members propose intents; deciding on one, creating a wallet,
//! joining one and cosigning go through the approval center, where only a
//! person answers. The wallet signs every transaction itself; this client
//! reads chain parameters, checks each plan with its own decoder,
//! broadcasts what the wallet signed and reports it.
//!
//! The client calls wallets with a wallet key of its own, an Ed25519 key
//! sealed under this device's local key: login delegations reach only fixed
//! canisters, and a team wallet binds the Principals its members name in
//! device-approved joins. Only device approvals, verified by the user home,
//! decide anything.

pub mod cosign;
pub mod decode;
pub mod rpc;

use crate::client::Client;
use crate::crypto::{random_id, sign};
use crate::ic::Ic;
use crate::now_ms;
use crate::payer::Stage;
use crate::payment::certified_leaf;
use candid::Principal;
use dmsg_protocol::{approval_message, digest, integration::*, wallet::*};
use dmsg_types::{
    ensure,
    integration::{AppRegistration, ApplicationApproval, ApprovalPurpose, ProductRegistration},
    user::Capability,
    wallet::*,
    Approval, CertifiedBatch, Ed25519Signature, Error, Hash, Result,
};
use futures_util::future::{join, join_all, try_join_all};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

const WALLET_KEY: &str = "wallet-key";
const JOB: &str = "wallet-job:";
const QUOTE: &str = "wallet-quote:";
/// The payer journal key of a quote's payment.
const ACTIVATION: &str = "wallet-activation:";
const APPLICATION: &str = "dmsg/application/approve/v1";
const ORIGIN: &str = "https://dmsg.net";
/// How long a signed application approval stays valid.
const APPROVAL_MS: u64 = 4 * dmsg_types::MINUTE;

/// What a wallet approval request does once a person approves it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// Pay a quote and create the wallet.
    Create {
        /// The quote.
        quote: Box<ActivationQuote>,
        /// The wallet.
        spec: Box<WalletSpec>,
    },
    /// Join a wallet with this device's wallet key.
    Join {
        /// The wallet.
        wallet: Principal,
        /// Its name.
        name: String,
    },
    /// Decide on a proposal.
    Decide {
        /// The wallet.
        wallet: Principal,
        /// Its name.
        name: String,
        /// The proposal as shown.
        proposal: Box<Proposal>,
        /// The decision.
        decision: Decision,
    },
    /// Cosign with a local cosigner key, then execute: a plan, which is
    /// signed and broadcast, or an ICP proposal's digest.
    Cosign {
        /// The wallet.
        wallet: Principal,
        /// Its name.
        name: String,
        /// The proposal.
        proposal: Box<Proposal>,
        /// The plan, checked by the decoder; none for ICP and ICRC.
        plan: Option<Box<ExecutionPlan>>,
        /// The wallet's cosigner ID of the local key.
        cosigner: u32,
    },
}

/// A wallet approval request's journal, sealed under the LDK.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletJob {
    /// Random ID, hex.
    pub id: String,
    /// What it does.
    pub kind: JobKind,
    /// The signed account approval, sent again on a retry.
    pub approval: Option<(ApplicationApproval, Approval)>,
    /// What execution returned.
    pub result: Option<Json>,
}

impl WalletJob {
    /// The commitment a person approves.
    pub fn digest(&self) -> Hash {
        digest("dmsg/wallet-job/v1", &(&self.id, &self.kind))
    }

    /// What the approval window shows.
    pub fn view(&self) -> Json {
        let mut view = match &self.kind {
            JobKind::Create { quote, spec } => json!({
                "action": "create",
                "quote_id": hex::encode(*quote.quote_id),
                "name": spec.name,
                "price_e8s": quote.price_e8s.to_string(),
                "ledger_fee_e8s": quote.ledger_fee_e8s.to_string(),
                "plan": format!("{:?}", quote.plan),
                "members": spec.members.iter().map(member_view).collect::<Vec<_>>(),
                "policy": policy_view(&spec.policy),
                "expires_at": quote.expires_at_ms,
            }),
            JobKind::Join { wallet, name } => json!({
                "action": "join",
                "wallet": wallet.to_text(),
                "name": name,
            }),
            JobKind::Decide {
                wallet,
                name,
                proposal,
                decision,
            } => json!({
                "action": "decide",
                "decision": format!("{decision:?}").to_lowercase(),
                "wallet": wallet.to_text(),
                "name": name,
                "proposal": proposal_view(proposal),
            }),
            JobKind::Cosign {
                wallet,
                name,
                proposal,
                plan,
                cosigner,
            } => json!({
                "action": "cosign",
                "wallet": wallet.to_text(),
                "name": name,
                "proposal": proposal_view(proposal),
                "plan": plan.as_deref().map(plan_view),
                "cosigner": cosigner,
            }),
        };
        view["result"] = self.result.clone().unwrap_or(Json::Null);
        view
    }
}

fn member_id_text(id: &MemberId) -> String {
    match id {
        MemberId::Account(a) => a.to_string(),
        MemberId::Principal(p) => p.to_text(),
    }
}

fn member_view(m: &MemberInput) -> Json {
    json!({
        "id": member_id_text(&m.id),
        "kind": if matches!(m.id, MemberId::Account(_)) { "account" } else { "principal" },
        "role": format!("{:?}", m.role),
        "label": m.label,
        "agent_ids": m.agent_ids,
    })
}

/// An asset's name and decimals for display.
pub fn asset_view(asset: &AssetKey) -> Json {
    match asset {
        AssetKey::Icp => json!({"asset": "ICP", "decimals": 8}),
        AssetKey::Icrc(ledger) => json!({"asset": "ICRC", "ledger": ledger.to_text()}),
        AssetKey::Btc => json!({"asset": "BTC", "decimals": 8}),
        AssetKey::Evm {
            chain_id,
            token: None,
        } => json!({"asset": "ETH", "chain_id": chain_id, "decimals": 18}),
        AssetKey::Evm {
            chain_id,
            token: Some(token),
        } => json!({"asset": "ERC20", "chain_id": chain_id, "token": eip55(&token.into_array())}),
        AssetKey::Sol { mint: None } => json!({"asset": "SOL", "decimals": 9}),
        AssetKey::Sol { mint: Some(mint) } => {
            json!({"asset": "SPL", "mint": sol_address(&mint.into_array())})
        }
    }
}

fn policy_view(p: &Policy) -> Json {
    json!({
        "version": p.version,
        "approver_threshold": p.approver_threshold,
        "admin_threshold": p.admin_threshold,
        "tiers": p.tiers.iter().map(|t| json!({
            "asset": asset_view(&t.asset),
            "up_to": t.up_to.to_string(),
            "approvals": t.approvals,
            "delay_ms": t.delay_ms,
            "book_only": t.book_only,
        })).collect::<Vec<_>>(),
        "address_book_cooldown_ms": p.address_book_cooldown_ms,
        "daily_limits": p.daily_limits.iter().map(|l| json!({"asset": asset_view(&l.asset), "amount": l.amount.to_string()})).collect::<Vec<_>>(),
        "spendable_assets": p.spendable_assets.iter().map(asset_view).collect::<Vec<_>>(),
        "call_targets": p.call_targets.iter().map(Principal::to_text).collect::<Vec<_>>(),
        "evm_chains": p.evm_chains,
        "proposal_ttl_ms": p.proposal_ttl_ms,
        "unfreeze_delay_ms": p.unfreeze_delay_ms,
        "daily_signatures": p.daily_signatures,
        "hardware_cosigners": p.hardware_cosigners,
        "public": p.public,
        "refuel": p.refuel.as_ref().map(|r| json!({
            "from": r.from,
            "min_cycles": r.min_cycles.to_string(),
            "icp_e8s": r.icp_e8s.to_string(),
            "max_per_day": r.max_per_day,
        })),
    })
}

/// An intent for display: its kind and what it moves or changes.
pub fn intent_view(intent: &Intent) -> Json {
    let outputs = |items: &[(Destination, u128)]| {
        items
            .iter()
            .map(|(to, amount)| json!({"to": destination_text(to), "amount": amount.to_string()}))
            .collect::<Vec<_>>()
    };
    match intent {
        Intent::Transfer {
            from,
            asset,
            to,
            amount,
            max_fee,
            memo,
        } => json!({
            "kind": "transfer",
            "from": from,
            "asset": asset_view(asset),
            "outputs": outputs(&[(to.clone(), *amount)]),
            "max_fee": max_fee.to_string(),
            "memo": memo.as_ref().map(hex::encode),
        }),
        Intent::Batch {
            from,
            asset,
            items,
            max_fee,
        } => json!({
            "kind": "batch",
            "from": from,
            "asset": asset_view(asset),
            "outputs": outputs(&items.iter().map(|i| (i.to.clone(), i.amount)).collect::<Vec<_>>()),
            "max_fee": max_fee.to_string(),
        }),
        Intent::CanisterCall {
            canister,
            method,
            arg,
            cycles,
            consent,
        } => json!({
            "kind": "canister_call",
            "canister": canister.to_text(),
            "method": method,
            "arg": hex::encode(arg),
            "arg_text": candid::IDLArgs::from_bytes(arg).ok().map(|a| a.to_string()),
            "cycles": cycles.to_string(),
            "consent": consent.as_ref().map(|c| c.message.clone()),
        }),
        Intent::AddAddress {
            chain,
            kind,
            label,
            cosign,
        } => json!({
            "kind": "add_address",
            "chain": format!("{chain:?}"),
            "control": format!("{kind:?}"),
            "label": label,
            "cosign": cosign,
        }),
        Intent::SetCosign { address, cosign } => {
            json!({"kind": "set_cosign", "address": address, "cosign": cosign})
        }
        Intent::AddCosigner(input) => json!({
            "kind": "add_cosigner",
            "member": member_id_text(&input.member),
            "curve": format!("{:?}", input.kind),
            "public_key": hex::encode(&input.public_key),
            "envelope": format!("{:?}", input.envelope),
            "hardware": input.hardware,
            "label": input.label,
        }),
        Intent::RemoveCosigner(id) => json!({"kind": "remove_cosigner", "cosigner": id}),
        Intent::AddMember(m) => json!({"kind": "add_member", "member": member_view(m)}),
        Intent::RemoveMember(id) => json!({"kind": "remove_member", "member": member_id_text(id)}),
        Intent::UpdateMember(m) => json!({"kind": "update_member", "member": member_view(m)}),
        Intent::SetPolicy(p) => json!({"kind": "set_policy", "policy": policy_view(p)}),
        Intent::AddressBook(BookChange::Add { destination, label }) => {
            json!({"kind": "book_add", "destination": destination_text(destination), "label": label})
        }
        Intent::AddressBook(BookChange::Remove { destination }) => {
            json!({"kind": "book_remove", "destination": destination_text(destination)})
        }
        Intent::Unfreeze => json!({"kind": "unfreeze"}),
        Intent::Refuel { from, icp_e8s } => {
            json!({"kind": "refuel", "from": from, "icp_e8s": icp_e8s.to_string()})
        }
        Intent::AcceptUpgrade {
            version,
            module_hash,
        } => {
            json!({"kind": "accept_upgrade", "version": version, "module_hash": hex::encode(**module_hash)})
        }
        Intent::LockCode => json!({"kind": "lock_code"}),
        Intent::CancelEvmNonce {
            address,
            chain_id,
            max_fee,
        } => {
            json!({"kind": "cancel_evm_nonce", "address": address, "chain_id": chain_id, "max_fee": max_fee.to_string()})
        }
        Intent::ReleaseEvmNonce { address, chain_id } => {
            json!({"kind": "release_evm_nonce", "address": address, "chain_id": chain_id})
        }
        Intent::InitSolNonce { address, max_fee } => {
            json!({"kind": "init_sol_nonce", "address": address, "max_fee": max_fee.to_string()})
        }
    }
}

/// A proposal for display.
pub fn proposal_view(p: &Proposal) -> Json {
    json!({
        "id": p.id,
        "status": format!("{:?}", p.status),
        "intent": intent_view(&p.intent),
        "proposer": member_id_text(&p.proposer),
        "via_agent": p.via_agent,
        "digest": hex::encode(*p.digest),
        "policy_version": p.policy_version,
        "requirement": {
            "approvals": p.requirement.approvals,
            "admin": p.requirement.admin,
            "delay_ms": p.requirement.delay_ms,
            "window_amount": p.requirement.window_amount.to_string(),
        },
        "votes": p.votes.iter().map(|v| json!({
            "member": member_id_text(&v.member),
            "decision": format!("{:?}", v.decision),
            "at": v.at_ms,
        })).collect::<Vec<_>>(),
        "created_at": p.created_at_ms,
        "expires_at": p.expires_at_ms,
        "ready_at": p.ready_at_ms,
        "plans": p.plans,
        "signed": p.signed.iter().map(|s| json!({"plan": s.plan, "txid": s.txid, "signed_at": s.signed_at_ms})).collect::<Vec<_>>(),
        "broadcasts": p.broadcasts.iter().map(|b| json!({
            "plan": b.plan,
            "outcome": format!("{:?}", b.outcome),
            "txid": b.txid,
            "member": member_id_text(&b.member),
            "at": b.at_ms,
            "note": b.note,
        })).collect::<Vec<_>>(),
        "outcome": p.outcome.as_ref().map(outcome_view),
        "failure": p.failure,
        "updated_at": p.updated_at_ms,
    })
}

fn outcome_view(o: &Outcome) -> Json {
    match o {
        Outcome::Applied => json!({"kind": "applied"}),
        Outcome::Ledger { blocks } => json!({"kind": "ledger", "blocks": blocks}),
        Outcome::Replied { reply, reply_hash } => json!({
            "kind": "replied",
            "reply": hex::encode(reply),
            "reply_text": candid::IDLArgs::from_bytes(reply).ok().map(|a| a.to_string()),
            "reply_hash": hex::encode(**reply_hash),
        }),
        Outcome::CallRejected { code, message } => {
            json!({"kind": "call_rejected", "code": code, "message": message})
        }
        Outcome::CallUnknown => json!({"kind": "call_unknown"}),
        Outcome::AddressCreated(id) => json!({"kind": "address_created", "address": id}),
        Outcome::CosignerAdded(id) => json!({"kind": "cosigner_added", "cosigner": id}),
        Outcome::ChainSettled { plan, txid } => {
            json!({"kind": "chain_settled", "plan": plan, "txid": txid})
        }
        Outcome::Refueled { block, cycles } => {
            json!({"kind": "refueled", "block": block, "cycles": cycles.to_string()})
        }
    }
}

/// A plan for display.
pub fn plan_view(plan: &ExecutionPlan) -> Json {
    let s = &plan.summary;
    json!({
        "proposal": plan.proposal,
        "seq": plan.seq,
        "chain": s.chain,
        "from": s.from,
        "asset": s.asset,
        "outputs": s.outputs.iter().map(|o| json!({"to": o.to, "amount": o.amount.to_string()})).collect::<Vec<_>>(),
        "change": s.change.as_ref().map(|o| json!({"to": o.to, "amount": o.amount.to_string()})),
        "fee": s.fee.to_string(),
        "nonce": s.nonce,
        "digests": plan.digests.len(),
        "expires_at": plan.expires_at_ms,
    })
}

/// Whether an address's key and text match the wallet's key roots.
fn address_verified(view: &WalletView, a: &Address) -> bool {
    let path = match derivation_path(a.chain, &a.kind, a.id) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let key_ok = match key_algorithm(a.chain, &a.kind) {
        Ok(None) => a.public_key.is_empty(),
        Ok(Some(KeyAlgorithm::Ed25519)) => view.key_roots.ed25519.as_ref().is_some_and(|root| {
            ic_ed25519::PublicKey::deserialize_raw(&root.public_key).is_ok_and(|master| {
                let path = ic_ed25519::DerivationPath::new(
                    path.iter()
                        .cloned()
                        .map(ic_ed25519::DerivationIndex)
                        .collect(),
                );
                let (child, _) =
                    master.derive_subkey_with_chain_code(&path, &root.chain_code.into_array());
                child.serialize_raw().as_slice() == a.public_key.as_slice()
            })
        }),
        Ok(Some(algorithm)) => {
            let root = match algorithm {
                KeyAlgorithm::Ecdsa => view.key_roots.ecdsa.as_ref(),
                _ => view.key_roots.bip340.as_ref(),
            };
            root.is_some_and(|root| {
                ic_secp256k1::PublicKey::deserialize_sec1(&root.public_key).is_ok_and(|master| {
                    let path = ic_secp256k1::DerivationPath::new(
                        path.iter()
                            .cloned()
                            .map(ic_secp256k1::DerivationIndex)
                            .collect(),
                    );
                    let (child, _) =
                        master.derive_subkey_with_chain_code(&path, &root.chain_code.into_array());
                    child.serialize_sec1(true) == a.public_key.as_slice()
                })
            })
        }
        Err(_) => false,
    };
    let cosigner: Option<[u8; 32]> = match &a.kind {
        AddressKind::Taproot(control) => view
            .cosigners
            .iter()
            .find(|c| c.id == control.cosigner)
            .and_then(|c| c.input.public_key.as_slice().try_into().ok()),
        _ => None,
    };
    key_ok
        && address_text(
            view.wallet,
            view.btc_network,
            a.id,
            a.chain,
            &a.kind,
            &a.public_key,
            cosigner.as_ref(),
        )
        .is_ok_and(|text| text == a.text)
}

/// A wallet for display, each address marked whether this client derived
/// the same key and text on its own.
pub fn wallet_view(view: &WalletView, me: Option<&MemberId>) -> Json {
    json!({
        "wallet": view.wallet.to_text(),
        "hub": view.hub.to_text(),
        "name": view.name,
        "key_name": view.key_name,
        "btc_network": format!("{:?}", view.btc_network),
        "frozen": view.frozen,
        "code": format!("{:?}", view.code),
        "created_at": view.created_at_ms,
        "next_proposal": view.next_proposal,
        "audit_len": view.audit_len,
        "audit_head": hex::encode(*view.audit_head),
        "channel_hint": view.channel_hint,
        "me": me.and_then(|me| view.members.iter().find(|m| m.id == *me)).map(|m| json!({
            "role": format!("{:?}", m.role),
            "status": format!("{:?}", m.status),
        })),
        "members": view.members.iter().map(|m| json!({
            "id": member_id_text(&m.id),
            "kind": if matches!(m.id, MemberId::Account(_)) { "account" } else { "principal" },
            "role": format!("{:?}", m.role),
            "label": m.label,
            "status": format!("{:?}", m.status),
            "principals": m.principals.iter().map(Principal::to_text).collect::<Vec<_>>(),
            "agent_ids": m.agent_ids,
        })).collect::<Vec<_>>(),
        "policy": policy_view(&view.policy),
        "addresses": view.addresses.iter().map(|a| json!({
            "id": a.id,
            "chain": format!("{:?}", a.chain),
            "control": match &a.kind {
                AddressKind::ChainKey => "chain_key",
                AddressKind::Segwit => "segwit",
                AddressKind::Taproot(_) => "taproot_2of2",
            },
            "text": a.text,
            "label": a.label,
            "cosign": a.cosign,
            "sol_nonce": a.sol_nonce.map(|n| sol_address(&n.into_array())),
            "verified": address_verified(view, a),
        })).collect::<Vec<_>>(),
        "cosigners": view.cosigners.iter().map(|c| json!({
            "id": c.id,
            "member": member_id_text(&c.input.member),
            "curve": format!("{:?}", c.input.kind),
            "public_key": hex::encode(&c.input.public_key),
            "envelope": format!("{:?}", c.input.envelope),
            "hardware": c.input.hardware,
            "label": c.input.label,
        })).collect::<Vec<_>>(),
    })
}

/// A balance as a view shows it: the amount, or why it is unknown.
fn shown<T: ToString>(balance: Result<T>) -> std::result::Result<String, String> {
    balance.map(|b| b.to_string()).map_err(|e| e.to_string())
}

fn principal(text: &str) -> Result<Principal> {
    Principal::from_text(text).map_err(|_| dmsg_types::invalid("principal"))
}

/// Parse a wallet Principal.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn parse_wallet(text: &str) -> Result<Principal> {
    principal(text)
}

impl Client {
    fn wallet_hub(&self) -> Result<Principal> {
        self.config
            .wallet
            .hub
            .ok_or_else(|| Error::Unavailable("team wallets are not configured".into()))
    }

    /// This device's wallet key: its Principal and agent, created once.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub(crate) async fn wallet_identity(&mut self) -> Result<(Principal, Ic)> {
        if let Some(identity) = &self.wallet_ic {
            return Ok(identity.clone());
        }
        let keys = self.keys()?;
        let seed: serde_bytes::ByteArray<32> = match self.workspace.private(keys, WALLET_KEY)? {
            Some(seed) => seed,
            None => {
                let seed = serde_bytes::ByteArray::new(crate::crypto::random_key());
                self.workspace.put_private(keys, WALLET_KEY, &seed)?;
                seed
            }
        };
        let identity = ic_agent::identity::BasicIdentity::from_raw_key(&seed.into_array());
        let ic = Ic::connect(&self.config, std::sync::Arc::new(identity)).await?;
        let pair = (ic.principal()?, ic);
        self.wallet_ic = Some(pair.clone());
        Ok(pair)
    }

    fn wallet_member(&self) -> Result<MemberId> {
        Ok(MemberId::Account(self.account()?.id))
    }

    /// The wallet as this device's wallet key reads it, certified, so no
    /// single replica can show other addresses, members or policy.
    pub async fn wallet_state(&mut self, wallet: Principal) -> Result<WalletView> {
        let (_, ic) = self.wallet_identity().await?;
        let batch: Result<CertifiedBatch> = ic.query(wallet, "get_wallet_certified", ()).await?;
        let value = certified_leaf(&batch?, b"wallet", wallet, &ic.root_key(), now_ms())?
            .ok_or(Error::IntegrityFailed)?;
        let view: WalletView = crate::codec::decode(&value, 1 << 20)?;
        ensure(view.wallet == wallet, Error::IntegrityFailed)?;
        Ok(view)
    }

    /// The account's wallets as the hub indexes them.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` without a hub.
    pub async fn wallet_list(&mut self) -> Result<Json> {
        let hub = self.wallet_hub()?;
        let me = self.wallet_member()?;
        let ic = self.anonymous().await?;
        let records: Vec<WalletRecord> = ic.query(hub, "list_wallets_of", (me,)).await?;
        Ok(Json::Array(
            records
                .iter()
                .map(|r| {
                    json!({
                        "wallet": r.wallet.to_text(),
                        "name": r.name,
                        "creator": r.creator.to_string(),
                        "version": r.version,
                        "locked": r.locked,
                        "members": r.members.len(),
                        "created_at": r.created_at_ms,
                    })
                })
                .collect(),
        ))
    }

    /// A wallet with its addresses checked against its key roots.
    pub async fn wallet_show(&mut self, wallet: Principal) -> Result<Json> {
        let view = self.wallet_state(wallet).await?;
        let (_, ic) = self.wallet_identity().await?;
        let stats: WalletStats = ic.query(wallet, "wallet_stats", ()).await?;
        let me = self.wallet_member().ok();
        let mut out = wallet_view(&view, me.as_ref());
        out["stats"] = json!({
            "cycles": stats.cycles.to_string(),
            "signatures_today": stats.signatures_today,
            "proposals": stats.proposals,
            "active_proposals": stats.active_proposals,
        });
        Ok(out)
    }

    /// Proposals, newest first.
    pub async fn wallet_proposals(
        &mut self,
        wallet: Principal,
        before: Option<u64>,
        limit: u16,
    ) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
        let page: Result<ProposalPage> =
            ic.query(wallet, "list_proposals", (before, limit)).await?;
        let page = page?;
        Ok(json!({
            "proposals": page.proposals.iter().map(proposal_view).collect::<Vec<_>>(),
            "next": page.next,
        }))
    }

    /// A proposal whose digest matches what it shows, so a decision or
    /// cosignature commits to exactly that, whichever replica answered.
    async fn wallet_proposal_raw(&mut self, wallet: Principal, id: u64) -> Result<Proposal> {
        let (_, ic) = self.wallet_identity().await?;
        let p: Result<Proposal> = ic.query(wallet, "get_proposal", (id,)).await?;
        let p = p?;
        let digest = proposal_digest(
            &self.config.environment,
            wallet,
            &p.op_id,
            &p.intent,
            p.policy_version,
            &p.proposer,
            &p.via_agent,
        );
        ensure(p.id == id && p.digest == digest, Error::IntegrityFailed)?;
        Ok(p)
    }

    /// One proposal with its plans.
    pub async fn wallet_proposal(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        let p = self.wallet_proposal_raw(wallet, id).await?;
        let (_, ic) = self.wallet_identity().await?;
        let plans = try_join_all((1..=p.plans).map(|seq| {
            let ic = &ic;
            async move {
                let plan: Result<ExecutionPlan> = ic.query(wallet, "get_plan", (id, seq)).await?;
                Ok::<_, Error>(plan_view(&plan?))
            }
        }))
        .await?;
        let mut out = proposal_view(&p);
        out["plan_details"] = Json::Array(plans);
        Ok(out)
    }

    /// The hub's terms for creating a wallet for this account, paid by the
    /// payer; kept for [`Self::wallet_create_request`].
    pub async fn wallet_quote(&mut self) -> Result<Json> {
        let hub = self.wallet_hub()?;
        let account = self.account()?.id;
        let (_, ic) = self.wallet_identity().await?;
        let quote: Result<ActivationQuote> = ic.update(hub, "quote_activation", (account,)).await?;
        let quote = quote?;
        let id = hex::encode(*quote.quote_id);
        self.workspace
            .put_private(self.keys()?, &format!("{QUOTE}{id}"), &quote)?;
        Ok(json!({
            "quote_id": id,
            "plan": format!("{:?}", quote.plan),
            "remaining": quote.remaining,
            "price_e8s": quote.price_e8s.to_string(),
            "ledger_fee_e8s": quote.ledger_fee_e8s.to_string(),
            "cycles_to_wallet": quote.cycles_to_wallet.to_string(),
            "deposit": quote.deposit.to_string(),
            "version": quote.version,
            "expires_at": quote.expires_at_ms,
        }))
    }

    fn wallet_job(&self, id: &str) -> Result<WalletJob> {
        self.workspace
            .private(self.keys()?, &format!("{JOB}{id}"))?
            .ok_or(Error::NotFound)
    }

    fn save_wallet_job(&self, job: &WalletJob) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{JOB}{}", job.id), job)
    }

    /// The approval view of a wallet job.
    pub(crate) fn wallet_job_view(&self, id: &str) -> Result<Json> {
        Ok(self.wallet_job(id)?.view())
    }

    /// The digest of a wallet job, rechecked when a person approves.
    pub(crate) fn wallet_job_digest(&self, id: &str) -> Result<Hash> {
        Ok(self.wallet_job(id)?.digest())
    }

    fn wallet_request(&mut self, source: &str, kind: JobKind) -> Result<Json> {
        let job = WalletJob {
            id: hex::encode(random_id()),
            kind,
            approval: None,
            result: None,
        };
        self.save_wallet_job(&job)?;
        let request = self.approval_create(
            source,
            crate::approval::Action::Wallet {
                job: job.id.clone(),
                digest: job.digest(),
            },
            now_ms() + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&request)
    }

    fn wallet_saved_quote(&self, quote_id: &str) -> Result<ActivationQuote> {
        self.workspace
            .private(self.keys()?, &format!("{QUOTE}{quote_id}"))?
            .ok_or(Error::NotFound)
    }

    /// Ask to pay quote `quote_id` from the payer and create `spec`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a quote this device did not request.
    pub fn wallet_create_request(
        &mut self,
        source: &str,
        quote_id: &str,
        spec: WalletSpec,
    ) -> Result<Json> {
        let quote = self.wallet_saved_quote(quote_id)?;
        validate_spec(&spec, &self.wallet_member()?)?;
        ensure(now_ms() < quote.expires_at_ms, Error::Expired)?;
        self.wallet_request(
            source,
            JobKind::Create {
                quote: Box::new(quote),
                spec: Box::new(spec),
            },
        )
    }

    /// Return the deposit of quote `quote_id`, less the ledger fee, to the
    /// identity that paid it, once the hub failed to create its wallet or
    /// the quote expired unused. The hub takes the claim from this device's
    /// wallet key, which requested the quote, and pays the account it names.
    /// The refunded quote is spent, so this device forgets it.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a quote this device did not request or
    /// no longer keeps, or an empty deposit, and `Error::Forbidden` for a
    /// quote still usable or one that created its wallet.
    pub async fn wallet_refund(&mut self, quote_id: &str) -> Result<Json> {
        let quote = self.wallet_saved_quote(quote_id)?;
        let hub = self.wallet_hub()?;
        // The journal names the identity that paid; a deposit paid some
        // other way goes to the current payer.
        let key = format!("{ACTIVATION}{}", hex::encode(*quote.quote_id));
        let owner = match self.payment_journal(&key)? {
            Some(payment) => payment.payer,
            None => self.payer().await?.0,
        };
        let to = Account {
            owner,
            subaccount: None,
        };
        let (_, ic) = self.wallet_identity().await?;
        let block: Result<u64> = ic
            .update(hub, "claim_activation_refund", (quote.quote_id, to))
            .await?;
        let block = block?;
        self.workspace
            .delete_private(&format!("{QUOTE}{quote_id}"))?;
        Ok(json!({"quote_id": quote_id, "to": to.to_string(), "block": block}))
    }

    /// The activation deposits this device paid that created no wallet,
    /// newest first. A failed creation's and an expired quote's are
    /// refundable now with [`Self::wallet_refund`]; an unused quote's once
    /// it expires, unless it creates its wallet first.
    pub async fn wallet_deposits(&mut self) -> Result<Json> {
        let hub = self.wallet_hub()?;
        let keys = self.keys()?;
        let mut paid = vec![];
        for key in self.workspace.store.sealed_keys(QUOTE)? {
            let Some(quote) = self.workspace.private::<ActivationQuote>(keys, &key)? else {
                continue;
            };
            let payment =
                self.payment_journal(&format!("{ACTIVATION}{}", hex::encode(*quote.quote_id)))?;
            // Only a payment sent may have reached the deposit.
            if payment.is_some_and(|p| matches!(p.stage, Stage::Confirmed | Stage::Unknown)) {
                paid.push(quote);
            }
        }
        paid.sort_by_key(|q| std::cmp::Reverse(q.issued_at_ms));
        let mut out = vec![];
        if paid.is_empty() {
            return Ok(Json::Array(out));
        }
        let reader = self.anonymous().await?;
        let config: HubInit = reader.query(hub, "get_hub_config", ()).await?;
        let now = now_ms();
        for quote in paid {
            let order: Option<ActivationOrder> =
                reader.query(hub, "get_order", (quote.quote_id,)).await?;
            let (state, reason) = match order.map(|o| o.status) {
                Some(ActivationStatus::Failed(reason)) => ("failed", Some(reason)),
                // Created, refunded, or still being created.
                Some(_) => continue,
                None if now >= quote.expires_at_ms => ("expired", None),
                None => ("unused", None),
            };
            let held: candid::Nat = reader
                .query(config.icp_ledger, "icrc1_balance_of", (quote.deposit,))
                .await?;
            let held = u128::try_from(held.0).map_err(|_| Error::IntegrityFailed)?;
            let fee = u128::from(quote.ledger_fee_e8s);
            if held > fee {
                out.push(json!({
                    "quote_id": hex::encode(*quote.quote_id),
                    "state": state,
                    "refundable": state != "unused",
                    "refund_e8s": (held - fee).to_string(),
                    "ledger_fee_e8s": quote.ledger_fee_e8s.to_string(),
                    "expires_at": quote.expires_at_ms,
                    "reason": reason,
                }));
            }
        }
        Ok(Json::Array(out))
    }

    /// Ask to join `wallet`, binding this device's wallet key. The hub first
    /// refreshes its member index from the wallet, which lists members
    /// added after creation only then.
    pub async fn wallet_join_request(&mut self, source: &str, wallet: Principal) -> Result<Json> {
        let hub = self.wallet_hub()?;
        let (_, ic) = self.wallet_identity().await?;
        let record: Result<WalletRecord> = ic.update(hub, "sync_wallet", (wallet,)).await?;
        let record = record?;
        ensure(
            record.members.contains(&self.wallet_member()?),
            Error::Forbidden,
        )?;
        self.wallet_request(
            source,
            JobKind::Join {
                wallet,
                name: record.name,
            },
        )
    }

    /// Ask to decide on proposal `id`.
    pub async fn wallet_decide_request(
        &mut self,
        source: &str,
        wallet: Principal,
        id: u64,
        decision: Decision,
    ) -> Result<Json> {
        let view = self.wallet_state(wallet).await?;
        let proposal = self.wallet_proposal_raw(wallet, id).await?;
        let wanted = match decision {
            Decision::Veto => ProposalStatus::Delayed,
            _ => ProposalStatus::Open,
        };
        ensure(proposal.status == wanted, Error::VersionConflict)?;
        self.wallet_request(
            source,
            JobKind::Decide {
                wallet,
                name: view.name,
                proposal: Box::new(proposal),
                decision,
            },
        )
    }

    /// Propose `intent` with this device's wallet key; an agent proposing
    /// for the member names its Agent ID.
    pub async fn wallet_propose(
        &mut self,
        wallet: Principal,
        intent: Intent,
        via_agent: Option<String>,
    ) -> Result<Json> {
        validate_intent(&intent)?;
        let (_, ic) = self.wallet_identity().await?;
        let args = ProposeArgs {
            intent,
            op_id: Hash::new(random_id()),
            via_agent,
        };
        let p: Result<Proposal> = ic.update(wallet, "propose", (args,)).await?;
        Ok(proposal_view(&p?))
    }

    /// Propose a transfer from string arguments: an asset as
    /// [`parse_asset`] reads it, a recipient as [`parse_destination`] reads
    /// it, and base-unit amounts.
    #[allow(clippy::too_many_arguments)]
    pub async fn wallet_send(
        &mut self,
        wallet: Principal,
        from: u32,
        asset: &str,
        to: &str,
        amount: &str,
        max_fee: &str,
        memo: Option<&str>,
        via_agent: Option<String>,
    ) -> Result<Json> {
        let view = self.wallet_state(wallet).await?;
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
        self.wallet_propose(wallet, intent, via_agent).await
    }

    /// Withdraw this member's own proposal.
    pub async fn wallet_cancel(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
        let p: Result<Proposal> = ic.update(wallet, "cancel_proposal", (id,)).await?;
        Ok(proposal_view(&p?))
    }

    /// Stop the wallet's executions at once.
    pub async fn wallet_freeze(&mut self, wallet: Principal) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
        let r: Result<()> = ic.update(wallet, "freeze", ()).await?;
        r?;
        Ok(json!({"frozen": true}))
    }

    /// Top the wallet up under its refuel rule.
    pub async fn wallet_refuel(&mut self, wallet: Principal) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
        let r: Result<Option<u128>> = ic.update(wallet, "refuel", ()).await?;
        Ok(json!({"cycles": r?.map(|c| c.to_string())}))
    }

    /// Resend an interrupted ICRC transfer or top-up with its exact arguments.
    pub async fn wallet_reconcile(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
        let r: Result<ExecuteResult> = ic.update(wallet, "reconcile", (id,)).await?;
        Ok(proposal_view(&r?.proposal))
    }

    /// Verify the whole audit chain against the wallet's head.
    pub async fn wallet_audit(&mut self, wallet: Principal) -> Result<Json> {
        let view = self.wallet_state(wallet).await?;
        let (_, ic) = self.wallet_identity().await?;
        let mut events = vec![];
        while (events.len() as u64) < view.audit_len {
            let page: Result<Vec<AuditEvent>> = ic
                .query(wallet, "audit", (events.len() as u64, 200u16))
                .await?;
            let page = page?;
            ensure(!page.is_empty(), Error::IntegrityFailed)?;
            events.extend(page);
        }
        let head = verify_audit(Hash::new([0; 32]), 0, &events)?;
        ensure(head == view.audit_head, Error::IntegrityFailed)?;
        Ok(json!({
            "verified": true,
            "events": events.len(),
            "head": hex::encode(*head),
            "recent": events.iter().rev().take(50).map(|e| json!({
                "seq": e.seq,
                "at": e.at_ms,
                "actor": e.actor.as_ref().map(member_id_text),
                "proposal": e.proposal,
                "kind": format!("{:?}", e.kind),
            })).collect::<Vec<_>>(),
        }))
    }

    /// Execute a ready proposal. ICP and internal changes run on the
    /// wallet; an external-chain transfer is planned from fresh chain
    /// parameters, checked by this client's decoder, signed by the wallet
    /// and broadcast here. A source address requiring this device's
    /// cosigner becomes an approval request instead.
    pub async fn wallet_execute(
        &mut self,
        source: &str,
        wallet: Principal,
        id: u64,
    ) -> Result<Json> {
        let proposal = self.wallet_proposal_raw(wallet, id).await?;
        let view = self.wallet_state(wallet).await?;
        let Some((from, chain)) = external_source(&proposal.intent) else {
            // An ICP source whose cosign policy names a key of this device
            // needs the person too; the cosignature covers the proposal digest.
            let icp_source = source_address(&proposal.intent)
                .and_then(|from| view.addresses.iter().find(|a| a.id == from));
            if let Some(address) = icp_source
                && let Some(cosigner) = self.local_cosigner_of(&view, address)?
            {
                return self.wallet_request(
                    source,
                    JobKind::Cosign {
                        wallet,
                        name: view.name.clone(),
                        proposal: Box::new(proposal),
                        plan: None,
                        cosigner,
                    },
                );
            }
            let (_, ic) = self.wallet_identity().await?;
            let r: Result<ExecuteResult> = ic
                .update(
                    wallet,
                    "execute",
                    (ExecuteArgs {
                        proposal: id,
                        plan: None,
                        cosigs: vec![],
                    },),
                )
                .await?;
            return Ok(proposal_view(&r?.proposal));
        };
        let address = view
            .addresses
            .iter()
            .find(|a| a.id == from)
            .cloned()
            .ok_or(Error::NotFound)?;
        let params = self
            .wallet_params(&view, &proposal, &address, chain)
            .await?;
        let (_, ic) = self.wallet_identity().await?;
        let plan: Result<ExecutionPlan> =
            ic.update(wallet, "prepare_execution", (id, params)).await?;
        let plan = plan?;
        decode::verify_plan(&view, &proposal.intent, &plan)?;
        // A cosign policy naming a key of this device needs the person.
        if let Some(cosigner) = self.local_cosigner_of(&view, &address)? {
            return self.wallet_request(
                source,
                JobKind::Cosign {
                    wallet,
                    name: view.name.clone(),
                    proposal: Box::new(proposal),
                    plan: Some(Box::new(plan)),
                    cosigner,
                },
            );
        }
        self.wallet_sign_and_send(wallet, &view, &proposal, &plan, vec![])
            .await
    }

    async fn wallet_sign_and_send(
        &mut self,
        wallet: Principal,
        view: &WalletView,
        proposal: &Proposal,
        plan: &ExecutionPlan,
        cosigs: Vec<CosignerSig>,
    ) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
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
        out["signed"] = json!({"plan": signed.plan, "txid": signed.txid});
        match self.wallet_broadcast_signed(view, proposal, &signed).await {
            Ok(txid) => {
                let reported = self
                    .wallet_report(
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

    async fn wallet_broadcast_signed(
        &mut self,
        view: &WalletView,
        proposal: &Proposal,
        signed: &SignedTx,
    ) -> Result<String> {
        let chains = rpc::Chains::new(&self.config.wallet)?;
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

    /// Broadcast a signed plan again and report it.
    pub async fn wallet_broadcast(
        &mut self,
        wallet: Principal,
        id: u64,
        plan: Option<u32>,
    ) -> Result<Json> {
        let view = self.wallet_state(wallet).await?;
        let proposal = self.wallet_proposal_raw(wallet, id).await?;
        let signed = match plan {
            Some(seq) => proposal.signed.iter().find(|s| s.plan == seq),
            None => proposal.signed.last(),
        }
        .cloned()
        .ok_or(Error::NotFound)?;
        let txid = self
            .wallet_broadcast_signed(&view, &proposal, &signed)
            .await?;
        let mut out = self
            .wallet_report(
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

    /// Check the chain for each signed plan, newest first, and report the
    /// first one final as settled or failed: after a fee bump, an earlier
    /// version may be the one that lands.
    pub async fn wallet_settle(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        let proposal = self.wallet_proposal_raw(wallet, id).await?;
        let latest = proposal.signed.last().cloned().ok_or(Error::NotFound)?;
        let chain = external_source(&proposal.intent).map(|(_, c)| c);
        let chains = rpc::Chains::new(&self.config.wallet)?;
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
            return self
                .wallet_report(wallet, id, signed.plan, outcome, note)
                .await;
        }
        match error {
            Some(e) if !pending => Err(e),
            _ => Ok(json!({"pending": true, "txid": latest.txid})),
        }
    }

    /// Report a signed plan's fate.
    pub async fn wallet_report(
        &mut self,
        wallet: Principal,
        id: u64,
        plan: u32,
        outcome: ChainOutcome,
        note: &str,
    ) -> Result<Json> {
        let (_, ic) = self.wallet_identity().await?;
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
        Ok(proposal_view(&p?))
    }

    /// Balances of every address: ICP and ICRC from the ledgers, the other
    /// chains from the configured endpoints, each with its source; every
    /// one is read side by side.
    pub async fn wallet_balances(&mut self, wallet: Principal) -> Result<Json> {
        let view = self.wallet_state(wallet).await?;
        let hub = self.wallet_hub()?;
        let reader = self.anonymous().await?;
        let config: HubInit = reader.query(hub, "get_hub_config", ()).await?;
        let chains = rpc::Chains::new(&self.config.wallet)?;
        let (view, reader, chains) = (&view, &reader, &chains);
        let (icp_ledger, bitcoin_api) = (config.icp_ledger, &self.config.wallet.bitcoin_api);
        let assets = &view.policy.spendable_assets;
        let out = join_all(view.addresses.iter().map(|a| async move {
            let balances: Vec<Json> = match a.chain {
                Chain::Icp => {
                    let icrc = assets.iter().filter_map(|asset| match asset {
                        AssetKey::Icrc(ledger) => Some(*ledger),
                        _ => None,
                    });
                    join_all(std::iter::once(icp_ledger).chain(icrc).map(|ledger| async move {
                        let account = icp_account(wallet, a.id);
                        let n: Result<candid::Nat> =
                            reader.query(ledger, "icrc1_balance_of", (account,)).await;
                        json!({
                            "ledger": ledger.to_text(),
                            "asset": if ledger == icp_ledger { "ICP" } else { "ICRC" },
                            "balance": shown(n.map(|n| n.0)),
                        })
                    }))
                    .await
                }
                Chain::Bitcoin => {
                    let b = chains.btc_balance(&a.text).await;
                    vec![json!({"asset": "BTC", "balance": shown(b), "source": bitcoin_api})]
                }
                Chain::Evm => {
                    let reads = view.policy.evm_chains.iter().flat_map(|chain_id| {
                        let tokens = assets.iter().filter_map(move |asset| match asset {
                            AssetKey::Evm {
                                chain_id: c,
                                token: Some(token),
                            } if c == chain_id => Some(eip55(&token.into_array())),
                            _ => None,
                        });
                        std::iter::once(None)
                            .chain(tokens.map(Some))
                            .map(move |token| (*chain_id, token))
                    });
                    join_all(reads.map(|(chain_id, token)| async move {
                        let b = chains.evm_balance(chain_id, &a.text, token.as_deref()).await;
                        match token {
                            None => {
                                json!({"asset": "ETH", "chain_id": chain_id, "balance": shown(b)})
                            }
                            Some(token) => {
                                json!({"asset": "ERC20", "token": token, "chain_id": chain_id, "balance": shown(b)})
                            }
                        }
                    }))
                    .await
                }
                Chain::Solana => {
                    let mints = assets.iter().filter_map(|asset| match asset {
                        AssetKey::Sol { mint: Some(mint) } => Some(sol_address(&mint.into_array())),
                        _ => None,
                    });
                    let sol = async {
                        let b = chains.sol_balance(&a.text).await;
                        json!({"asset": "SOL", "balance": shown(b)})
                    };
                    let tokens = join_all(mints.map(|mint| async move {
                        let b = chains.sol_token(&a.text, &mint).await;
                        json!({"asset": "SPL", "mint": mint, "balance": shown(b.map(|(b, _)| b))})
                    }));
                    let (sol, tokens) = join(sol, tokens).await;
                    std::iter::once(sol).chain(tokens).collect()
                }
            };
            json!({"address": a.id, "chain": format!("{:?}", a.chain), "text": a.text, "balances": balances})
        }))
        .await;
        Ok(json!({"wallet": wallet.to_text(), "read_at": now_ms(), "addresses": out}))
    }

    /// Chain parameters for a plan, read fresh: UTXOs and a fee rate,
    /// the pending nonce and fees, or a blockhash or durable nonce value.
    async fn wallet_params(
        &mut self,
        view: &WalletView,
        p: &Proposal,
        a: &Address,
        chain: Chain,
    ) -> Result<ExecutionParams> {
        let chains = rpc::Chains::new(&self.config.wallet)?;
        Ok(match chain {
            Chain::Bitcoin => {
                // A replacement spends the first signed plan's inputs.
                let utxos = match p.signed.first() {
                    Some(first) => {
                        let (_, ic) = self.wallet_identity().await?;
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
                let rate = chains.btc_fee_rate().await?;
                ExecutionParams::Bitcoin {
                    utxos,
                    // A replacement must pay more than the transaction it replaces.
                    fee_rate: if p.signed.is_empty() { rate } else { rate + 1 },
                    locktime: 0,
                    timelock: false,
                }
            }
            Chain::Evm => {
                let chain_id = evm_chain(&p.intent)?;
                let nonce = match (&p.intent, p.signed.first()) {
                    // A replacement keeps the nonce of the transaction it replaces.
                    (_, Some(first)) => {
                        let (_, ic) = self.wallet_identity().await?;
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
                        u64::from_str_radix(
                            v.as_str().unwrap_or_default().trim_start_matches("0x"),
                            16,
                        )
                        .map_err(|_| Error::Unavailable("nonce".into()))?
                    }
                    _ => chains.evm_nonce(chain_id, &a.text).await?,
                };
                let (mut max_fee, mut tip) = chains.evm_fees(chain_id).await?;
                if !p.signed.is_empty() || matches!(p.intent, Intent::CancelEvmNonce { .. }) {
                    // Replacements need at least a tenth more.
                    max_fee = max_fee * 12 / 10;
                    tip = tip * 12 / 10;
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
                let (decimals, create_ata) = match &p.intent {
                    Intent::Transfer {
                        asset: AssetKey::Sol { mint: Some(mint) },
                        to: Destination::Solana(owner),
                        ..
                    } => {
                        let mint_text = sol_address(&mint.into_array());
                        let (_, decimals) = chains.sol_token(&a.text, &mint_text).await?;
                        let target = decode::associated_token_account(
                            &owner.into_array(),
                            &mint.into_array(),
                        );
                        let exists = chains.sol_exists(&sol_address(&target)).await?;
                        (decimals, !exists)
                    }
                    _ => (None, false),
                };
                ExecutionParams::Solana {
                    blockhash,
                    compute_unit_price: 1_000,
                    compute_unit_limit: if create_ata { 60_000 } else { 20_000 },
                    decimals,
                    create_ata,
                }
            }
            Chain::Icp => return Err(Error::InvalidInput("ICP needs no plan".into())),
        })
    }

    /// The wallet cosigner ID of a key of this device named by `address`'s
    /// cosign policy, if any.
    fn local_cosigner_of(&self, view: &WalletView, address: &Address) -> Result<Option<u32>> {
        let Some(policy) = &address.cosign else {
            return Ok(None);
        };
        let local = self.wallet_cosigner_keys()?;
        Ok(view
            .cosigners
            .iter()
            .filter(|c| policy.cosigners.contains(&c.id))
            .find(|c| local.iter().any(|k| k.public_key == c.input.public_key))
            .map(|c| c.id))
    }

    /// Cosignatures by this device's key of wallet cosigner `cosigner` over
    /// `digests` of plan `plan`, plan 0 being an ICP proposal's digest.
    fn local_cosigs(
        &self,
        view: &WalletView,
        cosigner: u32,
        proposal: u64,
        plan: u32,
        digests: &[Hash],
    ) -> Result<Vec<CosignerSig>> {
        let c = view
            .cosigners
            .iter()
            .find(|c| c.id == cosigner)
            .ok_or(Error::NotFound)?;
        digests
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let message = cosign_message(view.wallet, proposal, plan, i as u32, d);
                Ok(CosignerSig {
                    cosigner,
                    digest_index: i as u32,
                    signature: self
                        .wallet_cosign(&c.input.public_key, &message[..])?
                        .into(),
                    authenticator_data: None,
                    client_data_json: None,
                })
            })
            .collect()
    }

    /// The registration of the wallet application and product, certified.
    async fn wallet_registrations(&mut self) -> Result<(AppRegistration, ProductRegistration)> {
        let init: dmsg_types::user::UserInit = self.home_config().await?;
        let commerce = init.commerce_canister;
        let ic = self.anonymous().await?;
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
            app.environment == self.config.environment && app.origins.iter().any(|o| o == ORIGIN),
            Error::IntegrityFailed,
        )?;
        Ok((app, product))
    }

    /// Sign this device's approval of `purpose` for `service` and submit it
    /// to the user home; the saved approval is sent again on a retry.
    async fn wallet_authorize(
        &mut self,
        job: &mut WalletJob,
        hub: Principal,
        service: Principal,
        purpose: ApprovalPurpose,
        action_digest: Hash,
    ) -> Result<(Hash, ApplicationApproval)> {
        if job.approval.is_none() {
            let (app, _) = self.wallet_registrations().await?;
            let (actor, _) = self.wallet_identity().await?;
            let account = self.account()?.clone();
            let state = self.refresh(account.home, &account.id).await?;
            let device = state.active(&self.workspace.meta.device_id)?;
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
                device_id: self.workspace.meta.device_id,
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
            approval.signature =
                Ed25519Signature::new(sign(&self.keys()?.signing, message.as_slice()));
            job.approval = Some((application, approval));
            self.save_wallet_job(job)?;
        }
        let (application, approval) = job.approval.clone().ok_or(Error::IntegrityFailed)?;
        let home = self.account()?.home;
        let login = self.session()?.ic.clone();
        let approved: Result<Hash> = login
            .update(home, "approve_application", (&application, &approval))
            .await?;
        approved?;
        Ok((approval.request_id, application))
    }

    /// Run an approved wallet job.
    pub(crate) async fn wallet_job_execute(&mut self, id: &str) -> Result<()> {
        let mut job = self.wallet_job(id)?;
        let result = match job.kind.clone() {
            JobKind::Create { quote, spec } => {
                let hub = self.wallet_hub()?;
                self.wallet_pay_quote(&quote).await?;
                let digest = activation_digest(&quote, &spec);
                let (approval_id, approval) = self
                    .wallet_authorize(&mut job, hub, hub, ApprovalPurpose::WalletCreate, digest)
                    .await?;
                let (_, ic) = self.wallet_identity().await?;
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
                    self.workspace
                        .delete_private(&format!("{QUOTE}{}", hex::encode(*quote.quote_id)))?;
                }
                json!({
                    "status": format!("{:?}", order.status),
                    "wallet": order.wallet.map(|w| w.to_text()),
                })
            }
            JobKind::Join { wallet, .. } => {
                let hub = self.wallet_hub()?;
                let (actor, _) = self.wallet_identity().await?;
                let account = self.account()?.id;
                let (approval_id, approval) = self
                    .wallet_authorize(
                        &mut job,
                        hub,
                        wallet,
                        ApprovalPurpose::WalletJoin,
                        join_digest(wallet, &account, actor),
                    )
                    .await?;
                let (_, ic) = self.wallet_identity().await?;
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
                let hub = self.wallet_hub()?;
                let (approval_id, approval) = self
                    .wallet_authorize(
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
                let (_, ic) = self.wallet_identity().await?;
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
                cosigner,
                ..
            } => {
                let view = self.wallet_state(wallet).await?;
                match plan {
                    Some(plan) => {
                        decode::verify_plan(&view, &proposal.intent, &plan)?;
                        let cosigs = self.local_cosigs(
                            &view,
                            cosigner,
                            proposal.id,
                            plan.seq,
                            &plan.digests,
                        )?;
                        self.wallet_sign_and_send(wallet, &view, &proposal, &plan, cosigs)
                            .await?
                    }
                    None => {
                        let cosigs =
                            self.local_cosigs(&view, cosigner, proposal.id, 0, &[proposal.digest])?;
                        let (_, ic) = self.wallet_identity().await?;
                        let r: Result<ExecuteResult> = ic
                            .update(
                                wallet,
                                "execute",
                                (ExecuteArgs {
                                    proposal: proposal.id,
                                    plan: None,
                                    cosigs,
                                },),
                            )
                            .await?;
                        proposal_view(&r?.proposal)
                    }
                }
            }
        };
        job.result = Some(result);
        self.save_wallet_job(&job)
    }

    /// Pay a quote's price into its deposit from the payer, once. A first
    /// payment needs a minute left on the quote for the hub to create the
    /// wallet; [`Self::wallet_refund`] returns a deposit no wallet used.
    async fn wallet_pay_quote(&mut self, quote: &ActivationQuote) -> Result<u64> {
        let key = format!("{ACTIVATION}{}", hex::encode(*quote.quote_id));
        let now = now_ms();
        if self.payment_journal(&key)?.is_none() {
            ensure(
                now + dmsg_types::MINUTE < quote.expires_at_ms,
                Error::Expired,
            )?;
        }
        let hub = self.wallet_hub()?;
        let reader = self.anonymous().await?;
        let config: HubInit = reader.query(hub, "get_hub_config", ()).await?;
        let deposit: Account = quote.deposit;
        let amount = u128::from(quote.price_e8s);
        let created_at = now * 1_000_000;
        self.pay(
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

    /// Generate a cosigner key on this device and return the input of the
    /// `AddCosigner` proposal that registers it for this account.
    pub fn wallet_cosigner_new(&mut self, label: &str, hardware: bool) -> Result<Json> {
        let key = self.wallet_cosigner_create(label, hardware)?;
        let input = key.input(self.wallet_member()?);
        Ok(json!({
            "key": key.view(),
            "intent": serde_json::to_value(Intent::AddCosigner(input)).map_err(|e| Error::InvalidInput(e.to_string()))?,
        }))
    }

    /// This device's wallet key Principal, which joins bind.
    pub async fn wallet_principal(&mut self) -> Result<Json> {
        let (principal, _) = self.wallet_identity().await?;
        Ok(json!({"principal": principal.to_text()}))
    }
}

/// Parse an asset: `icp`, `icrc:<ledger>`, `btc`, `evm:<chain>` or
/// `evm:<chain>:<0x token>`, `sol` or `sol:<mint>`.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn parse_asset(text: &str) -> Result<AssetKey> {
    let parts: Vec<&str> = text.split(':').collect();
    Ok(match parts.as_slice() {
        ["icp"] => AssetKey::Icp,
        ["icrc", ledger] => AssetKey::Icrc(principal(ledger)?),
        ["btc"] => AssetKey::Btc,
        ["evm", chain] => AssetKey::Evm {
            chain_id: chain.parse().map_err(|_| dmsg_types::invalid("asset"))?,
            token: None,
        },
        ["evm", chain, token] => AssetKey::Evm {
            chain_id: chain.parse().map_err(|_| dmsg_types::invalid("asset"))?,
            token: Some(Hash20::new(parse_evm_address(token)?)),
        },
        ["sol"] => AssetKey::Sol { mint: None },
        ["sol", mint] => AssetKey::Sol {
            mint: Some(Hash::new(parse_sol_address(mint)?)),
        },
        _ => return Err(dmsg_types::invalid("asset")),
    })
}

/// Parse a recipient for `asset`: an ICRC account text, a Bitcoin address,
/// an EVM address or a Solana address.
///
/// # Errors
/// Returns `Error::InvalidInput` when it does not fit the asset.
pub fn parse_destination(asset: &AssetKey, text: &str, network: BtcNetwork) -> Result<Destination> {
    Ok(match asset {
        AssetKey::Icp | AssetKey::Icrc(_) => Destination::Icrc(
            text.parse::<Account>()
                .map_err(|_| dmsg_types::invalid("account"))?,
        ),
        AssetKey::Btc => {
            btc_script(network, text)?;
            Destination::Bitcoin(text.into())
        }
        AssetKey::Evm { .. } => Destination::Evm(Hash20::new(parse_evm_address(text)?)),
        AssetKey::Sol { .. } => Destination::Solana(Hash::new(parse_sol_address(text)?)),
    })
}

/// A starting policy: `approvals` Approver approvals for spending,
/// `admin_approvals` Admin approvals for changes, every default chain
/// spendable, proposals open a week.
pub fn default_policy(approvals: u8, admin_approvals: u8) -> Policy {
    Policy {
        version: 1,
        approver_threshold: approvals,
        admin_threshold: admin_approvals,
        tiers: vec![],
        address_book_cooldown_ms: dmsg_types::DAY,
        daily_limits: vec![],
        spendable_assets: vec![
            AssetKey::Icp,
            AssetKey::Btc,
            AssetKey::Evm {
                chain_id: 1,
                token: None,
            },
            AssetKey::Evm {
                chain_id: 8453,
                token: None,
            },
            AssetKey::Sol { mint: None },
        ],
        call_targets: vec![],
        evm_chains: vec![1, 8453],
        proposal_ttl_ms: 7 * dmsg_types::DAY,
        unfreeze_delay_ms: dmsg_types::DAY,
        daily_signatures: 50,
        refuel: None,
        hardware_cosigners: false,
        public: false,
    }
}

/// The source address of an ICP or ICRC intent, which a cosign policy covers.
fn source_address(intent: &Intent) -> Option<u32> {
    match intent {
        Intent::Transfer { from, asset, .. } | Intent::Batch { from, asset, .. }
            if asset_chain(asset) == Chain::Icp =>
        {
            Some(*from)
        }
        Intent::Refuel { from, .. } => Some(*from),
        _ => None,
    }
}

/// The source and chain of an intent the wallet signs for another chain.
pub fn external_source(intent: &Intent) -> Option<(u32, Chain)> {
    match intent {
        Intent::Transfer { from, asset, .. } | Intent::Batch { from, asset, .. } => {
            let chain = asset_chain(asset);
            (chain != Chain::Icp).then_some((*from, chain))
        }
        Intent::CancelEvmNonce { address, .. } => Some((*address, Chain::Evm)),
        Intent::InitSolNonce { address, .. } => Some((*address, Chain::Solana)),
        _ => None,
    }
}

fn evm_chain(intent: &Intent) -> Result<u64> {
    match intent {
        Intent::Transfer {
            asset: AssetKey::Evm { chain_id, .. },
            ..
        }
        | Intent::CancelEvmNonce { chain_id, .. } => Ok(*chain_id),
        _ => Err(Error::InvalidInput("not an EVM intent".into())),
    }
}

#[cfg(test)]
mod tests;
