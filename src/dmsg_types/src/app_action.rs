//! Application-action profile. Each application declares its commands in a
//! governance-registered [`ActionSchema`](crate::app_action::ActionSchema); dMsg
//! owns only the closed value model, its canonical encoding and the rendering
//! rules. Product authority must attest to the exact action, and the receiver
//! must execute exactly the signed command.
use crate::{AccountId, Environment, Hash};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Full application decision, separate from portable document statements.
/// All timestamps are Unix milliseconds. No field is a caller-supplied display summary.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppAction {
    /// Profile version, exactly one.
    pub version: u16,
    /// Deployment domain.
    pub environment: Environment,
    /// Governance registered application.
    pub app_id: String,
    /// Exact registration revision used for preparation.
    pub app_config_version: u64,
    /// Exact browser origin; unlike document profiles, this is in signed content.
    pub origin: String,
    /// Canister that commits the product action.
    pub receiver: Principal,
    /// Product's stable actor identity, opaque to dMsg and distinct from the signer.
    pub actor: ByteBuf,
    /// Exact dMsg account selected by the product preparation, distinct from actor.
    pub signing_account: AccountId,
    /// Product request identity; not the dMsg execution identity.
    pub operation_id: Hash,
    /// Commitment to the complete product intent, including the product's own
    /// subject, precondition, role and policy commitments.
    pub intent_hash: Hash,
    /// Digest of the registered schema that gives the command its meaning.
    pub schema_hash: Hash,
    /// First valid time.
    pub issued_at_ms: u64,
    /// Exclusive end of the intent's validity window, at most five minutes.
    pub expires_at_ms: u64,
    /// Typed product command, checked against the schema named by `schema_hash`.
    pub command: ActionCommand,
    /// Immutable file/version references, ordered by file_id.
    pub files: Vec<ActionFile>,
}

/// One command of the application's schema, with every declared argument.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionCommand {
    /// Command name declared by the schema.
    pub name: String,
    /// Every declared argument, in schema order.
    pub args: ActionArgs,
}

/// Named values in schema order: a command's arguments or a record's fields.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ActionArgs(pub Vec<ActionArg>);

/// Named argument; the name must equal the schema field at its position.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionArg {
    /// Field name declared by the schema.
    pub name: String,
    /// Value of the declared field type.
    pub value: ActionValue,
}

/// Closed value model shared by every application. There is no opaque byte value.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActionValue {
    /// Unsigned integer within the declared range.
    Nat(u64),
    /// Boolean shown with the schema's yes/no labels.
    Bool(bool),
    /// Verbatim text, never interpreted as markup.
    Text(String),
    /// Nonzero 32-byte commitment, shown in hex.
    Hash(Hash),
    /// Authenticated principal, shown in text form.
    Principal(Principal),
    /// One option value declared by the schema.
    Choice(String),
    /// Exact external artifact reference.
    Artifact(ActionArtifact),
    /// Absent value of an optional field.
    Null,
    /// Items of one declared type.
    List(Vec<ActionValue>),
    /// Every field of a declared record, in schema order.
    Record(ActionArgs),
}

/// Governance-registered command vocabulary of one application.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionSchema {
    /// Schema format version, exactly one.
    pub version: u16,
    /// Commands the application may request, with unique names.
    pub commands: Vec<CommandSchema>,
}

/// One command: its displayed title and complete argument list.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommandSchema {
    /// Stable command name.
    pub name: String,
    /// Displayed title, one entry per locale.
    pub title: Vec<ActionLabel>,
    /// Arguments in display and encoding order, with unique names.
    pub fields: SchemaFields,
}

/// Fields in display and encoding order: a command's arguments or a record's fields.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SchemaFields(pub Vec<FieldSchema>);

/// One argument or record field. Every field is always displayed.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FieldSchema {
    /// Stable field name.
    pub name: String,
    /// Displayed label, one entry per locale.
    pub label: Vec<ActionLabel>,
    /// Accepted value type and bounds.
    pub ty: FieldType,
}

/// Plain display text for one locale; never markup.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionLabel {
    /// BCP 47 language tag, such as `en` or `zh`.
    pub locale: String,
    /// Display text.
    pub text: String,
}

/// One allowed choice value and its displayed label.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    /// Encoded value.
    pub value: String,
    /// Displayed label, one entry per locale.
    pub label: Vec<ActionLabel>,
}

/// Field types and bounds. Nesting is limited to three levels.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FieldType {
    /// Unsigned integer in the inclusive range.
    Nat {
        /// Inclusive minimum.
        min: u64,
        /// Inclusive maximum.
        max: u64,
    },
    /// Boolean with explicit labels for both values.
    Bool {
        /// Label of `true`.
        yes: Vec<ActionLabel>,
        /// Label of `false`.
        no: Vec<ActionLabel>,
    },
    /// Nonblank verbatim text.
    Text {
        /// Maximum UTF-8 length.
        max_bytes: u64,
        /// Whether line breaks and tabs are allowed and preserved.
        multiline: bool,
    },
    /// Nonzero 32-byte commitment.
    Hash,
    /// Authenticated principal.
    Principal,
    /// One of the declared options.
    Choice {
        /// Allowed values with unique names.
        options: Vec<ChoiceOption>,
    },
    /// External artifact reference.
    Artifact,
    /// Either `Null` or a value of the item type.
    Optional {
        /// Present value type; not itself optional.
        item: Box<FieldType>,
    },
    /// Items of one type.
    List {
        /// Item type.
        item: Box<FieldType>,
        /// Maximum item count.
        max_items: u64,
    },
    /// Fixed fields.
    Record {
        /// Fields in display and encoding order, with unique names.
        fields: SchemaFields,
    },
}

/// Exact external artifact reference; no automatic URI fetching.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionArtifact {
    /// HTTPS or IPFS reference.
    pub uri: String,
    /// Digest of exact bytes, not encoded digest text.
    pub sha256: Hash,
    /// Declared content type.
    pub content_type: String,
    /// Exact size in bytes.
    pub size: u64,
}

/// Exact product file version presented for this action.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionFile {
    /// Stable, product-scoped file locator.
    pub file_id: String,
    /// Immutable revision; positive.
    pub revision: u64,
    /// Exact SHA-256 bytes.
    pub sha256: Hash,
    /// Exact byte length.
    pub byte_length: u64,
    /// Media type, not inferred from the file name.
    pub media_type: String,
    /// Optional display name committed alongside the digest.
    pub display_name: Option<String>,
    /// Whether bytes are plaintext originals or ciphertext.
    pub representation: ActionFileRepresentation,
}

/// File digest interpretation. Neither value proves the original was read.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActionFileRepresentation {
    /// Hash names the original bytes.
    Original,
    /// Hash names encrypted bytes.
    Encrypted,
}
