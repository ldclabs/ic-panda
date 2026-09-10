//! Explicit Serde representation of ICRC ledger accounts.
//!
//! A ledger account (`owner` Principal plus optional 32-byte subaccount) is
//! distinct from a dMsg [`crate::AccountId`]. This adapter preserves subaccounts
//! as CBOR byte strings, including when nested in signed payment terms.
/// Serde `with` adapter for the public ICRC Account map.
///
/// Use `#[serde(with = "dmsg_types::account::account_cbor")]` on Account fields.
pub mod account_cbor {
    use crate::Hash;
    use candid::Principal;
    use icrc_ledger_types::icrc1::account::Account;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct EncodedAccount {
        owner: Principal,
        subaccount: Option<Hash>,
    }

    /// Return a serializable account view with a byte-string subaccount.
    pub fn value(account: &Account) -> impl Serialize {
        EncodedAccount {
            owner: account.owner,
            subaccount: account.subaccount.map(Hash::new),
        }
    }

    /// Serde `with` hook encoding owner and optional subaccount using the protocol representation.
    pub fn serialize<S: Serializer>(
        account: &Account,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        value(account).serialize(serializer)
    }

    /// Serde `with` hook decoding a ledger account; rejects invalid subaccount lengths.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Account, D::Error> {
        let account = EncodedAccount::deserialize(deserializer)?;
        Ok(Account {
            owner: account.owner,
            subaccount: account.subaccount.map(Hash::into_array),
        })
    }
}
