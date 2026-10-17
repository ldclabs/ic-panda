//! Stable state: the wallet record, proposals, plans, jobs, the address book,
//! the hash-chained audit log and the certified tree.
use candid::Principal;
use dmsg_protocol::{authenticated, canonical, wallet::*};
use dmsg_runtime::cert_map::CertMap;
use dmsg_runtime::storage::Stored;
use dmsg_types::{wallet::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;

/// Deployment settings fixed at install, except appended user homes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Settings {
    pub(crate) environment: Environment,
    pub(crate) hub: Principal,
    pub(crate) key_name: String,
    pub(crate) btc_network: BtcNetwork,
    pub(crate) icp_ledger: Principal,
    pub(crate) cmc: Principal,
    pub(crate) issuer_namespace: String,
    pub(crate) user_homes: Vec<Principal>,
    pub(crate) name: String,
    pub(crate) channel_hint: Option<String>,
}

/// An EVM transaction signed and not yet settled; the address signs no
/// other nonce until it settles or is released.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EvmPending {
    pub(crate) address: u32,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) proposal: u64,
}

/// The last blockhash or nonce value a Solana address signed with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SolLast {
    pub(crate) address: u32,
    pub(crate) blockhash: Hash,
}

/// An approved or executed spend in the rolling window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Spend {
    pub(crate) asset: AssetKey,
    pub(crate) amount: u128,
    pub(crate) at_ms: u64,
    pub(crate) proposal: u64,
}

/// The wallet record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Wallet {
    pub(crate) settings: Settings,
    pub(crate) created_at_ms: u64,
    pub(crate) members: Vec<Member>,
    pub(crate) policy: Policy,
    pub(crate) addresses: Vec<Address>,
    pub(crate) cosigners: Vec<Cosigner>,
    pub(crate) next_address: u32,
    pub(crate) next_cosigner: u32,
    pub(crate) next_proposal: u64,
    pub(crate) frozen: bool,
    pub(crate) code: CodeAcceptance,
    pub(crate) key_roots: KeyRoots,
    pub(crate) audit_head: Hash,
    pub(crate) day: u64,
    pub(crate) signatures_today: u32,
    pub(crate) refuels_today: u8,
    pub(crate) evm_pending: Vec<EvmPending>,
    pub(crate) sol_last: Vec<SolLast>,
    pub(crate) spends: Vec<Spend>,
    pub(crate) refuel: Option<crate::ledger::TopUp>,
}

impl Wallet {
    pub(crate) fn member(&self, id: &MemberId) -> Option<&Member> {
        self.members.iter().find(|m| m.id == *id)
    }

    /// The active member a Principal acts as.
    pub(crate) fn acting(&self, caller: Principal) -> Result<&Member> {
        authenticated(caller)?;
        self.members
            .iter()
            .find(|m| m.status == MemberStatus::Active && m.principals.contains(&caller))
            .ok_or(Error::Forbidden)
    }

    pub(crate) fn address(&self, id: u32) -> Result<&Address> {
        self.addresses
            .iter()
            .find(|a| a.id == id)
            .ok_or(Error::NotFound)
    }

    pub(crate) fn cosigner(&self, id: u32) -> Result<&Cosigner> {
        self.cosigners
            .iter()
            .find(|c| c.id == id)
            .ok_or(Error::NotFound)
    }

    /// Whether a native Taproot address's script commits to `cosigner`.
    pub(crate) fn taproot_cosigner(&self, cosigner: u32) -> bool {
        self.addresses
            .iter()
            .any(|a| matches!(&a.kind, AddressKind::Taproot(c) if c.cosigner == cosigner))
    }

    /// Whether `caller` may read the wallet.
    pub(crate) fn readable_by(&self, caller: Principal) -> bool {
        self.policy.public
            || caller == self.settings.hub
            || ic_cdk::api::is_controller(&caller)
            || self.members.iter().any(|m| m.principals.contains(&caller))
    }

    /// Reset the daily counters when the UTC day changes.
    pub(crate) fn roll_day(&mut self, now: u64) {
        let day = now / DAY;
        if day != self.day {
            self.day = day;
            self.signatures_today = 0;
            self.refuels_today = 0;
        }
    }

    /// The asset's spend in the 24 hours before `now`.
    pub(crate) fn window(&self, asset: &AssetKey, now: u64) -> u128 {
        self.spends
            .iter()
            .filter(|s| s.asset == *asset && s.at_ms + DAY > now)
            .fold(0, |sum, s| sum.saturating_add(s.amount))
    }

    pub(crate) fn view(&self) -> WalletView {
        WalletView {
            version: WALLET_VERSION,
            wallet: ic_cdk::api::canister_self(),
            hub: self.settings.hub,
            environment: self.settings.environment.clone(),
            name: self.settings.name.clone(),
            key_name: self.settings.key_name.clone(),
            btc_network: self.settings.btc_network,
            key_roots: self.key_roots.clone(),
            members: self.members.clone(),
            policy: self.policy.clone(),
            addresses: self.addresses.clone(),
            cosigners: self.cosigners.clone(),
            frozen: self.frozen,
            code: self.code.clone(),
            next_proposal: self.next_proposal,
            audit_len: AUDIT.with(|l| l.len()),
            audit_head: self.audit_head,
            channel_hint: self.settings.channel_hint.clone(),
            created_at_ms: self.created_at_ms,
        }
    }
}

/// One threshold signature a plan needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SignSpec {
    /// BIP-340; `tweak` is the BIP-341 merkle root for a key-path spend
    /// (empty for no scripts), absent for a script-path spend.
    Bip340 {
        path: Vec<serde_bytes::ByteBuf>,
        tweak: Option<serde_bytes::ByteBuf>,
    },
    /// ECDSA secp256k1 over a 32-byte hash.
    Ecdsa { path: Vec<serde_bytes::ByteBuf> },
    /// Ed25519 over the message.
    Ed25519 { path: Vec<serde_bytes::ByteBuf> },
}

/// A prepared plan and how to assemble its signed transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlanRecord {
    pub(crate) plan: ExecutionPlan,
    pub(crate) signs: Vec<SignSpec>,
    pub(crate) assembly: crate::chains::Assembly,
}

/// Ledger work of an ICP or ICRC proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Job {
    /// ICRC transfers, sent one by one.
    Ledger(crate::ledger::LedgerJob),
    /// ICP to the cycles minting canister, then a notification.
    TopUp(crate::ledger::TopUp),
    /// A canister call in flight.
    Call,
    /// Threshold signing of a plan in flight.
    Signing {
        plan: u32,
        signatures: Vec<serde_bytes::ByteBuf>,
    },
    /// A public key being read for a new address.
    Address,
}

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_WALLET: RefCell<StableCell<Stored<Option<Wallet>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), Stored(None)));
    static WALLET: RefCell<Option<Wallet>> =
        RefCell::new(STABLE_WALLET.with_borrow(|c| c.get().0.clone()));
    static PROPOSALS: RefCell<StableBTreeMap<u64, Stored<Proposal>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static OPS: RefCell<StableBTreeMap<[u8; 32], u64, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    static PLANS: RefCell<StableBTreeMap<[u8; 12], Stored<PlanRecord>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    static JOBS: RefCell<StableBTreeMap<u64, Stored<Job>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    pub(crate) static AUDIT: StableLog<Stored<AuditEvent>, Memory, Memory> =
        StableLog::init(memory(5), memory(6));
    static BOOK: RefCell<StableBTreeMap<Vec<u8>, Stored<BookEntry>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(7)));
    static CERT: RefCell<CertMap<Memory>> = RefCell::new(CertMap::new(memory(8), memory(9)));
}

pub(crate) fn with<R>(f: impl FnOnce(&Wallet) -> R) -> R {
    WALLET.with_borrow(|w| f(w.as_ref().expect("installed")))
}

pub(crate) fn wallet() -> Wallet {
    with(Clone::clone)
}

/// Persist the wallet and certify its view.
pub(crate) fn save(w: Wallet) {
    let view = canonical(&w.view());
    STABLE_WALLET.with_borrow_mut(|c| c.set(Stored(Some(w.clone()))));
    WALLET.with_borrow_mut(|cell| *cell = Some(w));
    CERT.with_borrow_mut(|c| c.insert(b"wallet".to_vec(), &view));
}

pub(crate) fn proposal(id: u64) -> Result<Proposal> {
    PROPOSALS
        .with_borrow(|t| t.get(&id))
        .map(|p| p.0)
        .ok_or(Error::NotFound)
}

pub(crate) fn proposal_key(id: u64) -> Vec<u8> {
    [&b"proposal/"[..], &id.to_be_bytes()].concat()
}

pub(crate) fn save_proposal(p: &Proposal) {
    let bytes = canonical(p);
    PROPOSALS.with_borrow_mut(|t| t.insert(p.id, Stored(p.clone())));
    CERT.with_borrow_mut(|c| c.insert(proposal_key(p.id), &bytes));
}

pub(crate) fn proposals_before(before: Option<u64>, limit: usize) -> Vec<Proposal> {
    PROPOSALS.with_borrow(|t| {
        let end = before.unwrap_or(u64::MAX);
        t.range(..end)
            .rev()
            .take(limit)
            .map(|e| e.value().0)
            .collect()
    })
}

pub(crate) fn oldest_proposal() -> Option<Proposal> {
    PROPOSALS.with_borrow(|t| t.first_key_value().map(|(_, p)| p.0))
}

pub(crate) fn proposal_count() -> u64 {
    PROPOSALS.with_borrow(|t| t.len())
}

pub(crate) fn remove_proposal(id: u64) {
    if let Some(p) = PROPOSALS.with_borrow_mut(|t| t.remove(&id)) {
        OPS.with_borrow_mut(|t| t.remove(&p.0.op_id.into_array()));
        CERT.with_borrow_mut(|c| c.remove(&proposal_key(id)));
        for seq in 1..=p.0.plans {
            PLANS.with_borrow_mut(|t| t.remove(&plan_key(id, seq)));
        }
    }
}

pub(crate) fn op(op_id: &Hash) -> Option<u64> {
    OPS.with_borrow(|t| t.get(&op_id.into_array()))
}

pub(crate) fn save_op(op_id: &Hash, id: u64) {
    OPS.with_borrow_mut(|t| t.insert(op_id.into_array(), id));
}

fn plan_key(id: u64, seq: u32) -> [u8; 12] {
    let mut key = [0; 12];
    key[..8].copy_from_slice(&id.to_be_bytes());
    key[8..].copy_from_slice(&seq.to_be_bytes());
    key
}

pub(crate) fn plan(id: u64, seq: u32) -> Result<PlanRecord> {
    PLANS
        .with_borrow(|t| t.get(&plan_key(id, seq)))
        .map(|p| p.0)
        .ok_or(Error::NotFound)
}

pub(crate) fn save_plan(record: &PlanRecord) {
    PLANS.with_borrow_mut(|t| {
        t.insert(
            plan_key(record.plan.proposal, record.plan.seq),
            Stored(record.clone()),
        )
    });
}

pub(crate) fn job(id: u64) -> Option<Job> {
    JOBS.with_borrow(|t| t.get(&id)).map(|j| j.0)
}

pub(crate) fn save_job(id: u64, job: &Job) {
    JOBS.with_borrow_mut(|t| t.insert(id, Stored(job.clone())));
}

pub(crate) fn remove_job(id: u64) {
    JOBS.with_borrow_mut(|t| t.remove(&id));
}

pub(crate) fn book_key(d: &Destination) -> Vec<u8> {
    canonical(d)
}

pub(crate) fn book_entry(d: &Destination) -> Option<BookEntry> {
    BOOK.with_borrow(|t| t.get(&book_key(d))).map(|e| e.0)
}

pub(crate) fn book_len() -> u64 {
    BOOK.with_borrow(|t| t.len())
}

pub(crate) fn book_page(after: Option<Destination>, limit: usize) -> Vec<BookEntry> {
    use std::ops::Bound::{Excluded, Unbounded};
    BOOK.with_borrow(|t| {
        let start = after.map_or(Unbounded, |d| Excluded(book_key(&d)));
        t.range((start, Unbounded))
            .take(limit)
            .map(|e| e.value().0)
            .collect()
    })
}

pub(crate) fn set_book(change: &BookChange, now: u64) {
    BOOK.with_borrow_mut(|t| match change {
        BookChange::Add { destination, label } => {
            let added_at_ms = t
                .get(&book_key(destination))
                .map_or(now, |e| e.0.added_at_ms);
            t.insert(
                book_key(destination),
                Stored(BookEntry {
                    destination: destination.clone(),
                    label: label.clone(),
                    added_at_ms,
                }),
            );
        }
        BookChange::Remove { destination } => {
            t.remove(&book_key(destination));
        }
    });
}

/// Append an audit event to the chain and certify the new head.
pub(crate) fn audit(
    w: &mut Wallet,
    at_ms: u64,
    actor: Option<MemberId>,
    proposal: Option<u64>,
    kind: AuditKind,
) {
    let seq = AUDIT.with(|l| l.len());
    let prev = w.audit_head;
    let hash = audit_hash(&prev, seq, at_ms, &actor, proposal, &kind);
    let event = AuditEvent {
        seq,
        at_ms,
        actor,
        proposal,
        kind,
        prev,
        hash,
    };
    AUDIT
        .with(|l| l.append(&Stored(event)))
        .expect("audit log memory");
    w.audit_head = hash;
}

pub(crate) fn audit_page(from: u64, limit: u64) -> Vec<AuditEvent> {
    AUDIT.with(|l| {
        (from..l.len().min(from.saturating_add(limit)))
            .filter_map(|i| l.get(i).map(|e| e.0))
            .collect()
    })
}

pub(crate) fn certified(
    keys: Vec<Vec<u8>>,
    value: impl FnMut(&[u8]) -> Option<Vec<u8>>,
) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), keys, value))
}

pub(crate) fn publish() {
    CERT.with_borrow(|c| c.publish());
}

pub(crate) fn stable_pages() -> u64 {
    ic_cdk::stable::stable_size()
}
