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

pub mod audit;
pub mod cosign;
pub mod decode;
pub mod desktop;
pub mod native;
pub mod nc_migrate;
pub mod ops;
pub mod packets;
pub mod rotate;
pub mod rpc;
mod snapshot;

pub use snapshot::WalletSnapshot;

use crate::client::Client;
use crate::codec::principal_input;
use crate::crypto::{random_id, sign};
use crate::hold::{Hold, Unlocked};
use crate::ic::Ic;
use crate::now_ms;
use crate::payer::Stage;
use crate::payment::certified_leaf;
use candid::{Nat, Principal};
use dmsg_protocol::{approval_message, digest, integration::*, wallet::*};
use dmsg_types::{
    Approval, CertifiedBatch, Ed25519Signature, Error, Hash, Result,
    billing::PlanId,
    ensure,
    integration::{AppRegistration, ApplicationApproval, ApprovalPurpose, ProductRegistration},
    user::Capability,
    wallet::*,
};
use futures_util::future::{join, join_all, try_join_all};
use futures_util::stream::{self, StreamExt, TryStreamExt};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

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
        /// Devices of this member whose bindings this device takes over.
        #[serde(default)]
        replaces: Vec<Hash>,
        /// Take over the bindings of every other device.
        #[serde(default)]
        exclusive: bool,
    },
    /// Remove the bindings of this member's devices, such as lost ones.
    Unbind {
        /// The wallet.
        wallet: Principal,
        /// Its name.
        name: String,
        /// The devices, ascending.
        devices: Vec<Hash>,
    },
    /// Decide on a proposal.
    Decide {
        /// The wallet.
        wallet: Principal,
        /// Its name.
        name: String,
        /// Certified state against which the change is reviewed.
        context: Box<WalletSnapshot>,
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
        /// Certified state against which the change is reviewed.
        context: Box<WalletSnapshot>,
        /// The proposal.
        proposal: Box<Proposal>,
        /// The plan, checked by the decoder; none for ICP and ICRC.
        plan: Option<Box<ExecutionPlan>>,
        /// The wallet's cosigner ID of the local key.
        cosigners: Vec<u32>,
        /// Collect a portable packet instead of executing immediately.
        collect_only: bool,
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
            JobKind::Join {
                wallet,
                name,
                replaces,
                exclusive,
            } => json!({
                "action": "join",
                "wallet": wallet.to_text(),
                "name": name,
                "replaces": replaces.iter().map(|d| hex::encode(**d)).collect::<Vec<_>>(),
                "exclusive": exclusive,
            }),
            JobKind::Unbind {
                wallet,
                name,
                devices,
            } => json!({
                "action": "unbind",
                "wallet": wallet.to_text(),
                "name": name,
                "devices": devices.iter().map(|d| hex::encode(**d)).collect::<Vec<_>>(),
            }),
            JobKind::Decide {
                wallet,
                name,
                proposal,
                decision,
                context,
            } => json!({
                "action": "decide",
                "decision": format!("{decision:?}").to_lowercase(),
                "wallet": wallet.to_text(),
                "name": name,
                "proposal": proposal_view(proposal),
                "before": wallet_view(context.view(), None),
            }),
            JobKind::Cosign {
                wallet,
                name,
                proposal,
                plan,
                cosigners,
                collect_only,
                context,
            } => json!({
                "action": "cosign",
                "wallet": wallet.to_text(),
                "name": name,
                "proposal": proposal_view(proposal),
                "before": wallet_view(context.view(), None),
                "plan": plan.as_deref().map(plan_view),
                "cosigners": cosigners,
                "collect_only": collect_only,
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
    let mut out = match asset {
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
    };
    out["key"] = json!(desktop::asset_key(asset));
    out
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
            "control_details": kind,
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
        Intent::ReplaceCosigner { id, input } => json!({
            "kind": "replace_cosigner",
            "cosigner": id,
            "member": member_id_text(&input.member),
            "curve": format!("{:?}", input.kind),
            "public_key": hex::encode(&input.public_key),
            "envelope": format!("{:?}", input.envelope),
            "hardware": input.hardware,
            "label": input.label,
        }),
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
            proposal,
            nonce,
            max_fee,
        } => {
            json!({"kind": "cancel_evm_nonce", "address": address, "chain_id": chain_id, "proposal": proposal, "nonce": nonce.to_string(), "max_fee": max_fee.to_string()})
        }
        Intent::ReleaseEvmNonce {
            address,
            chain_id,
            proposal,
            nonce,
        } => {
            json!({"kind": "release_evm_nonce", "address": address, "chain_id": chain_id, "proposal": proposal, "nonce": nonce.to_string()})
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
        "code_acceptance": view.code,
        "created_at": view.created_at_ms,
        "next_proposal": view.next_proposal,
        "audit_len": view.audit_len,
        "audit_head": hex::encode(*view.audit_head),
        "channel_hint": view.channel_hint,
        "me": me.and_then(|me| view.members.iter().find(|m| m.id == *me)).map(|m| json!({
            "id": member_id_text(&m.id),
            "role": format!("{:?}", m.role),
            "status": format!("{:?}", m.status),
        })),
        "members": view.members.iter().map(|m| json!({
            "id": member_id_text(&m.id),
            "kind": if matches!(m.id, MemberId::Account(_)) { "account" } else { "principal" },
            "role": format!("{:?}", m.role),
            "label": m.label,
            "status": format!("{:?}", m.status),
            "principals": m.bindings.iter().map(|b| b.principal.to_text()).collect::<Vec<_>>(),
            "bindings": m.bindings.iter().map(|b| json!({
                "principal": b.principal.to_text(),
                "device": b.device.map(|d| hex::encode(*d)),
                "bound_at": b.bound_at_ms,
            })).collect::<Vec<_>>(),
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
            "control_details": match &a.kind { AddressKind::Taproot(control) => serde_json::to_value(control).ok(), _ => None },
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

/// Parse a wallet Principal.
///
/// # Errors
/// Returns `Error::InvalidInput` for anything else.
pub fn parse_wallet(text: &str) -> Result<Principal> {
    principal_input(text)
}

impl Client {
    fn wallet_hub(&self) -> Result<Principal> {
        self.config
            .wallet
            .hub
            .ok_or_else(|| Error::Unavailable("team wallets are not configured".into()))
    }

    /// The seed of this device's wallet key, made once.
    fn wallet_seed(&self) -> Result<serde_bytes::ByteArray<32>> {
        let keys = self.keys()?;
        Ok(match self.workspace.private(keys, WALLET_KEY)? {
            Some(seed) => seed,
            None => {
                let seed = serde_bytes::ByteArray::new(crate::crypto::random_key());
                self.workspace.put_private(keys, WALLET_KEY, &seed)?;
                seed
            }
        })
    }

    /// The Principal of this device's wallet key, if it made one; it calls
    /// the wallets as [`identity`] does.
    pub(crate) fn wallet_principal(&self) -> Result<Option<Principal>> {
        let seed: Option<serde_bytes::ByteArray<32>> =
            self.workspace.private(self.keys()?, WALLET_KEY)?;
        seed.map(|seed| {
            let key = ic_agent::identity::BasicIdentity::from_raw_key(&seed.into_array());
            ic_agent::Identity::sender(&key).map_err(|_| Error::AuthRequired)
        })
        .transpose()
    }

    fn wallet_member(&self) -> Result<MemberId> {
        Ok(MemberId::Account(self.account()?.id))
    }

    /// The wallet as this device's wallet key reads it, certified, so no
    /// single replica can show other addresses, members or policy.
    pub async fn wallet_state(&mut self, wallet: Principal) -> Result<WalletView> {
        state(&self.hold(), wallet).await
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
        let now = now_ms();
        let request = self.approval_create_at(
            source,
            crate::approval::Action::Wallet {
                job: job.id.clone(),
                digest: job.digest(),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
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
    /// Returns `Error::NotFound` for a quote this device did not request and
    /// `Error::QuotaExceeded` when the quote's plan has no wallet left, so no
    /// payment starts for a wallet the hub would refuse. An unfunded quote
    /// returns `INSUFFICIENT_FUNDS` without creating an approval request.
    pub async fn wallet_create_request(
        &mut self,
        source: &str,
        quote_id: &str,
        spec: WalletSpec,
    ) -> Result<Json> {
        ops::create_request(&self.hold(), source, quote_id, spec).await
    }

    /// Return the deposit of quote `quote_id`, less the ledger fee, to the
    /// identity that paid it, once the hub failed to create its wallet or
    /// the quote expired unused. The hub takes the claim from this device's
    /// wallet key, which requested the quote, and pays the account it names.
    /// The quote stays tracked so later deposits can also be refunded.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for a quote this device did not request or
    /// no longer keeps, or an empty deposit, and `Error::Forbidden` for a
    /// quote still usable or one that created its wallet.
    pub async fn wallet_refund(&mut self, quote_id: &str) -> Result<Json> {
        ops::refund(&self.hold(), quote_id).await
    }

    /// The activation quotes this device paid, newest first.
    fn paid_quotes(&self) -> Result<Vec<ActivationQuote>> {
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
        Ok(paid)
    }

    /// Ask to decide on proposal `id`.
    pub async fn wallet_decide_request(
        &mut self,
        source: &str,
        wallet: Principal,
        id: u64,
        decision: Decision,
    ) -> Result<Json> {
        ops::decide_request(&self.hold(), source, wallet, id, decision).await
    }

    /// Propose `intent` with this device's wallet key; an agent proposing
    /// for the member names its Agent ID.
    pub async fn wallet_propose(
        &mut self,
        wallet: Principal,
        intent: Intent,
        via_agent: Option<String>,
    ) -> Result<Json> {
        ops::propose(&self.hold(), wallet, intent, via_agent).await
    }

    /// Propose that this account's member lets `agent_ids` propose on its
    /// behalf, keeping its role and label: the wallet accepts an agent's
    /// proposal only for a member that lists its Agent ID.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when this account is not a member.
    pub async fn wallet_member_agents(
        &mut self,
        wallet: Principal,
        agent_ids: Vec<String>,
    ) -> Result<Json> {
        ops::member_agents(&self.hold(), wallet, agent_ids).await
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
        ops::send(
            &self.hold(),
            wallet,
            from,
            asset,
            to,
            amount,
            max_fee,
            memo,
            via_agent,
        )
        .await
    }

    /// Withdraw this member's own proposal.
    pub async fn wallet_cancel(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        ops::cancel(&self.hold(), wallet, id).await
    }

    /// Stop the wallet's executions at once.
    pub async fn wallet_freeze(&mut self, wallet: Principal) -> Result<Json> {
        ops::freeze(&self.hold(), wallet).await
    }

    /// Top the wallet up under its refuel rule.
    pub async fn wallet_refuel(&mut self, wallet: Principal) -> Result<Json> {
        ops::refuel(&self.hold(), wallet).await
    }

    /// Resend an interrupted ICRC transfer or top-up with its exact arguments.
    pub async fn wallet_reconcile(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        ops::reconcile(&self.hold(), wallet, id).await
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
        ops::execute(&self.hold(), source, wallet, id).await
    }

    /// Broadcast a signed plan again and report it.
    pub async fn wallet_broadcast(
        &mut self,
        wallet: Principal,
        id: u64,
        plan: Option<u32>,
    ) -> Result<Json> {
        ops::broadcast(&self.hold(), wallet, id, plan).await
    }

    /// Check the chain for each signed plan, newest first, and report the
    /// first one final as settled or failed: after a fee bump, an earlier
    /// version may be the one that lands.
    pub async fn wallet_settle(&mut self, wallet: Principal, id: u64) -> Result<Json> {
        ops::settle(&self.hold(), wallet, id).await
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
        ops::report(&self.hold(), wallet, id, plan, outcome, note).await
    }

    /// The chain endpoints, over one HTTP client for the client's life.
    fn chains(&mut self) -> Result<rpc::Chains> {
        if self.chains.is_none() {
            self.chains = Some(rpc::Chains::new(&desktop::settings(self)?)?);
        }
        self.chains
            .clone()
            .ok_or(Error::Unavailable("chains".into()))
    }

    /// The wallet's cosigners whose keys this device holds.
    fn held_cosigners(&self, view: &WalletView) -> Result<Vec<u32>> {
        let local = self.wallet_cosigner_keys()?;
        let mut ids = vec![];
        for c in &view.cosigners {
            let held = match c.input.kind {
                CosignerKind::Bip340 => self.native_holds(view.wallet, &c.input.public_key)?,
                _ => local.iter().any(|k| k.public_key == c.input.public_key),
            };
            if held {
                ids.push(c.id);
            }
        }
        Ok(ids)
    }

    /// Eligible keys of this device used by any of the requirements: the
    /// address policies' keys, or the native key of an L1 spend, which no
    /// hardware policy filters.
    fn local_cosigners_for(
        &self,
        view: &WalletView,
        requirements: &[CosignRequirement],
    ) -> Result<Vec<u32>> {
        let native = packets::native(view, requirements);
        let held = self.held_cosigners(view)?;
        Ok(view
            .cosigners
            .iter()
            .filter(|c| {
                held.contains(&c.id)
                    && (native || !view.policy.hardware_cosigners || c.input.hardware)
                    && requirements
                        .iter()
                        .any(|r| r.policy.cosigners.contains(&c.id))
            })
            .map(|c| c.id)
            .collect())
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
}

/// This device's wallet key: its Principal and agent, made once without the
/// client.
///
/// # Errors
/// Returns `Error::Locked` while locked.
pub(crate) async fn identity<H: Hold>(h: &H) -> Result<(Principal, Ic)> {
    let (seed, config) = match h
        .with(|c| {
            c.keys()?;
            match &c.wallet_ic {
                Some(identity) => Ok(Err(identity.clone())),
                None => Ok(Ok((c.wallet_seed()?, c.config.clone()))),
            }
        })
        .await?
    {
        Err(identity) => return Ok(identity),
        Ok(made) => made,
    };
    let key = ic_agent::identity::BasicIdentity::from_raw_key(&seed.into_array());
    let ic = Ic::connect(&config, std::sync::Arc::new(key)).await?;
    let pair = (ic.principal()?, ic);
    h.with(|c| {
        c.keys()?;
        Ok(c.wallet_ic.get_or_insert(pair).clone())
    })
    .await
}

/// The hub's terms for creating a wallet for this account, paid by this
/// device's wallet key; kept for [`wallet_create_request`].
///
/// # Errors
/// Returns `Error::Unavailable` without a hub.
pub async fn quote<H: Hold>(h: &H) -> Result<Json> {
    let (hub, account) = h.with(|c| Ok((c.wallet_hub()?, c.account()?.id))).await?;
    let (_, ic) = identity(h).await?;
    let quote: Result<ActivationQuote> = ic.update(hub, "quote_activation", (account,)).await?;
    let quote = quote?;
    let id = hex::encode(*quote.quote_id);
    h.with(|c| {
        c.workspace
            .put_private(c.keys()?, &format!("{QUOTE}{id}"), &quote)
    })
    .await?;
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

/// What this account's plan allows and what an activation costs now, read
/// without a stored quote. The hub answers from its own pricing and quota
/// records; the plan and ledger fee are read beside it, so the view is fast and
/// a plan with no wallet left reads as zero instead of failing.
///
/// # Errors
/// Returns `Error::Unavailable` without a hub.
pub async fn quote_preview<H: Hold>(h: &H) -> Result<Json> {
    let (hub, account) = h.with(|c| Ok((c.wallet_hub()?, c.account()?.id))).await?;
    let ic = h.anonymous().await?;
    let preview: Result<ActivationPreview> = ic
        .query(hub, "quote_activation_preview", (account,))
        .await?;
    let preview = preview?;
    let (plan, fee) = join(
        crate::commerce::plan(h),
        ledger_fee(&ic, preview.icp_ledger),
    )
    .await;
    let plan: PlanId = plan?;
    let fee = fee?;
    let remaining = preview
        .plans
        .iter()
        .find(|p| p.plan == plan)
        .map_or(0, |p| p.remaining);
    Ok(json!({
        "plan": format!("{plan:?}"),
        "remaining": remaining,
        "price_e8s": preview.price_e8s.to_string(),
        "ledger_fee_e8s": fee.to_string(),
        "cycles_to_wallet": preview.cycles_to_wallet.to_string(),
        "version": preview.version,
    }))
}

/// Current wallet creation quotas for the membership benefit list. This
/// public configuration read also works before an account is initialized.
pub(crate) async fn plan_quotas<H: Hold>(h: &H) -> Result<Vec<PlanQuota>> {
    let hub = h.with(|c| c.wallet_hub()).await?;
    let config: HubInit = h
        .anonymous()
        .await?
        .query(hub, "get_hub_config", ())
        .await?;
    Ok(config.pricing.quotas)
}

/// The ledger's current fee, in its atomic unit.
async fn ledger_fee(ic: &Ic, ledger: Principal) -> Result<u128> {
    let n: Nat = ic.query(ledger, "icrc1_fee", ()).await?;
    u128::try_from(n.0).map_err(|_| Error::IntegrityFailed)
}

/// The wallet as this device's wallet key reads it, certified, so no single
/// replica can show other addresses, members or policy.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an uncertified answer.
pub async fn state<H: Hold>(h: &H, wallet: Principal) -> Result<WalletView> {
    let (_, ic) = identity(h).await?;
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
pub async fn list<H: Hold>(h: &H) -> Result<Json> {
    let (hub, me) = h
        .with(|c| Ok((c.wallet_hub()?, c.wallet_member()?)))
        .await?;
    let ic = h.anonymous().await?;
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

/// This member's bindings in a wallet, each with the account's view of its
/// device: `current` for this device, `active`, `revoked`, or `unknown`, such
/// as a device a login recovery removed or the one that created the wallet.
/// `unbind` lists the revoked and unknown devices that an unbind can remove.
///
/// # Errors
/// Returns `Error::Forbidden` when this account is not a member.
pub async fn bindings<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let view = state(h, wallet).await?;
    let (actor, _) = identity(h).await?;
    let (me, this) = h
        .with(|c| Ok((c.wallet_member()?, c.workspace.meta.device_id)))
        .await?;
    let member = view
        .members
        .iter()
        .find(|m| m.id == me)
        .ok_or(Error::Forbidden)?;
    let account = crate::account::refreshed(h).await?;
    let mut unbind = vec![];
    let rows: Vec<Json> = member
        .bindings
        .iter()
        .map(|b| {
            let device = b.device.map(|d| (d, account.device(&d)));
            let shown = match device {
                Some((d, _)) if d == this => "current",
                Some((_, Some(x))) if x.revoked_at.is_none() => "active",
                Some((_, Some(_))) => "revoked",
                _ => "unknown",
            };
            if let Some((d, _)) = device
                && matches!(shown, "revoked" | "unknown")
            {
                unbind.push(hex::encode(*d));
            }
            json!({
                "principal": b.principal.to_text(),
                "device": b.device.map(|d| hex::encode(*d)),
                "bound_at": b.bound_at_ms,
                "state": shown,
            })
        })
        .collect();
    unbind.sort();
    Ok(
        json!({"wallet": wallet.to_text(), "bound": member.binds(&actor), "bindings": rows, "unbind": unbind}),
    )
}

/// The proposal giving cosigner `id` of this member the key `label` of this
/// device, after signing a local challenge with it, so a key that cannot sign
/// is never proposed.
///
/// # Errors
/// Returns `Error::NotFound` for an unknown cosigner or label and
/// `Error::Forbidden` for another member's cosigner.
pub async fn replacement<H: Hold>(
    h: &H,
    wallet: Principal,
    id: u32,
    label: &str,
) -> Result<Intent> {
    let view = state(h, wallet).await?;
    let current = view
        .cosigners
        .iter()
        .find(|c| c.id == id)
        .ok_or(Error::NotFound)?;
    cosign::test_key(h, label).await?;
    let input = h
        .with(|c| {
            let me = c.wallet_member()?;
            ensure(current.input.member == me, Error::Forbidden)?;
            let key = c
                .wallet_cosigner_keys()?
                .into_iter()
                .find(|k| k.label == label)
                .ok_or(Error::NotFound)?;
            Ok(key.input(me))
        })
        .await?;
    ensure(
        view.cosigners
            .iter()
            .all(|c| c.input.public_key != input.public_key),
        Error::IdempotencyConflict,
    )?;
    Ok(Intent::ReplaceCosigner { id, input })
}

/// A wallet with its addresses checked against its key roots.
///
/// # Errors
/// As [`state`].
pub async fn show<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let (view, stats) = futures_util::try_join!(
        state(h, wallet),
        ic.query::<_, WalletStats>(wallet, "wallet_stats", ())
    )?;
    let me = h.with(|c| Ok(c.wallet_member().ok())).await?;
    let mut out = wallet_view(&view, me.as_ref());
    // What only this device knows: addresses it archived after moving
    // their coins, and which cosigner keys it holds.
    let (archived, local) = h
        .with(|c| Ok((c.archived(wallet)?, c.held_cosigners(&view)?)))
        .await?;
    for a in out["addresses"].as_array_mut().into_iter().flatten() {
        a["archived"] = json!(
            a["id"]
                .as_u64()
                .is_some_and(|id| archived.contains(&(id as u32)))
        );
    }
    for k in out["cosigners"].as_array_mut().into_iter().flatten() {
        k["local"] = json!(
            k["id"]
                .as_u64()
                .is_some_and(|id| local.contains(&(id as u32)))
        );
    }
    out["environment"] = h
        .with(|c| {
            Ok(json!(crate::config::environment_name(
                &c.config.environment
            )))
        })
        .await?;
    out["stats"] = json!({
        "cycles": stats.cycles.to_string(),
        "signatures_today": stats.signatures_today,
        "proposals": stats.proposals,
        "active_proposals": stats.active_proposals,
        "stable_pages": stats.stable_pages,
    });
    Ok(out)
}

/// Proposals, newest first.
///
/// # Errors
/// Query failures return `Error::Unavailable`.
pub async fn proposals<H: Hold>(
    h: &H,
    wallet: Principal,
    before: Option<u64>,
    limit: u16,
) -> Result<Json> {
    let (_, ic) = identity(h).await?;
    let page: Result<ProposalPage> = ic.query(wallet, "list_proposals", (before, limit)).await?;
    let page = page?;
    Ok(json!({
        "proposals": page.proposals.iter().map(proposal_view).collect::<Vec<_>>(),
        "next": page.next,
    }))
}

/// A proposal whose digest matches what it shows, so a decision or
/// cosignature commits to exactly that, whichever replica answered.
async fn proposal_raw<H: Hold>(h: &H, wallet: Principal, id: u64) -> Result<Proposal> {
    let (_, ic) = identity(h).await?;
    let batch: Result<CertifiedBatch> = ic.query(wallet, "get_proposal_certified", (id,)).await?;
    let key = [b"proposal/".as_slice(), &id.to_be_bytes()].concat();
    let value =
        certified_leaf(&batch?, &key, wallet, &ic.root_key(), now_ms())?.ok_or(Error::NotFound)?;
    let mut p: Proposal = crate::codec::decode(&value, 4 << 20)?;
    let view = state(h, wallet).await?;
    let environment = h.with(|c| Ok(c.config.environment.clone())).await?;
    let digest = proposal_digest(
        &environment,
        wallet,
        &p.op_id,
        &p.intent,
        p.policy_version,
        &p.proposer,
        &p.via_agent,
    );
    ensure(p.id == id && p.digest == digest, Error::IntegrityFailed)?;
    let at = now_ms();
    if is_waiting(p.status) {
        if at >= p.expires_at_ms {
            p.status = ProposalStatus::Expired;
        } else if p.policy_version != view.policy.version {
            p.status = ProposalStatus::Stale;
        } else if p.status == ProposalStatus::Delayed
            && p.ready_at_ms.is_some_and(|ready| at >= ready)
        {
            p.status = ProposalStatus::Ready;
        }
    }
    h.with(|c| {
        use crate::notifications::{self, Item, Target};
        let me = c.wallet_member()?;
        let now = now_ms();
        let target = Target::Wallet {
            wallet: wallet.to_text(),
            proposal: p.id.to_string(),
        };
        let account = c.account()?.id.to_string();
        let eligible = p.status == ProposalStatus::Open
            && p.expires_at_ms > now
            && !p.votes.iter().any(|vote| vote.member == me)
            && view.members.iter().any(|member| {
                member.id == me
                    && member.status == MemberStatus::Active
                    && dmsg_protocol::wallet::counts(member.role, &p.requirement)
            });
        if !eligible
            && c.workspace
                .store
                .notification_item(&notifications::item_id(&account, &target))?
                .is_none()
        {
            return Ok(());
        }
        let mut item = Item::new(account, target, view.audit_len.max(1), now);
        item.first_at = p.created_at_ms;
        item.deadline = Some(p.expires_at_ms);
        item.group = hex::encode(*crate::codec::digest(
            "dmsg/notification/wallet/v1",
            &wallet.to_text(),
        ));
        item.open = eligible;
        item.actionable = eligible;
        notifications::upsert(&c.workspace.store, item)?;
        Ok(())
    })
    .await?;
    Ok(p)
}

/// Refresh owner-actionable wallet proposals using the existing cached pending
/// history for discovery and certified proposal reads for notification evidence.
/// Partial reads never clear a wallet's old pending projections.
pub async fn refresh_notifications<H: Hold>(h: &H) -> Result<()> {
    let h = Unlocked::new(h).await?;
    let wallets = list(&h).await?;
    for row in wallets.as_array().ok_or(Error::IntegrityFailed)? {
        let wallet =
            crate::codec::principal(row["wallet"].as_str().ok_or(Error::IntegrityFailed)?)?;
        let history = desktop::history(&h, wallet, true).await?;
        let mut ids: std::collections::BTreeSet<u64> = history["proposals"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| row["id"].as_u64())
            .collect();
        h.with(|c| {
            for item in c.workspace.store.notification_items()? {
                if let crate::notifications::Target::Wallet {
                    wallet: previous,
                    proposal,
                } = item.target
                    && item.open
                    && previous == wallet.to_text()
                {
                    ids.insert(proposal.parse().map_err(|_| Error::IntegrityFailed)?);
                }
            }
            Ok(())
        })
        .await?;
        for id in ids {
            proposal_raw(&h, wallet, id).await?;
        }
    }
    Ok(())
}

/// One proposal with its plans.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a proposal whose digest does not
/// match.
pub async fn proposal<H: Hold>(h: &H, wallet: Principal, id: u64) -> Result<Json> {
    let p = proposal_raw(h, wallet, id).await?;
    let (_, ic) = identity(h).await?;
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
    // The key a replacement is reviewed against: the one the cosigner holds
    // now, which is the new one once it executed.
    if let Intent::ReplaceCosigner { id, .. } = &p.intent {
        let view = state(h, wallet).await?;
        out["current_key"] = view
            .cosigners
            .iter()
            .find(|c| c.id == *id)
            .map_or(Json::Null, |c| {
                json!({
                    "label": c.input.label,
                    "public_key": hex::encode(&c.input.public_key),
                    "hardware": c.input.hardware,
                })
            });
    }
    if let Intent::Transfer { from, asset, .. } | Intent::Batch { from, asset, .. } = &p.intent {
        let ledger = match asset {
            AssetKey::Icrc(ledger) => Some(*ledger),
            AssetKey::Icp => {
                let hub = h.with(|c| c.wallet_hub()).await?;
                let config: HubInit = h
                    .anonymous()
                    .await?
                    .query(hub, "get_hub_config", ())
                    .await?;
                Some(config.icp_ledger)
            }
            _ => None,
        };
        if let Some(ledger) = ledger {
            out["ledger"] = json!(ledger.to_text());
            out["source_account"] = json!(icp_account(wallet, *from).to_string());
        }
    }
    Ok(out)
}

/// Paid quotes with refundable, reserved or interrupted activation funds.
/// Completed quotes stay tracked so surplus and late deposits remain visible.
///
/// # Errors
/// Query failures return `Error::Unavailable`.
pub async fn deposits<H: Hold>(h: &H) -> Result<Json> {
    let h = Unlocked::new(h).await?;
    let (hub, paid) = h.with(|c| Ok((c.wallet_hub()?, c.paid_quotes()?))).await?;
    if paid.is_empty() {
        return Ok(Json::Array(vec![]));
    }
    let reader = h.anonymous().await?;
    let config: HubInit = reader.query(hub, "get_hub_config", ()).await?;
    let fee = ledger_fee(&reader, config.icp_ledger).await?;
    let read = stream::iter(paid.into_iter().map(|quote| {
        let reader = &reader;
        async move {
            let (order, reserved, held) = futures_util::try_join!(
                reader.query::<_, Option<ActivationOrder>>(hub, "get_order", (quote.quote_id,)),
                reader.query::<_, Result<u128>>(hub, "get_activation_reserve", (quote.quote_id,)),
                reader.query::<_, Nat>(config.icp_ledger, "icrc1_balance_of", (quote.deposit,))
            )?;
            Ok::<_, Error>((
                quote,
                order,
                reserved?,
                u128::try_from(held.0).map_err(|_| Error::IntegrityFailed)?,
            ))
        }
    }))
    .buffered(4)
    .try_collect::<Vec<_>>()
    .await?;
    let at = now_ms();
    let rows = read
        .into_iter()
        .filter_map(|(quote, order, reserved, held)| {
            let refund = held.saturating_sub(reserved).saturating_sub(fee);
            let (state, reason, abortable) = match order.as_ref().map(|o| &o.status) {
                Some(ActivationStatus::Ready) => ("surplus", None, false),
                Some(ActivationStatus::Created) => ("installing", None, true),
                Some(ActivationStatus::Paid) => ("creating", None, true),
                Some(ActivationStatus::Failed(reason)) => ("failed", Some(reason.clone()), false),
                Some(ActivationStatus::Refunded { .. }) => ("surplus", None, false),
                None if at >= quote.expires_at_ms => ("expired", None, false),
                None => ("unused", None, false),
            };
            if (held <= fee || state == "surplus" && refund == 0) && !abortable {
                return None;
            }
            Some(json!({
                "quote_id":hex::encode(*quote.quote_id),"state":state,"refundable":refund>0,
                "refund_e8s":refund.to_string(),"ledger_fee_e8s":fee.to_string(),
                "expires_at":quote.expires_at_ms,"reason":reason,"abortable":abortable
            }))
        })
        .collect::<Vec<_>>();
    h.with(|_| Ok(Json::Array(rows))).await
}

/// Verify the whole audit chain against the wallet's head.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a chain that does not end there.
pub async fn audit<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    audit::read(h, wallet, None).await
}

/// This device's wallet key Principal, which joins bind.
///
/// # Errors
/// Returns `Error::Locked` while locked.
pub async fn principal<H: Hold>(h: &H) -> Result<Json> {
    let (principal, _) = identity(h).await?;
    Ok(json!({"principal": principal.to_text()}))
}

/// Balances of every address: ICP and ICRC from the ledgers, the other
/// chains from the configured endpoints, each with its source; every one is
/// read side by side, without the client.
///
/// # Errors
/// As [`state`].
pub async fn balances<H: Hold>(h: &H, wallet: Principal) -> Result<Json> {
    let bound = Unlocked::new(h).await?;
    let h = &bound;
    let view = state(h, wallet).await?;
    let (hub, chains, bitcoin_api) = h
        .with(|c| {
            Ok((
                c.wallet_hub()?,
                c.chains()?,
                desktop::settings(c)?.bitcoin_api,
            ))
        })
        .await?;
    let reader = h.anonymous().await?;
    let config: HubInit = reader.query(hub, "get_hub_config", ()).await?;
    let (view, reader, chains) = (&view, &reader, &chains);
    let (icp_ledger, bitcoin_api) = (config.icp_ledger, &bitcoin_api);
    let assets = &view.policy.spendable_assets;
    let mut out = join_all(view.addresses.iter().map(|a| async move {
            let balances: Vec<Json> = match a.chain {
                Chain::Icp => {
                    let icrc = assets.iter().filter_map(|asset| match asset {
                        AssetKey::Icrc(ledger) if *ledger != icp_ledger => Some(*ledger),
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
    let settings = h.with(|c| desktop::settings(c)).await?;
    let mut assets = std::collections::BTreeMap::new();
    for address in &mut out {
        for balance in address["balances"].as_array_mut().into_iter().flatten() {
            let key = match balance["asset"].as_str().unwrap_or("") {
                "ICP" => "icp".into(),
                "ICRC" => format!("icrc:{}", balance["ledger"].as_str().unwrap_or("")),
                "BTC" => "btc".into(),
                "ETH" => format!("evm:{}", balance["chain_id"]),
                "ERC20" => format!(
                    "evm:{}:{}",
                    balance["chain_id"],
                    balance["token"].as_str().unwrap_or("")
                ),
                "SOL" => "sol".into(),
                "SPL" => format!("sol:{}", balance["mint"].as_str().unwrap_or("")),
                _ => continue,
            };
            let asset = parse_asset(&key)?;
            assets.insert(key.clone(), asset.clone());
            balance["key"] = json!(key);
            balance["source"] = match asset {
                AssetKey::Icp | AssetKey::Icrc(_) => balance["ledger"].clone(),
                AssetKey::Btc => json!(settings.bitcoin_api),
                AssetKey::Evm { chain_id, .. } => json!(
                    settings
                        .evm_rpc
                        .iter()
                        .find(|(id, _)| *id == chain_id)
                        .map(|(_, url)| url)
                ),
                AssetKey::Sol { .. } => json!(settings.solana_rpc),
            };
        }
    }
    let metadata: std::collections::BTreeMap<String, Json> =
        stream::iter(assets.into_iter().map(|(key, asset)| async move {
            Ok::<_, Error>((key, desktop::metadata(h, &asset, false).await?))
        }))
        .buffered(4)
        .try_collect()
        .await?;
    for address in &mut out {
        for balance in address["balances"].as_array_mut().into_iter().flatten() {
            if let Some(value) = balance["key"].as_str().and_then(|key| metadata.get(key)) {
                balance["metadata"] = value.clone();
            }
        }
    }
    h.with(|_| Ok(())).await?;
    Ok(json!({"wallet": wallet.to_text(), "read_at": now_ms(), "addresses": out}))
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
        ["icrc", ledger] => AssetKey::Icrc(principal_input(ledger)?),
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
