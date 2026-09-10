//! Account identity: twelve binary bytes; canonical Xid text at human-readable edges.
/// Stable account identity: 12 binary bytes, canonical 20-character Xid text.
/// Allocated by the user canister; never truncate a Hash or Principal to create one.
pub use ic_auth_types::Xid as AccountId;
