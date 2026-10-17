//! `dmsg-wallet/1`: team wallets, one canister per team.
//!
//! A wallet holds ICP and ICRC assets as its own ledger accounts and signs
//! Bitcoin, EVM and Solana transactions with chain-key threshold signatures.
//! Members propose intents, approve them under the wallet's policy, and
//! broadcast signed transactions from their clients; the canister never makes
//! HTTPS outcalls. A dMsg account member approves with a device approval
//! (`ApplicationApproval`) that its user home verifies; a Principal member
//! calls in person.
//!
//! Times are Unix milliseconds; amounts are integer base units of the asset
//! (e8s, satoshis, wei, lamports, token atomic units). Constructing or
//! decoding these records performs no validation or authorization.
use crate::{
    AccountId, Environment, Hash, OpId, billing::PlanId, integration::ApplicationApproval,
};
use candid::{CandidType, Principal};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Wire version of the wallet records.
pub const WALLET_VERSION: u16 = 1;
/// Registered application of wallet approvals.
pub const WALLET_APP_ID: &str = "dmsg-wallet";
/// Registered product of wallet approvals.
pub const WALLET_PRODUCT_ID: &str = "dmsg-wallet";
/// Subject schema of wallet approvals: the 10 bytes of a canister Principal,
/// the wallet's for its decisions and the hub's for creating one.
pub const WALLET_SUBJECT_SCHEMA: &str = "dmsg-wallet-v1";
/// Maximum members of a wallet.
pub const MAX_MEMBERS: usize = 64;
/// Maximum addresses of a wallet.
pub const MAX_ADDRESSES: usize = 256;
/// Maximum cosigners of a wallet.
pub const MAX_COSIGNERS: usize = 64;
/// Maximum address-book entries of a wallet.
pub const MAX_BOOK_ENTRIES: usize = 4_096;
/// Maximum execution plans of one proposal.
pub const MAX_PLANS: u32 = 8;
/// Maximum outputs of a batch transfer.
pub const MAX_BATCH_ITEMS: usize = 32;
/// Maximum Bitcoin inputs of one plan; each needs its own signature.
pub const MAX_BTC_INPUTS: usize = 32;
/// Maximum principals bound to one member.
pub const MAX_MEMBER_PRINCIPALS: usize = 4;
/// Maximum Agent IDs allowed to propose for one member.
pub const MAX_MEMBER_AGENTS: usize = 8;
/// Lifetime of an execution plan.
pub const PLAN_TTL_MS: u64 = 10 * crate::MINUTE;
/// Delay of a Recovery member's proposal.
pub const RECOVERY_DELAY_MS: u64 = 7 * crate::DAY;
/// Delay of policy, cosigner, member removal, unfreeze and code proposals.
pub const SENSITIVE_DELAY_MS: u64 = crate::DAY;
/// Lifetime of an activation quote.
pub const QUOTE_TTL_MS: u64 = 15 * crate::MINUTE;
/// Maximum bytes of a canister call argument or a retained reply.
pub const MAX_CALL_BYTES: usize = 65_536;

/// 20-byte EVM address.
pub type Hash20 = serde_bytes::ByteArray<20>;

/// A wallet member, one of the team: a dMsg account or a Principal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemberId {
    /// A dMsg account; it approves with device approvals its user home verifies.
    Account(AccountId),
    /// A Principal calling in person.
    Principal(Principal),
}

/// What a member may do. Roles are ordered: each includes the ones after it
/// except Recovery, which only proposes membership changes.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Approves admin proposals (policy, members, cosigners, code) and everything below.
    Admin,
    /// Approves and vetoes spending proposals, freezes the wallet.
    Approver,
    /// Proposes and executes; its approvals do not count.
    Proposer,
    /// Reads the wallet.
    Viewer,
    /// Proposes only membership changes, with a seven-day delay; its approvals never count.
    Recovery,
}

/// Whether a member confirmed joining.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberStatus {
    /// Added by a proposal; acts only after `join`.
    Invited,
    /// Joined.
    Active,
}

/// A member as proposed.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MemberInput {
    /// Member identity.
    pub id: MemberId,
    /// Role.
    pub role: Role,
    /// Display label, at most 64 bytes.
    pub label: String,
    /// Agent IDs allowed to propose on this member's behalf.
    pub agent_ids: Vec<String>,
}

/// A member of the wallet.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Member identity.
    pub id: MemberId,
    /// Role.
    pub role: Role,
    /// Display label.
    pub label: String,
    /// Agent IDs allowed to propose on this member's behalf.
    pub agent_ids: Vec<String>,
    /// Whether the member joined.
    pub status: MemberStatus,
    /// Principals that act as this member: the Principal itself, or the
    /// login Principals an account bound when joining.
    pub principals: Vec<Principal>,
    /// Time the member was added.
    pub added_at_ms: u64,
}

/// Asset identity; amounts and limits are in its native base units.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetKey {
    /// ICP on the ICP ledger.
    Icp,
    /// An ICRC-1 ledger, such as ckBTC, ckUSDC or PANDA.
    Icrc(Principal),
    /// Bitcoin.
    Btc,
    /// The native coin (`token = None`) or an ERC-20 token of an EVM chain.
    Evm {
        /// EIP-155 chain ID.
        chain_id: u64,
        /// ERC-20 contract.
        token: Option<Hash20>,
    },
    /// SOL (`mint = None`) or an SPL token.
    Sol {
        /// SPL token mint.
        mint: Option<Hash>,
    },
}

/// Chain an address lives on.
#[derive(
    CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Chain {
    /// ICP and ICRC ledgers: the wallet's own account and a subaccount.
    Icp,
    /// Bitcoin.
    Bitcoin,
    /// Every EVM chain; one key serves them all.
    Evm,
    /// Solana.
    Solana,
}

/// Bitcoin network of a deployment.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BtcNetwork {
    /// `bc` addresses.
    Mainnet,
    /// `tb` addresses (testnet4 and signet).
    Testnet,
    /// `bcrt` addresses.
    Regtest,
}

/// How an address is controlled.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AddressKind {
    /// The wallet's chain key alone: an ICP subaccount, a Bitcoin P2TR key
    /// path, an EVM account or a Solana account. A cosign policy makes the
    /// canister require cosigner signatures before it signs.
    ChainKey,
    /// A Bitcoin P2WPKH output signed with threshold ECDSA.
    Segwit,
    /// A Bitcoin Taproot output with an unspendable key path: the first leaf
    /// needs the chain key and the cosigner, the second the chain key alone
    /// once an output is `chain_key_after` blocks old, and an optional third
    /// a recovery key after its own delay.
    Taproot(TaprootControl),
}

/// Script tree of a native two-key Taproot address.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TaprootControl {
    /// Cosigner whose x-only secp256k1 key the first leaf requires.
    pub cosigner: u32,
    /// Relative lock, in blocks, after which the chain key spends alone.
    pub chain_key_after: u16,
    /// Optional recovery key and its relative lock.
    pub recovery: Option<TaprootRecovery>,
}

/// Recovery leaf of a native Taproot address.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TaprootRecovery {
    /// x-only secp256k1 public key.
    pub key: Hash,
    /// Relative lock in blocks.
    pub after: u16,
}

/// Cosigners an address requires before the canister signs.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CosignPolicy {
    /// Cosigner IDs.
    pub cosigners: Vec<u32>,
    /// Signatures required, at least one.
    pub threshold: u8,
}

/// A wallet address.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Address {
    /// Wallet-unique ID; also the derivation index and, on ICP, the subaccount.
    pub id: u32,
    /// Chain.
    pub chain: Chain,
    /// Control.
    pub kind: AddressKind,
    /// Chain-key public key at the address's derivation path: SEC1 compressed
    /// secp256k1 (Bitcoin, EVM) or 32-byte Ed25519 (Solana); empty for ICP.
    pub public_key: ByteBuf,
    /// Display form: ICRC account text, bech32 or bech32m, EIP-55 hex or base58.
    pub text: String,
    /// Display label.
    pub label: String,
    /// Cosigners required before signing.
    pub cosign: Option<CosignPolicy>,
    /// Solana durable nonce account, once created.
    pub sol_nonce: Option<Hash>,
    /// Creation time.
    pub created_at_ms: u64,
}

/// Curve of a cosigner key.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CosignerKind {
    /// NIST P-256 ECDSA: Secure Enclave, TPM, FIDO2 and passkeys.
    P256,
    /// Ed25519.
    Ed25519,
    /// secp256k1 ECDSA.
    Secp256k1,
    /// BIP-340 Schnorr over secp256k1 with an x-only key; native Taproot
    /// cosigners sign the transaction's sighash with it.
    Bip340,
}

/// How a cosigner wraps the message it signs.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CosignerEnvelope {
    /// The signature covers the cosign message itself (P-256 and secp256k1
    /// sign its SHA-256, as ECDSA over SHA-256 does).
    Raw,
    /// A CTAP2 assertion whose client data hash is the cosign message.
    Fido2 {
        /// SHA-256 of the relying-party ID.
        rp_id_hash: Hash,
        /// Whether user verification (PIN or biometrics) is required.
        user_verification: bool,
    },
    /// A WebAuthn assertion whose challenge is the cosign message.
    WebAuthn {
        /// SHA-256 of the relying-party ID.
        rp_id_hash: Hash,
        /// Exact origins accepted in the client data.
        origins: Vec<String>,
        /// Whether user verification is required.
        user_verification: bool,
    },
}

/// A cosigner as proposed.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CosignerInput {
    /// Member owning the key; removing the member removes the cosigner.
    pub member: MemberId,
    /// Curve.
    pub kind: CosignerKind,
    /// SEC1 compressed (P-256, secp256k1), x-only (BIP-340) or 32-byte
    /// Ed25519 public key.
    pub public_key: ByteBuf,
    /// Message wrapping.
    pub envelope: CosignerEnvelope,
    /// Whether the client declared a non-exportable hardware key. The canister
    /// cannot check it; a policy may refuse software keys.
    pub hardware: bool,
    /// Display label.
    pub label: String,
}

/// A registered cosigner.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Cosigner {
    /// Wallet-unique ID.
    pub id: u32,
    /// The proposed key and envelope.
    pub input: CosignerInput,
    /// Highest authenticator signature counter seen (FIDO2 and WebAuthn).
    pub counter: u32,
    /// Registration time.
    pub added_at_ms: u64,
}

/// One cosigner signature over one cosign message.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CosignerSig {
    /// Cosigner ID.
    pub cosigner: u32,
    /// Index of the digest in the plan.
    pub digest_index: u32,
    /// 64-byte signature: ECDSA `r || s`, BIP-340 or Ed25519; clients convert
    /// DER signatures of Secure Enclave and TPM keys.
    pub signature: ByteBuf,
    /// Authenticator data (FIDO2 and WebAuthn).
    pub authenticator_data: Option<ByteBuf>,
    /// Client data JSON (WebAuthn).
    pub client_data_json: Option<ByteBuf>,
}

/// Approval requirement of one asset amount band.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Tier {
    /// Asset.
    pub asset: AssetKey,
    /// Inclusive upper bound of the rolling 24-hour amount, proposal included.
    pub up_to: u128,
    /// Approver approvals required.
    pub approvals: u8,
    /// Delay after the last approval during which an Approver may veto.
    pub delay_ms: u64,
    /// Whether destinations must be address-book entries past their cooldown.
    pub book_only: bool,
}

/// Daily ceiling of an asset; amounts above it need every Approver.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AssetLimit {
    /// Asset.
    pub asset: AssetKey,
    /// Rolling 24-hour amount.
    pub amount: u128,
}

/// Cycles top-up from the wallet's own ICP.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RefuelRule {
    /// ICP address paying the top-up.
    pub from: u32,
    /// Top up when the cycle balance is below this.
    pub min_cycles: u128,
    /// ICP sent to the cycles minting canister per top-up.
    pub icp_e8s: u64,
    /// Top-ups per UTC day.
    pub max_per_day: u8,
}

/// The wallet's rules. Every change increases `version`, and open proposals
/// made under an older version can no longer pass.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    /// Monotonic version; a proposal fixes the version it was made under.
    pub version: u64,
    /// Default Approver approvals for spending without a matching tier.
    pub approver_threshold: u8,
    /// Admin approvals for admin proposals.
    pub admin_threshold: u8,
    /// Amount bands per asset, ascending by `up_to`.
    pub tiers: Vec<Tier>,
    /// Time a new address-book entry waits before `book_only` tiers accept it.
    pub address_book_cooldown_ms: u64,
    /// Daily ceilings.
    pub daily_limits: Vec<AssetLimit>,
    /// Assets the wallet may send; empty means receive only.
    pub spendable_assets: Vec<AssetKey>,
    /// Canisters a canister-call proposal may target.
    pub call_targets: Vec<Principal>,
    /// EVM chain IDs the wallet may sign for.
    pub evm_chains: Vec<u64>,
    /// Lifetime of an unexecuted proposal.
    pub proposal_ttl_ms: u64,
    /// Delay of an Unfreeze proposal.
    pub unfreeze_delay_ms: u64,
    /// Threshold signatures per UTC day.
    pub daily_signatures: u32,
    /// Automatic cycles top-up.
    pub refuel: Option<RefuelRule>,
    /// Whether cosigners must be declared hardware keys.
    pub hardware_cosigners: bool,
    /// Whether anyone may read the wallet, not only members.
    pub public: bool,
}

/// Recipient of a transfer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Destination {
    /// An ICRC-1 account (ICP included).
    Icrc(#[serde(with = "crate::account::account_cbor")] Account),
    /// A Bitcoin address of the deployment's network.
    Bitcoin(String),
    /// An EVM address.
    Evm(Hash20),
    /// A Solana account (for SPL tokens, the owner of the receiving token account).
    Solana(Hash),
}

/// One output of a batch transfer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BatchItem {
    /// Recipient.
    pub to: Destination,
    /// Amount.
    pub amount: u128,
}

/// Address-book change.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum BookChange {
    /// Add or relabel an entry; a new entry starts its cooldown.
    Add {
        /// Recipient.
        destination: Destination,
        /// Display label.
        label: String,
    },
    /// Remove an entry.
    Remove {
        /// Recipient.
        destination: Destination,
    },
}

/// Address-book entry.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BookEntry {
    /// Recipient.
    pub destination: Destination,
    /// Display label.
    pub label: String,
    /// Time the entry was added; its cooldown starts here.
    pub added_at_ms: u64,
}

/// ICRC-21 consent text the target canister gave for a call.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConsentRecord {
    /// Language of the text.
    pub language: String,
    /// The generic display message.
    pub message: String,
}

/// What a proposal asks the wallet to do.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// Send one amount.
    Transfer {
        /// Source address.
        from: u32,
        /// Asset.
        asset: AssetKey,
        /// Recipient.
        to: Destination,
        /// Amount.
        amount: u128,
        /// Highest network fee, in the chain's native base units.
        max_fee: u128,
        /// ICRC memo, at most 32 bytes.
        memo: Option<ByteBuf>,
    },
    /// Send several amounts of one asset: ICRC transfers one by one, one
    /// Bitcoin or Solana transaction. Not for EVM.
    Batch {
        /// Source address.
        from: u32,
        /// Asset.
        asset: AssetKey,
        /// Outputs.
        items: Vec<BatchItem>,
        /// Highest network fee of the whole batch.
        max_fee: u128,
    },
    /// Call a canister in the policy's `call_targets`.
    CanisterCall {
        /// Target.
        canister: Principal,
        /// Method.
        method: String,
        /// Candid argument bytes.
        arg: ByteBuf,
        /// Cycles attached.
        cycles: u128,
        /// ICRC-21 consent text, filled by the wallet when the target offers one.
        consent: Option<ConsentRecord>,
    },
    /// Create an address.
    AddAddress {
        /// Chain.
        chain: Chain,
        /// Control.
        kind: AddressKind,
        /// Display label.
        label: String,
        /// Cosigners required.
        cosign: Option<CosignPolicy>,
    },
    /// Replace an address's cosign policy.
    SetCosign {
        /// Address.
        address: u32,
        /// New policy, or none.
        cosign: Option<CosignPolicy>,
    },
    /// Register a cosigner.
    AddCosigner(CosignerInput),
    /// Remove a cosigner and every cosign policy naming it.
    RemoveCosigner(u32),
    /// Invite a member.
    AddMember(MemberInput),
    /// Remove a member, its pending approvals and its cosigners, except one
    /// a native Taproot address commits to, which stays for that address.
    RemoveMember(MemberId),
    /// Change a member's role, label or agents.
    UpdateMember(MemberInput),
    /// Replace the policy; `version` must be the current one plus one.
    SetPolicy(Policy),
    /// Change the address book.
    AddressBook(BookChange),
    /// Resume executions; any Approver freezes the wallet directly.
    Unfreeze,
    /// Top up cycles with ICP from an ICP address.
    Refuel {
        /// Source ICP address.
        from: u32,
        /// ICP sent to the cycles minting canister.
        icp_e8s: u64,
    },
    /// Let the hub install a registered version.
    AcceptUpgrade {
        /// Hub version number.
        version: u32,
        /// SHA-256 of the module.
        module_hash: Hash,
    },
    /// Let the hub remove every controller; the code can never change again.
    LockCode,
    /// Replace a signed but unsettled EVM transaction with a zero-value
    /// self-transfer at the same nonce.
    CancelEvmNonce {
        /// EVM address.
        address: u32,
        /// Chain.
        chain_id: u64,
        /// Highest network fee in wei.
        max_fee: u128,
    },
    /// Forget an EVM address's unsettled transaction without replacing it.
    ReleaseEvmNonce {
        /// EVM address.
        address: u32,
        /// Chain.
        chain_id: u64,
    },
    /// Create the Solana durable nonce account of an address.
    InitSolNonce {
        /// Solana address.
        address: u32,
        /// Highest network fee in lamports, besides the rent deposit.
        max_fee: u128,
    },
}

/// Lifecycle of a proposal.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Collecting approvals.
    Open,
    /// Approved; an Approver may veto until `ready_at_ms`.
    Delayed,
    /// Approved and past its delay.
    Ready,
    /// A ledger call, a canister call or threshold signing is in flight or
    /// its outcome is unknown; reconcile or execute again.
    Executing,
    /// A transaction is signed and waits for broadcast.
    Signed,
    /// A member reported broadcasting a signed transaction.
    Broadcast,
    /// Done.
    Settled,
    /// The ledger, target or chain refused it.
    Failed,
    /// Approvers rejected it.
    Rejected,
    /// An Approver vetoed it during its delay.
    Vetoed,
    /// Its proposer cancelled it.
    Cancelled,
    /// Its lifetime ended before execution.
    Expired,
    /// The policy changed before execution.
    Stale,
}

/// Approvals and delay a proposal needs, set when it is made. A spend's is
/// raised at an approval when its rolling window then asks for more.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// Approvals required.
    pub approvals: u8,
    /// Whether only Admin approvals count.
    pub admin: bool,
    /// Delay after the last approval.
    pub delay_ms: u64,
    /// The rolling 24-hour amount the tier was chosen for, proposal included.
    pub window_amount: u128,
}

/// A member's decision.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Approve.
    Approve,
    /// Reject.
    Reject,
    /// Veto during the delay.
    Veto,
}

/// One recorded decision.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    /// Member.
    pub member: MemberId,
    /// Decision.
    pub decision: Decision,
    /// Time.
    pub at_ms: u64,
}

/// Evidence that a member made a decision.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum MemberApproval {
    /// A dMsg account member's device approval, verified by its user home.
    Account {
        /// The device approval's request ID.
        approval_id: Hash,
        /// The approved terms.
        approval: ApplicationApproval,
    },
    /// The calling Principal is the member.
    Principal,
}

/// Arguments of a new proposal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProposeArgs {
    /// What to do.
    pub intent: Intent,
    /// Proposer-chosen identity; repeating it returns the same proposal.
    pub op_id: OpId,
    /// Agent ID proposing on the member's behalf.
    pub via_agent: Option<String>,
}

/// Outcome of an executed proposal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Internal change applied.
    Applied,
    /// ICRC transfers, one block per item.
    Ledger {
        /// Blocks in item order.
        blocks: Vec<u64>,
    },
    /// Canister call reply, at most 64 KiB, with its SHA-256.
    Replied {
        /// Reply bytes.
        reply: ByteBuf,
        /// SHA-256 of the full reply.
        reply_hash: Hash,
    },
    /// The target rejected the call.
    CallRejected {
        /// Reject code.
        code: u32,
        /// Reject message.
        message: String,
    },
    /// The call's outcome is unknown; check the target before proposing again.
    CallUnknown,
    /// Address created.
    AddressCreated(u32),
    /// Cosigner registered.
    CosignerAdded(u32),
    /// A member reported the transaction final.
    ChainSettled {
        /// Settled plan.
        plan: u32,
        /// Transaction ID.
        txid: String,
    },
    /// Cycles bought with ICP.
    Refueled {
        /// ICP ledger block.
        block: u64,
        /// Cycles minted.
        cycles: u128,
    },
}

/// A proposal.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    /// Wallet-local ID.
    pub id: u64,
    /// Proposer-chosen identity.
    pub op_id: OpId,
    /// What to do.
    pub intent: Intent,
    /// Policy version it was made under.
    pub policy_version: u64,
    /// Proposer.
    pub proposer: MemberId,
    /// Agent ID proposing on the proposer's behalf.
    pub via_agent: Option<String>,
    /// Commitment members approve.
    pub digest: Hash,
    /// Creation time.
    pub created_at_ms: u64,
    /// Execution deadline.
    pub expires_at_ms: u64,
    /// Approvals and delay required.
    pub requirement: Requirement,
    /// Status.
    pub status: ProposalStatus,
    /// Decisions in order.
    pub votes: Vec<Vote>,
    /// End of the veto delay.
    pub ready_at_ms: Option<u64>,
    /// Execution plans prepared so far.
    pub plans: u32,
    /// Signed transactions in plan order.
    pub signed: Vec<SignedTx>,
    /// Broadcast reports.
    pub broadcasts: Vec<BroadcastReport>,
    /// Outcome.
    pub outcome: Option<Outcome>,
    /// Failure reason.
    pub failure: Option<String>,
    /// Time of the last status change.
    pub updated_at_ms: u64,
}

/// One unspent Bitcoin output to spend.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Utxo {
    /// Transaction ID in internal byte order (the reverse of the displayed hex).
    pub txid: Hash,
    /// Output index.
    pub vout: u32,
    /// Value in satoshis. A wrong value makes the signature invalid on chain.
    pub value: u64,
    /// Wallet address that owns the output.
    pub address: u32,
}

/// Chain parameters the executor read and the plan uses.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExecutionParams {
    /// Bitcoin.
    Bitcoin {
        /// Inputs; change returns to the source address.
        utxos: Vec<Utxo>,
        /// Fee rate in satoshis per virtual byte.
        fee_rate: u64,
        /// Transaction lock time.
        locktime: u32,
        /// Spend a native Taproot address with the chain key alone, through
        /// its timelocked leaf; the inputs must be old enough.
        timelock: bool,
    },
    /// EVM, EIP-1559.
    Evm {
        /// Account nonce.
        nonce: u64,
        /// Highest fee per gas in wei.
        max_fee_per_gas: u128,
        /// Highest priority fee per gas in wei.
        max_priority_fee_per_gas: u128,
        /// Gas limit.
        gas_limit: u64,
    },
    /// Solana.
    Solana {
        /// Recent blockhash, or the durable nonce value when the address has a nonce account.
        blockhash: Hash,
        /// Compute unit price in micro-lamports.
        compute_unit_price: u64,
        /// Compute unit limit.
        compute_unit_limit: u32,
        /// Token decimals of an SPL transfer.
        decimals: Option<u8>,
        /// Whether to create the recipient's associated token account first.
        create_ata: bool,
    },
}

/// One output of a decoded transaction.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SummaryOutput {
    /// Recipient in display form.
    pub to: String,
    /// Amount in base units.
    pub amount: u128,
}

/// What an unsigned transaction does, as its encoder built it. A client
/// decodes the unsigned bytes on its own and refuses a mismatch.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DecodedSummary {
    /// `bitcoin:<network>`, `evm:<chain_id>` or `solana`.
    pub chain: String,
    /// Source address text.
    pub from: String,
    /// `BTC`, `ETH`, `ERC20:<0x contract>`, `SOL` or `SPL:<mint>`.
    pub asset: String,
    /// Payments.
    pub outputs: Vec<SummaryOutput>,
    /// Change back to the source.
    pub change: Option<SummaryOutput>,
    /// Exact fee (Bitcoin), the fee ceiling (EVM) or the signature and
    /// priority fee (Solana), in native base units.
    pub fee: u128,
    /// EVM nonce, Solana blockhash or nonce (base58), or empty.
    pub nonce: String,
}

/// An unsigned transaction ready for threshold signing.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlan {
    /// Proposal.
    pub proposal: u64,
    /// Plan number within the proposal, from 1.
    pub seq: u32,
    /// Parameters.
    pub params: ExecutionParams,
    /// Unsigned transaction: a Bitcoin transaction without witnesses, an
    /// EIP-1559 payload `0x02 || rlp(...)`, or a Solana message.
    pub unsigned: ByteBuf,
    /// Messages to sign, in signing order.
    pub digests: Vec<Hash>,
    /// Decoded effect.
    pub summary: DecodedSummary,
    /// Creation time.
    pub created_at_ms: u64,
    /// Deadline for `execute`.
    pub expires_at_ms: u64,
}

/// A signed transaction.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedTx {
    /// Plan.
    pub plan: u32,
    /// Raw transaction to broadcast.
    pub raw: ByteBuf,
    /// Transaction ID: displayed Bitcoin hex, EVM 0x hash or Solana base58 signature.
    pub txid: String,
    /// Signing time.
    pub signed_at_ms: u64,
}

/// A member's report of broadcasting or settling a transaction.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BroadcastReport {
    /// Plan.
    pub plan: u32,
    /// What happened.
    pub outcome: ChainOutcome,
    /// Transaction ID.
    pub txid: String,
    /// Reporting member.
    pub member: MemberId,
    /// Time.
    pub at_ms: u64,
    /// Free text, at most 256 bytes.
    pub note: String,
}

/// Arguments of `execute`.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecuteArgs {
    /// Proposal.
    pub proposal: u64,
    /// Plan to sign, for external chains.
    pub plan: Option<u32>,
    /// Cosigner signatures the source address's cosign policy requires,
    /// or the native Taproot cosigner's signatures over the plan digests.
    pub cosigs: Vec<CosignerSig>,
}

/// What a member saw of a signed transaction.
#[derive(CandidType, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainOutcome {
    /// Sent to the network.
    Broadcast,
    /// Final on chain.
    Settled,
    /// Will never be final: refused, reverted or replaced.
    Failed,
}

/// A member's report about a signed transaction. The wallet records it
/// without checking the chain.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChainReport {
    /// Proposal.
    pub proposal: u64,
    /// Signed plan.
    pub plan: u32,
    /// What happened.
    pub outcome: ChainOutcome,
    /// Free text, at most 256 bytes.
    pub note: String,
}

/// Result of `execute`.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecuteResult {
    /// Proposal after execution.
    pub proposal: Proposal,
    /// The transaction signed by this call, for external chains.
    pub signed: Option<SignedTx>,
}

/// Kind of an audit event.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum AuditKind {
    /// Wallet installed.
    Created,
    /// Member joined or bound a Principal.
    Joined,
    /// Proposal made.
    Proposed,
    /// Decision recorded.
    Decided(Decision),
    /// Proposal cancelled.
    Cancelled,
    /// Proposal approved; `Delayed` or `Ready` follows.
    Approved,
    /// Execution began.
    Executing,
    /// Transaction signed.
    Signed {
        /// Plan.
        plan: u32,
        /// Transaction ID.
        txid: String,
    },
    /// Broadcast reported.
    Broadcast {
        /// Plan.
        plan: u32,
        /// Transaction ID.
        txid: String,
    },
    /// Proposal settled.
    Settled,
    /// Proposal failed.
    Failed(String),
    /// Proposal ended without execution: rejected, vetoed, expired or stale.
    Closed(ProposalStatus),
    /// Wallet frozen by a member.
    Frozen,
    /// Cycles topped up.
    Refueled {
        /// ICP ledger block.
        block: u64,
    },
}

/// Hash-chained audit event.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    /// Position from zero.
    pub seq: u64,
    /// Time.
    pub at_ms: u64,
    /// Acting member.
    pub actor: Option<MemberId>,
    /// Proposal.
    pub proposal: Option<u64>,
    /// What happened.
    pub kind: AuditKind,
    /// Hash of the previous event, zero for the first.
    pub prev: Hash,
    /// `digest("dmsg-wallet/audit/v1", (prev, seq, at_ms, actor, proposal, kind))`.
    pub hash: Hash,
}

/// Root public key and chain code of one chain-key algorithm for this
/// wallet: the key at the empty derivation path. Addresses derive from it
/// offline with `derive_subkey_with_chain_code`.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyRoot {
    /// SEC1 compressed or 32-byte Ed25519 key.
    pub public_key: ByteBuf,
    /// Chain code.
    pub chain_code: Hash,
}

/// The wallet's chain-key roots.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct KeyRoots {
    /// Threshold ECDSA secp256k1 (EVM, Bitcoin P2WPKH).
    pub ecdsa: Option<KeyRoot>,
    /// Threshold Schnorr BIP-340 (Bitcoin Taproot).
    pub bip340: Option<KeyRoot>,
    /// Threshold Schnorr Ed25519 (Solana).
    pub ed25519: Option<KeyRoot>,
}

/// What the hub may do with the wallet's code.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CodeAcceptance {
    /// Nothing accepted.
    None,
    /// Install this version.
    Upgrade {
        /// Hub version number.
        version: u32,
        /// SHA-256 of the module.
        module_hash: Hash,
    },
    /// Remove every controller.
    Lock,
}

/// Install argument of a wallet, chosen by the hub.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletInit {
    /// Deployment domain.
    pub environment: Environment,
    /// The hub that created the wallet.
    pub hub: Principal,
    /// Chain-key name: `key_1`, `test_key_1` or `dfx_test_key`.
    pub key_name: String,
    /// Bitcoin network of addresses.
    pub btc_network: BtcNetwork,
    /// ICP ledger.
    pub icp_ledger: Principal,
    /// Cycles minting canister.
    pub cmc: Principal,
    /// Account issuer namespace of the user homes.
    pub issuer_namespace: String,
    /// User homes verifying account members' approvals.
    pub user_homes: Vec<Principal>,
    /// Display name.
    pub name: String,
    /// The creator, an Admin bound to `creator_principal`.
    pub creator: MemberId,
    /// The creator's login Principal.
    pub creator_principal: Principal,
    /// Other members, invited.
    pub members: Vec<MemberInput>,
    /// Initial policy, version 1.
    pub policy: Policy,
    /// Encrypted channel whose members follow wallet events.
    pub channel_hint: Option<String>,
}

/// Public state of a wallet.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletView {
    /// Wire version.
    pub version: u16,
    /// This wallet.
    pub wallet: Principal,
    /// Creating hub.
    pub hub: Principal,
    /// Deployment domain.
    pub environment: Environment,
    /// Display name.
    pub name: String,
    /// Chain-key name.
    pub key_name: String,
    /// Bitcoin network.
    pub btc_network: BtcNetwork,
    /// Chain-key roots, once read.
    pub key_roots: KeyRoots,
    /// Members.
    pub members: Vec<Member>,
    /// Policy.
    pub policy: Policy,
    /// Addresses.
    pub addresses: Vec<Address>,
    /// Cosigners.
    pub cosigners: Vec<Cosigner>,
    /// Whether executions are stopped.
    pub frozen: bool,
    /// What the hub may do with the code.
    pub code: CodeAcceptance,
    /// Next proposal ID.
    pub next_proposal: u64,
    /// Audit events recorded.
    pub audit_len: u64,
    /// Hash of the last audit event.
    pub audit_head: Hash,
    /// Encrypted channel following wallet events.
    pub channel_hint: Option<String>,
    /// Creation time.
    pub created_at_ms: u64,
}

/// Operational counters of a wallet.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletStats {
    /// Cycle balance.
    pub cycles: u128,
    /// Threshold signatures today.
    pub signatures_today: u32,
    /// Proposals stored.
    pub proposals: u64,
    /// Proposals not yet final among the newest 200.
    pub active_proposals: u64,
    /// Stable memory in 64 KiB pages.
    pub stable_pages: u64,
}

/// Page of proposals, newest first.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProposalPage {
    /// Proposals.
    pub proposals: Vec<Proposal>,
    /// Pass as `before` for the next page.
    pub next: Option<u64>,
}

/// Wallets an account may create per base plan.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanQuota {
    /// Plan.
    pub plan: PlanId,
    /// Wallets.
    pub wallets: u32,
}

/// Hub deployment.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubInit {
    /// Deployment domain.
    pub environment: Environment,
    /// SNS governance.
    pub governance: Principal,
    /// ICP ledger.
    pub icp_ledger: Principal,
    /// Cycles minting canister.
    pub cmc: Principal,
    /// Commerce canister serving entitlements.
    pub commerce: Principal,
    /// Account issuer namespace.
    pub issuer_namespace: String,
    /// User homes, append-only.
    pub user_homes: Vec<Principal>,
    /// Chain-key name given to wallets.
    pub key_name: String,
    /// Bitcoin network given to wallets.
    pub btc_network: BtcNetwork,
    /// Pricing and quotas.
    pub pricing: HubPricing,
    /// Account receiving activation revenue.
    #[serde(with = "crate::account::account_cbor")]
    pub revenue: Account,
}

/// Activation price and quotas.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubPricing {
    /// Activation price in ICP e8s.
    pub price_e8s: u64,
    /// Cycles given to each new wallet.
    pub cycles_to_wallet: u128,
    /// Part of each price converted to cycles for the hub, in e8s.
    pub cycles_share_e8s: u64,
    /// Wallets per plan; plans not listed get none.
    pub quotas: Vec<PlanQuota>,
    /// Stops new quotes and creations.
    pub paused: bool,
}

/// Activation terms for one account.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ActivationQuote {
    /// Quote identity.
    pub quote_id: Hash,
    /// Creating account.
    pub account_id: AccountId,
    /// Paying and calling Principal.
    pub payer: Principal,
    /// Base plan read from commerce.
    pub plan: PlanId,
    /// Wallets the account may still create, this one included.
    pub remaining: u32,
    /// Price in ICP e8s.
    pub price_e8s: u64,
    /// ICP ledger fee the payer adds.
    pub ledger_fee_e8s: u64,
    /// Cycles given to the wallet.
    pub cycles_to_wallet: u128,
    /// Account to pay into.
    #[serde(with = "crate::account::account_cbor")]
    pub deposit: Account,
    /// Version installed.
    pub version: u32,
    /// Issue time.
    pub issued_at_ms: u64,
    /// Exclusive deadline for `create_wallet`.
    pub expires_at_ms: u64,
}

/// The wallet a quote creates.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletSpec {
    /// Display name, 1..=64 bytes.
    pub name: String,
    /// Members besides the creator.
    pub members: Vec<MemberInput>,
    /// Initial policy, version 1.
    pub policy: Policy,
    /// Encrypted channel following wallet events.
    pub channel_hint: Option<String>,
}

/// Request to create a paid wallet.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreateWallet {
    /// Paid quote.
    pub quote_id: Hash,
    /// The wallet.
    pub spec: WalletSpec,
    /// The creator's device approval request ID.
    pub approval_id: Hash,
    /// Device-approved terms: purpose `WalletCreate`, service the hub,
    /// `action_digest = activation_digest(quote, spec)`.
    pub approval: ApplicationApproval,
}

/// Progress of an activation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActivationStatus {
    /// Payment and approval verified.
    Paid,
    /// Canister created.
    Created,
    /// Code installed; the wallet is live.
    Ready,
    /// Canister creation failed; the payment may be refunded.
    Failed(String),
    /// Payment returned.
    Refunded {
        /// ICP ledger block.
        block: u64,
    },
}

/// One activation.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ActivationOrder {
    /// Paid quote.
    pub quote: ActivationQuote,
    /// Creating member.
    pub spec: WalletSpec,
    /// Progress.
    pub status: ActivationStatus,
    /// Created wallet.
    pub wallet: Option<Principal>,
    /// Whether the payment moved to the hub's revenue.
    pub collected: bool,
    /// Last change.
    pub updated_at_ms: u64,
}

/// The hub's record of a wallet.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletRecord {
    /// Wallet canister.
    pub wallet: Principal,
    /// Creating account.
    pub creator: AccountId,
    /// Display name at creation.
    pub name: String,
    /// Installed version.
    pub version: u32,
    /// Whether the controllers were removed.
    pub locked: bool,
    /// Members as the wallet last reported them.
    pub members: Vec<MemberId>,
    /// Creation time.
    pub created_at_ms: u64,
}

/// A registered wallet module.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletVersion {
    /// Version number, from 1.
    pub version: u32,
    /// SHA-256 of the module.
    pub module_hash: Hash,
    /// Module bytes.
    pub size: u64,
    /// Release notes.
    pub notes: String,
    /// Registration time.
    pub registered_at_ms: u64,
    /// Whether new wallets stop using it; installed wallets may still accept it.
    pub retired: bool,
}

/// Resource state of a wallet canister, read by its controller, the hub.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletHealth {
    /// Cycle balance.
    pub cycles: u128,
    /// Memory in bytes.
    pub memory_bytes: u128,
    /// Cycles burned per day while idle.
    pub idle_cycles_per_day: u128,
    /// Whether the canister runs.
    pub running: bool,
}

/// Wasm module staged for registration.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StagedModule {
    /// Bytes uploaded so far.
    pub size: u64,
    /// SHA-256 of those bytes.
    pub module_hash: Hash,
}

/// Hub counters.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubStats {
    /// Wallets created.
    pub wallets: u64,
    /// Activations recorded.
    pub orders: u64,
    /// Ready activations whose payment is not yet revenue.
    pub uncollected: u64,
    /// Registered versions.
    pub versions: u32,
    /// Version new wallets get.
    pub current_version: Option<u32>,
    /// Cycle balance.
    pub cycles: u128,
}
