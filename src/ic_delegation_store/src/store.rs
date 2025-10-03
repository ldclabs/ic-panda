use ciborium::{from_reader, into_writer};
use ic_auth_types::ByteBufB64;
use ic_auth_verifier::{verify_delegation_chain, SignInResponse};
use ic_http_certification::{
    cel::{create_cel_expr, DefaultCelBuilder},
    HttpCertification, HttpCertificationPath, HttpCertificationTree, HttpCertificationTreeEntry,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BinaryHeap, HashMap},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

const MAX_TO_PRUNE: usize = 42;

#[derive(Default)]
pub struct State {
    pub allowed_origins: Vec<String>,
    delegations: HashMap<ByteBufB64, ByteBufB64>, // pubkey -> signed delegation payload
    expiration_queue: BinaryHeap<DelegationExpiration>,
}

#[derive(PartialEq, Eq)]
struct DelegationExpiration {
    expires_at: u64, // ms
    pubkey: ByteBufB64,
}

impl Ord for DelegationExpiration {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap is a max heap, but we want expired entries
        // first, hence the inversed order.
        other.expires_at.cmp(&self.expires_at)
    }
}

impl PartialOrd for DelegationExpiration {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl State {
    pub fn get_delegation(&self, pubkey: ByteBufB64, origin: Option<String>) -> Option<ByteBufB64> {
        if !self.allowed_origins.is_empty() {
            if let Some(origin) = origin {
                let in_origin = normalize_origin(&origin);
                let allowed = self.allowed_origins.iter().any(|o| o == in_origin);
                if !allowed {
                    return None;
                }
            }
        }

        self.delegations.get(&pubkey).cloned()
    }

    pub fn put_delegation(
        &mut self,
        signed_delegation: ByteBufB64,
        origin: String,
    ) -> Result<ByteBufB64, String> {
        if !self.allowed_origins.is_empty() {
            let in_origin = normalize_origin(&origin);
            let allowed = self.allowed_origins.iter().any(|o| o == in_origin);
            if !allowed {
                return Err(format!("origin not allowed: {}", origin));
            }
        }

        let now_ms = ic_cdk::api::time() / 1_000_000;

        let obj: SignInResponse = from_reader(&signed_delegation[..])
            .map_err(|e| format!("failed to decode signed delegation: {}", e))?;

        let session_pubkey = obj
            .delegations
            .last()
            .ok_or("no delegation found")?
            .delegation
            .pubkey
            .clone();

        verify_delegation_chain(
            &obj.user_pubkey,
            session_pubkey.as_slice(),
            &obj.delegations,
            now_ms,
            None,
        )?;

        self.prune_expired(now_ms);

        self.delegations
            .insert(session_pubkey.clone(), signed_delegation);
        self.expiration_queue.push(DelegationExpiration {
            expires_at: now_ms + 60 * 1000, // now + 1 minute
            pubkey: session_pubkey.clone(),
        });

        Ok(session_pubkey)
    }

    fn prune_expired(&mut self, now_ms: u64) -> u64 {
        let mut num_pruned = 0;

        for _step in 0..MAX_TO_PRUNE {
            if let Some(expiration) = self.expiration_queue.peek() {
                if expiration.expires_at > now_ms {
                    return num_pruned;
                }
            }
            if let Some(expiration) = self.expiration_queue.pop() {
                self.delegations.remove(&expiration.pubkey);
            }
            num_pruned += 1;
        }

        num_pruned
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static HTTP_TREE: RefCell<HttpCertificationTree> = RefCell::new(HttpCertificationTree::default());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<Vec<u8>, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            Vec::new()
        )
    );
}

pub mod state {
    use super::*;

    use lazy_static::lazy_static;
    use once_cell::sync::Lazy;

    lazy_static! {
        pub static ref DEFAULT_EXPR_PATH: HttpCertificationPath<'static> =
            HttpCertificationPath::wildcard("");
        pub static ref DEFAULT_CERTIFICATION: HttpCertification = HttpCertification::skip();
        pub static ref DEFAULT_CEL_EXPR: String =
            create_cel_expr(&DefaultCelBuilder::skip_certification());
    }

    pub static DEFAULT_CERT_ENTRY: Lazy<HttpCertificationTreeEntry> =
        Lazy::new(|| HttpCertificationTreeEntry::new(&*DEFAULT_EXPR_PATH, *DEFAULT_CERTIFICATION));

    #[derive(Default, Serialize, Deserialize)]
    struct StableState {
        allowed_origins: Vec<String>,
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with_borrow_mut(f)
    }

    pub fn http_tree_with<R>(f: impl FnOnce(&HttpCertificationTree) -> R) -> R {
        HTTP_TREE.with(|r| f(&r.borrow()))
    }

    pub fn init_http_certified_data() {
        HTTP_TREE.with(|r| {
            let mut tree = r.borrow_mut();
            tree.insert(&DEFAULT_CERT_ENTRY);
            ic_cdk::api::certified_data_set(tree.root_hash())
        });
    }

    pub fn load() {
        STATE_STORE.with(|r| {
            STATE.with(|h| {
                let v: StableState =
                    from_reader(&r.borrow().get()[..]).expect("failed to decode STATE_STORE data");
                h.borrow_mut().allowed_origins = v.allowed_origins;
            });
        });
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                let mut buf = vec![];
                into_writer(
                    &StableState {
                        allowed_origins: h.borrow().allowed_origins.clone(),
                    },
                    &mut buf,
                )
                .expect("failed to encode STATE_STORE data");
                r.borrow_mut().set(buf);
            });
        });
    }

    pub fn get_delegation(pubkey: ByteBufB64, origin: Option<String>) -> Option<ByteBufB64> {
        STATE.with_borrow(|s| s.get_delegation(pubkey, origin))
    }

    pub fn put_delegation(
        signed_delegation: ByteBufB64,
        origin: String,
    ) -> Result<ByteBufB64, String> {
        STATE.with_borrow_mut(|s| s.put_delegation(signed_delegation, origin))
    }
}

#[inline]
fn normalize_origin(s: &str) -> &str {
    // 简单归一化：去空白、去尾部/、小写、去掉 http:80 / https:443 默认端口
    let o = s.trim().trim_end_matches('/');
    if o.starts_with("http://") && o.ends_with(":80") {
        &o[..o.len() - 3]
    } else if o.starts_with("https://") && o.ends_with(":443") {
        &o[..o.len() - 4]
    } else {
        o
    }
}
