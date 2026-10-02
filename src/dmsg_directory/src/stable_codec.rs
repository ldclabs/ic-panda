use crate::store::{Config, Record};
use candid::Principal;
use cbor2::Cbor;
use dmsg_runtime::storage::StableCodec;
use dmsg_types::{agent::*, *};
use serde_bytes::ByteBuf;

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct DirectoryInitRepr {
    #[cbor(key = 1)]
    pub environment: Environment,
    #[cbor(key = 2)]
    pub issuer_namespace: String,
    #[cbor(key = 3)]
    pub user_homes: Vec<Principal>,
    #[cbor(key = 4)]
    pub principal_origin: String,
    #[cbor(key = 5)]
    pub controller_source: String,
    #[cbor(key = 6)]
    pub delegation_query_url: String,
    #[cbor(key = 7)]
    pub profile_url_prefix: String,
    #[cbor(key = 8)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_domains: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ConfigRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub init: DirectoryInitRepr,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        let init = &self.init;
        ConfigRepr {
            schema: self.schema,
            init: DirectoryInitRepr {
                environment: init.environment.clone(),
                issuer_namespace: init.issuer_namespace.clone(),
                user_homes: init.user_homes.clone(),
                principal_origin: init.principal_origin.clone(),
                controller_source: init.controller_source.clone(),
                delegation_query_url: init.delegation_query_url.clone(),
                profile_url_prefix: init.profile_url_prefix.clone(),
                custom_domains: init.custom_domains.clone(),
            },
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        let init = repr.init;
        Self {
            schema: repr.schema,
            init: DirectoryInit {
                environment: init.environment,
                issuer_namespace: init.issuer_namespace,
                user_homes: init.user_homes,
                principal_origin: init.principal_origin,
                controller_source: init.controller_source,
                delegation_query_url: init.delegation_query_url,
                profile_url_prefix: init.profile_url_prefix,
                custom_domains: init.custom_domains,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct RecordRepr {
    #[cbor(key = 1)]
    pub home_user: Principal,
    #[cbor(key = 2)]
    pub version: u64,
    #[cbor(key = 3)]
    pub updated_at: u64,
    #[cbor(key = 4)]
    pub state_digest: Hash,
    #[cbor(key = 5)]
    pub document: ByteBuf,
    #[cbor(key = 6)]
    pub document_digest: Hash,
}

impl StableCodec for Record {
    type Repr = RecordRepr;

    fn to_repr(&self) -> Self::Repr {
        RecordRepr {
            home_user: self.home_user,
            version: self.version,
            updated_at: self.updated_at,
            state_digest: self.state_digest,
            document_digest: self.document_digest,
            document: self.document.clone(),
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            home_user: repr.home_user,
            version: repr.version,
            updated_at: repr.updated_at,
            state_digest: repr.state_digest,
            document_digest: repr.document_digest,
            document: repr.document,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};

    #[test]
    fn config_roundtrip_preserves_all_fields_and_empty_domains() {
        let mut config = Config {
            schema: crate::store::STABLE_SCHEMA,
            init: DirectoryInit {
                environment: Environment::Staging,
                issuer_namespace: "https://dmsg.test/u/".into(),
                user_homes: vec![Principal::from_slice(&[1]), Principal::from_slice(&[2])],
                principal_origin: "https://id.dmsg.test".into(),
                controller_source: "https://dmsg.test".into(),
                delegation_query_url: "https://agents.dmsg.test/query".into(),
                profile_url_prefix: "https://dmsg.test/u/".into(),
                custom_domains: vec![],
            },
        };
        for domains in [vec![], vec!["id.dmsg.test".into(), "id2.dmsg.test".into()]] {
            config.init.custom_domains = domains;
            let bytes = compact_bytes(&config);
            let decoded: Config = compact_from_bytes(&bytes);
            assert_eq!(decoded.schema, config.schema);
            assert_eq!(decoded.init, config.init);
            assert_eq!(compact_bytes(&decoded), bytes);
        }
    }

    #[test]
    fn record_roundtrip_preserves_exact_body_and_both_digests() {
        let document = br#"{"controllers":[],"type":"person"}"#.to_vec();
        let record = Record {
            home_user: Principal::from_slice(&[1]),
            version: 42,
            updated_at: 1_790_000_000_000,
            state_digest: Hash::new([7; 32]),
            document_digest: dmsg_protocol::sha256(&document),
            document: document.into(),
        };
        let bytes = compact_bytes(&record);
        let decoded: Record = compact_from_bytes(&bytes);
        assert_eq!(decoded, record);
        assert_eq!(
            decoded.document_digest,
            dmsg_protocol::sha256(&decoded.document)
        );
        assert_eq!(compact_bytes(&decoded), bytes);
    }
}
