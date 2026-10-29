//! Stable state: the wallet record, proposals, plans, jobs, the address book,
//! the hash-chained audit log and the certified tree.
use candid::Principal;
use cbor2::Cbor;
use dmsg_protocol::{authenticated, canonical, wallet::*};
use dmsg_runtime::cert_map::CertMap;
use dmsg_runtime::storage::{CompactStored, Stored};
use dmsg_types::{wallet::*, *};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog,
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
};
use std::cell::RefCell;

// Stable layout: wallet=0, proposals=1, operation IDs=2, plans=3, jobs=4,
// audit index=5, audit data=6, address book=7, certified leaf hashes=8,
// certification nodes=9.
type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) const STABLE_SCHEMA: u16 = 2;

/// Job key of the policy's automatic top-up; proposal IDs start at 1.
pub(crate) const REFUEL_JOB: u64 = 0;

/// Deployment settings fixed at install, except appended user homes.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct Settings {
    #[cbor(key = 1)]
    pub(crate) environment: Environment,
    #[cbor(key = 2)]
    pub(crate) hub: Principal,
    #[cbor(key = 3)]
    pub(crate) key_name: String,
    #[cbor(key = 4)]
    pub(crate) btc_network: BtcNetwork,
    #[cbor(key = 5)]
    pub(crate) icp_ledger: Principal,
    #[cbor(key = 6)]
    pub(crate) cmc: Principal,
    #[cbor(key = 7)]
    pub(crate) issuer_namespace: String,
    #[cbor(key = 8)]
    pub(crate) user_homes: Vec<Principal>,
    #[cbor(key = 9)]
    pub(crate) name: String,
    #[cbor(key = 10)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) channel_hint: Option<String>,
}

/// An EVM transaction signed and not yet settled; the address signs no
/// other nonce until it settles or is released.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct EvmPending {
    #[cbor(key = 1)]
    pub(crate) address: u32,
    #[cbor(key = 2)]
    pub(crate) chain_id: u64,
    #[cbor(key = 3)]
    pub(crate) nonce: u64,
    #[cbor(key = 4)]
    pub(crate) proposal: u64,
}

/// The last blockhash or nonce value a Solana address signed with.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct SolLast {
    #[cbor(key = 1)]
    pub(crate) address: u32,
    #[cbor(key = 2)]
    pub(crate) blockhash: Hash,
}

/// An approved or executed spend in the rolling window.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct Spend {
    #[cbor(key = 1)]
    pub(crate) asset: AssetKey,
    #[cbor(key = 2)]
    pub(crate) amount: u128,
    #[cbor(key = 3)]
    pub(crate) at_ms: u64,
    #[cbor(key = 4)]
    pub(crate) proposal: u64,
    /// None reserves an approved payment until its first execution.
    #[cbor(key = 5)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) executed_at_ms: Option<u64>,
}

/// The wallet record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wallet {
    pub(crate) schema: u16,
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
    pub(crate) code_busy: bool,
    pub(crate) key_roots: KeyRoots,
    pub(crate) audit_head: Hash,
    pub(crate) day: u64,
    pub(crate) signatures_today: u32,
    pub(crate) refuels_today: u8,
    pub(crate) evm_pending: Vec<EvmPending>,
    pub(crate) sol_last: Vec<SolLast>,
    pub(crate) spends: Vec<Spend>,
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
            .find(|m| m.status == MemberStatus::Active && m.binds(&caller))
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
            || self.members.iter().any(|m| m.binds(&caller))
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

    /// Unexecuted reservations plus spends begun in the 24 hours before `now`.
    pub(crate) fn window(&self, asset: &AssetKey, now: u64) -> u128 {
        self.spends
            .iter()
            .filter(|s| s.asset == *asset && self.retains_spend(s, now))
            .fold(0, |sum, s| sum.saturating_add(s.amount))
    }

    pub(crate) fn retains_spend(&self, spend: &Spend, now: u64) -> bool {
        if let Some(at) = spend.executed_at_ms {
            return at.saturating_add(DAY) > now;
        }
        // Deadlines and authorization changes release unexecuted reservations
        // even if nobody has refreshed the proposal yet.
        proposal(spend.proposal).is_ok_and(|p| {
            if is_waiting(p.status) {
                p.policy_version == self.policy.version && now < p.expires_at_ms
            } else {
                !matches!(
                    p.status,
                    ProposalStatus::Cancelled
                        | ProposalStatus::Rejected
                        | ProposalStatus::Vetoed
                        | ProposalStatus::Expired
                        | ProposalStatus::Stale
                )
            }
        })
    }

    pub(crate) fn commit_spend(&mut self, proposal: u64, at: u64) {
        if let Some(s) = self.spends.iter_mut().find(|s| s.proposal == proposal) {
            s.executed_at_ms.get_or_insert(at);
        }
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

/// The threshold signature every digest of a plan needs: a plan spends from
/// one address, so one key and one tweak sign all of its digests.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) enum SignSpec {
    /// BIP-340; `tweak` is the BIP-341 merkle root for a key-path spend
    /// (empty for no scripts), absent for a script-path spend.
    Bip340 {
        #[cbor(key = 1)]
        path: Vec<serde_bytes::ByteBuf>,
        #[cbor(key = 2)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tweak: Option<serde_bytes::ByteBuf>,
    },
    /// ECDSA secp256k1 over a 32-byte hash.
    Ecdsa {
        #[cbor(key = 1)]
        path: Vec<serde_bytes::ByteBuf>,
    },
    /// Ed25519 over the whole unsigned message.
    Ed25519 {
        #[cbor(key = 1)]
        path: Vec<serde_bytes::ByteBuf>,
    },
}

/// A prepared plan and how to assemble its signed transaction.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct PlanRecord {
    #[cbor(key = 1)]
    pub(crate) plan: ExecutionPlan,
    #[cbor(key = 2)]
    pub(crate) sign: SignSpec,
    #[cbor(key = 3)]
    pub(crate) assembly: crate::chains::Assembly,
    /// Snapshot of the nonce account used to build a Solana message. A plan
    /// cannot change from a recent blockhash to a durable nonce after signing.
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sol_nonce: Option<Hash>,
}

/// Work of a proposal in flight. Ledger work keeps its exact arguments so a
/// retry is deduplicated; the others only mark the proposal busy.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) enum Job {
    /// ICRC transfers, sent one by one.
    Ledger(crate::ledger::LedgerJob),
    /// ICP to the cycles minting canister, then a notification.
    TopUp(crate::ledger::TopUp),
    /// A canister call in flight.
    Call,
    /// Threshold signing of a plan in flight.
    Signing,
    /// A public key being read for a new address.
    Address,
}

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_WALLET: RefCell<StableCell<CompactStored<Option<Wallet>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    // Every message reads the record, so it stays decoded on the heap.
    static WALLET: RefCell<Option<Wallet>> =
        RefCell::new(STABLE_WALLET.with_borrow(|c| c.get().value()));
    static PROPOSALS: RefCell<StableBTreeMap<u64, CompactStored<Proposal>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static OPS: RefCell<StableBTreeMap<[u8; 32], u64, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    static PLANS: RefCell<StableBTreeMap<[u8; 12], Stored<PlanRecord>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    static JOBS: RefCell<StableBTreeMap<u64, Stored<Job>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    static AUDIT: StableLog<CompactStored<AuditEvent>, Memory, Memory> =
        StableLog::init(memory(5), memory(6));
    static BOOK: RefCell<StableBTreeMap<Vec<u8>, CompactStored<BookEntry>, Memory>> =
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
    STABLE_WALLET.with_borrow_mut(|c| c.set(CompactStored::some(&w)));
    CERT.with_borrow_mut(|c| c.insert(b"wallet".to_vec(), &view));
    WALLET.with_borrow_mut(|cell| *cell = Some(w));
}

pub(crate) fn proposal(id: u64) -> Result<Proposal> {
    PROPOSALS
        .with_borrow(|t| t.get(&id))
        .map(CompactStored::into_inner)
        .ok_or(Error::NotFound)
}

pub(crate) fn proposal_key(id: u64) -> Vec<u8> {
    [&b"proposal/"[..], &id.to_be_bytes()].concat()
}

pub(crate) fn save_proposal(p: &Proposal) {
    let bytes = canonical(p);
    PROPOSALS.with_borrow_mut(|t| t.insert(p.id, CompactStored::new(p)));
    CERT.with_borrow_mut(|c| c.insert(proposal_key(p.id), &bytes));
}

pub(crate) fn proposals_before(before: Option<u64>, limit: usize) -> Vec<Proposal> {
    PROPOSALS.with_borrow(|t| {
        let end = before.unwrap_or(u64::MAX);
        t.range(..end)
            .rev()
            .take(limit)
            .map(|e| e.value().into_inner())
            .collect()
    })
}

/// IDs of the `limit` oldest proposals.
pub(crate) fn oldest_ids(limit: usize) -> Vec<u64> {
    PROPOSALS.with_borrow(|t| t.keys().take(limit).collect())
}

pub(crate) fn proposal_count() -> u64 {
    PROPOSALS.with_borrow(|t| t.len())
}

pub(crate) fn remove_proposal(id: u64) {
    if let Some(p) = PROPOSALS.with_borrow_mut(|t| t.remove(&id)) {
        let p = p.into_inner();
        OPS.with_borrow_mut(|t| t.remove(&p.op_id.into_array()));
        CERT.with_borrow_mut(|c| c.remove(&proposal_key(id)));
        PLANS.with_borrow_mut(|t| {
            for seq in 1..=p.plans {
                t.remove(&plan_key(id, seq));
            }
        });
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
    BOOK.with_borrow(|t| t.get(&book_key(d)))
        .map(CompactStored::into_inner)
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
            .map(|e| e.value().into_inner())
            .collect()
    })
}

pub(crate) fn set_book(change: &BookChange, now: u64) {
    BOOK.with_borrow_mut(|t| match change {
        BookChange::Add { destination, label } => {
            let key = book_key(destination);
            let added_at_ms = t.get(&key).map_or(now, |e| e.into_inner().added_at_ms);
            t.insert(
                key,
                CompactStored::new(&BookEntry {
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
        .with(|l| l.append(&CompactStored::new(&event)))
        .expect("audit log memory");
    w.audit_head = hash;
}

pub(crate) fn audit_page(from: u64, limit: u64) -> Vec<AuditEvent> {
    AUDIT.with(|l| {
        (from..l.len().min(from.saturating_add(limit)))
            .filter_map(|i| l.get(i).map(CompactStored::into_inner))
            .collect()
    })
}

pub(crate) fn certified(
    me: Principal,
    keys: Vec<Vec<u8>>,
    value: impl FnMut(&[u8]) -> Option<Vec<u8>>,
) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(me, keys, value))
}

pub(crate) fn stable_pages() -> u64 {
    ic_cdk::stable::stable_size()
}
