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

    pub fn value(account: &Account) -> impl Serialize {
        EncodedAccount {
            owner: account.owner,
            subaccount: account.subaccount.map(Hash::new),
        }
    }

    pub fn serialize<S: Serializer>(
        account: &Account,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        value(account).serialize(serializer)
    }

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
