//! Account identity: twelve binary bytes; canonical Xid text at human-readable edges.
use candid::{types::Serializer as CandidSerializer, CandidType};
use ic_auth_types::Xid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, ops::Deref, str::FromStr};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct AccountId(pub [u8; 12]);
impl AccountId {
    pub const fn new(bytes: [u8; 12]) -> Self {
        Self(bytes)
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}
impl Deref for AccountId {
    type Target = [u8; 12];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Xid(self.0).fmt(f)
    }
}
impl fmt::Debug for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AccountId({self})")
    }
}
impl FromStr for AccountId {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let id: Xid = text.parse()?;
        if id.to_string() != text {
            return Err("noncanonical Xid".into());
        }
        Ok(Self(id.0))
    }
}
impl Serialize for AccountId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Xid(self.0).serialize(s)
    }
}
impl<'de> Deserialize<'de> for AccountId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            String::deserialize(d)?
                .parse()
                .map_err(serde::de::Error::custom)
        } else {
            Xid::deserialize(d).map(|id| Self(id.0))
        }
    }
}
impl CandidType for AccountId {
    fn _ty() -> candid::types::Type {
        Vec::<u8>::_ty()
    }
    fn idl_serialize<S: CandidSerializer>(&self, s: S) -> Result<(), S::Error> {
        s.serialize_blob(&self.0)
    }
}
