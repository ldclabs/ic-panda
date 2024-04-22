use candid::{CandidType, Nat, Principal};
use ciborium::{from_reader, into_writer};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, StableMinHeap, Storable,
};
use lib_panda::{mac_256, Cryptogram};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    ops,
};

use crate::utils::{luck_amount, luckycode_from_string, luckycode_to_string};
use crate::{types, SECOND, TOKEN_1, TOKEN_SMALL_UNIT};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub airdrop_balance: u64,
    pub total_airdrop: u64,
    pub total_airdrop_count: u64,
    pub total_luckydraw: u64,
    pub total_luckydraw_icp: u64,
    pub total_luckydraw_count: u64,
    pub total_prize: Option<u64>,
    pub total_prize_count: Option<u64>,  // prize claiming count
    pub total_prizes_count: Option<u64>, // total prizes count
    pub latest_airdrop_logs: Vec<types::AirdropLog>, // latest 10 airdrop logs
    pub luckiest_luckydraw_logs: Vec<types::LuckyDrawLog>, // latest 10 luckiest luckydraw logs
    pub latest_luckydraw_logs: Vec<types::LuckyDrawLog>, // latest 10 luckydraw logs
    pub managers: Option<BTreeSet<Principal>>,
    pub airdrop_amount: Option<u64>,
    pub prize_subsidy: Option<SysPrizeSubsidy>,
    pub lucky_code: Option<u32>,
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode State data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode State data")
    }
}

// SysPrizeSubsidy
#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct SysPrizeSubsidy(
    pub u64, // Prize fee in PANDA * TOKEN_1
    pub u16, // Min quantity requirement for subsidy
    pub u32, // Min total amount tokens requirement for subsidy
    pub u8,  // Subsidy ratio, [0, 50]
    pub u32, // Max subsidy tokens per prize
    pub u16, // Subsidy count limit
);
impl Storable for SysPrizeSubsidy {
    const BOUND: Bound = Bound::Bounded {
        max_size: 28,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode SysPrizeSubsidy data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode SysPrizeSubsidy data")
    }
}

impl SysPrizeSubsidy {
    pub fn subsidy(&self, claimable: u32, quantity: u16) -> u32 {
        if quantity < self.1 || claimable < self.2 || self.5 == 0 {
            return 0;
        }

        let subsidy = claimable * self.3 as u32 / 100;
        if subsidy > self.4 {
            self.4
        } else {
            subsidy
        }
    }
}

// AirdropState format: (lucky code, total claimed tokens, claimable tokens)
// If total claimed tokens is smaller than TOKEN_1, it is effective timestamp in hours since the UNIX epoch.
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct AirdropState(pub u32, pub u64, pub u64);

impl Storable for AirdropState {
    const BOUND: Bound = Bound::Bounded {
        max_size: 24,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode AirdropState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode AirdropState data")
    }
}

// AirdropLog format: (user, time, token_amount, lucky_code)
#[derive(Clone, Deserialize, Serialize)]
pub struct AirdropLog(Principal, u64, u64, u32);

impl From<(u64, AirdropLog)> for types::AirdropLog {
    fn from(log: (u64, AirdropLog)) -> Self {
        let (idx, log) = log;
        types::AirdropLog {
            id: Nat::from(idx),
            ts: log.1,
            caller: log.0,
            amount: Nat::from(log.2),
            lucky_code: luckycode_to_string(log.3),
        }
    }
}

impl Storable for AirdropLog {
    const BOUND: Bound = Bound::Bounded {
        max_size: 55,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode AirdropLog data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode AirdropLog data")
    }
}

// AirdropLog format: (user, time, token_amount, icp_amount, random_number)
#[derive(Clone, Deserialize, Serialize)]
pub struct LuckyDrawLog(Principal, u64, u64, u64, u64);

impl From<(u64, LuckyDrawLog)> for types::LuckyDrawLog {
    fn from(log: (u64, LuckyDrawLog)) -> Self {
        let (idx, log) = log;
        types::LuckyDrawLog {
            id: Nat::from(idx),
            ts: log.1,
            caller: log.0,
            amount: Nat::from(log.2),
            icp_amount: Nat::from(log.3),
            random: log.4,
        }
    }
}

impl Storable for LuckyDrawLog {
    const BOUND: Bound = Bound::Bounded {
        max_size: 68,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode LuckyDrawLog data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode LuckyDrawLog data")
    }
}

// Prize format: (Issuer code, Issue time, Expire, Claimable amount, Quantity)
// System can only issue prizes for free airdrop with Prize(0, Issue time, expire, 0, 0).
// System prizes will not be stored.
#[derive(Clone, Debug, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct Prize(
    pub u32, // Issuer code: The lucky code of the issuer, 0 for system
    pub u32, // Issue time: The issue time of the prize, in minutes since UNIX epoch
    pub u16, // Expire: The expire duration in minutes
    pub u32, // Total amount: The amount of tokens that can be claimed by users, in PANDA
    pub u16, // Quantity: How many users can claim the prize
);

impl Storable for Prize {
    const BOUND: Bound = Bound::Bounded {
        max_size: 22,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Prize data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Prize data")
    }
}

impl Prize {
    pub fn is_valid(&self, now_sec: u64) -> bool {
        (self.1 + self.2 as u32) >= (now_sec / 60) as u32
    }

    pub fn is_valid_system(&self, now_sec: u64) -> bool {
        self.0 == 0 && self.3 == 0 && self.4 == 0 && self.is_valid(now_sec)
    }

    fn range_bounds_of(
        issuer: u32,
        start_ts: u32,
        end_ts: u32,
    ) -> (ops::Bound<Prize>, ops::Bound<Prize>) {
        (
            ops::Bound::Excluded(Prize(issuer, start_ts, 0, 0, 0)),
            ops::Bound::Excluded(Prize(issuer, end_ts, 0, 0, 0)),
        )
    }
}

// Only use for airdrop cryptogram.
// IssuerPrize key: (Issue time, Expire, Claimable tokens, Quantity)
// IssuerPrize value: filled quantity
#[derive(Clone, Deserialize, Serialize)]
pub struct IssuerPrizes(pub BTreeMap<(u32, u16, u32, u16), u16>);
impl Storable for IssuerPrizes {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode IssuerPrizes data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode IssuerPrizes data")
    }
}

// Only use for airdrop cryptogram.
#[derive(Clone, Deserialize, Serialize)]
pub struct Principals(BTreeSet<Principal>);
impl Storable for Principals {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Principals data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Principals data")
    }
}

// PrizeInfo
#[derive(Clone, Deserialize, Serialize)]
pub struct PrizeInfo(
    pub u8,              // Prize Kind, 0 - fixed amount prize, 1 - lucky amount prize
    pub u64,             // Fee in PANDA * TOKEN_1
    pub u32,             // Sys Subsidy in tokens
    pub u64,             // Refund amount in PANDA * TOKEN_1
    pub u16,             // Filled quantity
    pub u64,             // End Time in seconds since UNIX epoch
    pub Option<ByteBuf>, // Memo
);
impl Storable for PrizeInfo {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode PrizeInfo data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode PrizeInfo data")
    }
}

// PrizeRecipients: key - luck code, value - amount in PANDA * TOKEN_SMALL_UNIT
#[derive(Clone, Deserialize, Serialize)]
pub struct PrizeRecipients(BTreeMap<u32, u32>);
impl Storable for PrizeRecipients {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode PrizeRecipients data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode PrizeRecipients data")
    }
}

// PrizeClaimLog format: (Prize, Claimed Time, claimed Amount in PANDA * TOKEN_1)
#[derive(Clone, Deserialize, Serialize)]
pub struct PrizeClaimLogs(Vec<(Prize, u64, u64)>);
impl Storable for PrizeClaimLogs {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode PrizeClaimLogs data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode PrizeClaimLogs data")
    }
}

// PrizeRefund: (Check time in minutes, Prize)
// Check time: Prize.1 + Prize.2 + 1
#[derive(Clone, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct PrizeRefund(pub u32, pub Prize);
impl Storable for PrizeRefund {
    const BOUND: Bound = Bound::Bounded {
        max_size: 28,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode PrizeRefund data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode PrizeRefund data")
    }
}

// NamingState
#[derive(Clone, Deserialize, Serialize)]
pub struct NamingState(
    pub String, // Unique name
    pub u64,    // Created time in seconds
    pub u32,    // Deposit amount in tokens
    pub u32,    // Annual fee in tokens
);
impl Storable for NamingState {
    const BOUND: Bound = Bound::Bounded {
        max_size: 86,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode NamingState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode NamingState data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const AIRDROP_MEMORY_ID: MemoryId = MemoryId::new(1);
const LUCKYCODE_MEMORY_ID: MemoryId = MemoryId::new(2);
const AIRDROP_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(3);
const AIRDROP_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(4);
const LUCKYDRAW_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(5);
const LUCKYDRAW_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(6);
const ISSUER_PRIZE_MEMORY_ID: MemoryId = MemoryId::new(7);
const PRIZE_MEMORY_ID: MemoryId = MemoryId::new(8);
const KEYS_MEMORY_ID: MemoryId = MemoryId::new(9);
const X_AUTH_MEMORY_ID: MemoryId = MemoryId::new(10);
const NAMING_MEMORY_ID: MemoryId = MemoryId::new(11);
const NAMING_STATE_MEMORY_ID: MemoryId = MemoryId::new(12);
const PRIZE_INFO_MEMORY_ID: MemoryId = MemoryId::new(13);
const PRIZE_REC_MEMORY_ID: MemoryId = MemoryId::new(14);
const PRIZE_LOG_MEMORY_ID: MemoryId = MemoryId::new(15);
const PRIZE_REFUND_MEMORY_ID: MemoryId = MemoryId::new(16);

thread_local! {
    static CAPTCHA_SECRET: RefCell<[u8; 32]> = const { RefCell::new([0; 32]) };
    static CHALLENGE_PUB_KEY: RefCell<[u8; 32]> = const { RefCell::new([0; 32]) };

    static STATE_HEAP: RefCell<State> = RefCell::new(State::default());

    static ACTIVE_USERS: RefCell<BTreeSet<Principal>> = const { RefCell::new(BTreeSet::new()) };

    static MANAGERS: RefCell<BTreeSet<Principal>> = const { RefCell::new(BTreeSet::new()) };

    static NOTIFICATIONS: RefCell<BTreeMap<u8, types::Notification>> = const { RefCell::new(BTreeMap::new()) };

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    static AIRDROP: RefCell<StableBTreeMap<Principal, AirdropState, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_MEMORY_ID)),
        )
    );

    static LUCKYCODE: RefCell<StableBTreeMap<u32, Principal, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKYCODE_MEMORY_ID)),
        )
    );

    static AIRDROP_LOGS: RefCell<StableLog<AirdropLog, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_LOG_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_LOG_DATA_MEMORY_ID)),
        ).expect("failed to init AIRDROP_LOGS store")
    );

    static LUCKYDRAW_LOGS: RefCell<StableLog<LuckyDrawLog, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKYDRAW_LOG_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKYDRAW_LOG_DATA_MEMORY_ID)),
        ).expect("failed to init LUCKY_DRAW_LOGS store")
    );

    // Only use for airdrop cryptogram.
    static ISSUER_PRIZE: RefCell<StableBTreeMap<u32, IssuerPrizes, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(ISSUER_PRIZE_MEMORY_ID)),
        )
    );

    // Only use for airdrop cryptogram.
    static PRIZE: RefCell<StableBTreeMap<Prize, Principals, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(PRIZE_MEMORY_ID)),
        )
    );

    static KEYS: RefCell<StableBTreeMap<String, Vec<u8>, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(KEYS_MEMORY_ID)),
        )
    );

    static X_AUTH: RefCell<StableBTreeMap<String, (Principal, u64), Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(X_AUTH_MEMORY_ID)),
        )
    );

    static NAMING: RefCell<StableBTreeMap<String, u32, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(NAMING_MEMORY_ID)),
        )
    );

    static NAMING_STATE: RefCell<StableBTreeMap<u32, NamingState, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(NAMING_STATE_MEMORY_ID)),
        )
    );

    static PRIZE_INFO: RefCell<StableBTreeMap<Prize, PrizeInfo, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(PRIZE_INFO_MEMORY_ID)),
        )
    );

    static PRIZE_REC: RefCell<StableBTreeMap<Prize, PrizeRecipients, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(PRIZE_REC_MEMORY_ID)),
        )
    );

    static PRIZE_LOG: RefCell<StableBTreeMap<u32, PrizeClaimLogs, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(PRIZE_LOG_MEMORY_ID)),
        )
    );

    static PRIZE_REFUND: RefCell<StableMinHeap<PrizeRefund, Memory>> = RefCell::new(
        StableMinHeap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(PRIZE_REFUND_MEMORY_ID)),
        ).expect("failed to init PRIZE_REFUND store")
    );
}

pub mod keys {
    use super::*;

    pub async fn load() {
        let keys = KEYS.with(|r| r.borrow().iter().collect::<BTreeMap<String, Vec<u8>>>());
        {
            let mut secret: Vec<u8> = match keys.get("CAPTCHA_SECRET") {
                Some(secret) => secret.clone(),
                None => vec![],
            };
            if secret.len() != 32 {
                let rr = ic_cdk::api::management_canister::main::raw_rand()
                    .await
                    .expect("failed to get random bytes");
                secret = mac_256(&rr.0, b"CAPTCHA_SECRET").to_vec();
            }
            CAPTCHA_SECRET.with(|r| r.borrow_mut().copy_from_slice(&secret));
        }
        {
            let key: Vec<u8> = match keys.get("CHALLENGE_PUB_KEY") {
                Some(secret) => secret.clone(),
                None => vec![],
            };
            if key.len() == 32 {
                CHALLENGE_PUB_KEY.with(|r| r.borrow_mut().copy_from_slice(&key));
            }
        }
    }

    pub fn save() {
        KEYS.with(|r| {
            r.borrow_mut().insert(
                "CAPTCHA_SECRET".to_string(),
                CAPTCHA_SECRET.with(|r| r.borrow().to_vec()),
            );
            r.borrow_mut().insert(
                "CHALLENGE_PUB_KEY".to_string(),
                CHALLENGE_PUB_KEY.with(|r| r.borrow().to_vec()),
            );
        });
    }

    pub fn with_secret<R>(f: impl FnOnce(&[u8; 32]) -> R) -> R {
        CAPTCHA_SECRET.with(|r| f(&r.borrow()))
    }

    pub fn with_challenge_pub_key<R>(f: impl FnOnce(&[u8; 32]) -> R) -> R {
        CHALLENGE_PUB_KEY.with(|r| f(&r.borrow()))
    }

    pub fn set_challenge_pub_key(key: [u8; 32]) {
        CHALLENGE_PUB_KEY.with(|r| *r.borrow_mut() = key);
    }

    pub static AIRDROP_KEY: Lazy<[u8; 32]> =
        Lazy::new(|| with_secret(|b| mac_256(b, b"AIRDROP_KEY")));
    pub static PRIZE_KEY: Lazy<[u8; 32]> = Lazy::new(|| with_secret(|b| mac_256(b, b"PRIZE_KEY")));
}

pub mod luckycode {
    use super::*;

    // pub fn get(code: u32) -> Option<Principal> {
    //     LUCKYCODE.with(|r| r.borrow().get(&code))
    // }

    pub fn get_by_string(code: &str) -> Option<Principal> {
        match luckycode_from_string(code) {
            Ok(code) => LUCKYCODE.with(|r| r.borrow().get(&code)),
            Err(_) => None,
        }
    }

    pub fn new_from(user: Principal) -> u32 {
        LUCKYCODE.with(|r| {
            let mut m = r.borrow_mut();
            let mut code = state::next_lucky_code();
            let mut i = 0u32;
            while m.contains_key(&code) {
                if i > 1000 {
                    ic_cdk::trap("failed to generate a lucky code");
                }
                i += 1;
                code = state::next_lucky_code();
            }
            m.insert(code, user);
            code
        })
    }
}

pub mod xauth {
    use super::*;

    // pub fn get(id: &String) -> Option<(Principal, u64)> {
    //     X_AUTH.with(|r| r.borrow().get(id))
    // }

    // pub fn exists(id: &String) -> bool {
    //     X_AUTH.with(|r| r.borrow().contains_key(id))
    // }

    pub fn try_set(id: String, user: Principal, now_sec: u64) -> bool {
        X_AUTH.with(|r| {
            let mut m = r.borrow_mut();
            if m.contains_key(&id) {
                return false;
            }

            m.insert(id, (user, now_sec));
            true
        })
    }
}

pub mod airdrop {
    use super::*;

    // check if a user has claimed airdrop.
    pub fn state_of(user: &Principal) -> Option<AirdropState> {
        AIRDROP.with(|r| r.borrow().get(user))
    }

    // update luckydraw state and append a log.
    // return the log or an error message when append failed.
    pub fn insert(
        user: Principal,
        referrer: Option<Principal>,
        now_sec: u64,
        amount: u64,
        rebate_bonus: u64,
        caller_code: u32,
    ) -> Result<types::AirdropLog, String> {
        let referrer_code = AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            m.insert(user, AirdropState(caller_code, 0, amount));

            match referrer {
                None => 0,
                Some(referrer) => match m.get(&referrer) {
                    None => 0,
                    Some(state) => {
                        m.insert(
                            referrer,
                            AirdropState(state.0, state.1, state.2.saturating_add(rebate_bonus)),
                        );
                        state.0
                    }
                },
            }
        });

        let log = AirdropLog(user, now_sec, amount, referrer_code);
        let idx = AIRDROP_LOGS
            .with(|r| r.borrow_mut().append(&log))
            .map_err(|err| format!("failed to append airdrop log, error {:?}", err))?;
        Ok(types::AirdropLog::from((idx, log)))
    }

    pub fn deposit(user: Principal, amount: u64) -> Result<AirdropState, String> {
        AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                None => Err("no lucky code to deposit".to_string()),
                Some(state) => {
                    let state = AirdropState(state.0, state.1, state.2.saturating_add(amount));
                    m.insert(user, state.clone());
                    Ok(state)
                }
            }
        })
    }

    pub fn withdraw(user: Principal, amount: u64) -> Result<AirdropState, String> {
        AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                // should never happen, we have checked the state before calling this function
                None => Err("no lucky code to withdraw".to_string()),
                Some(state) => {
                    if state.2 < amount {
                        return Err("insufficient lucky balance to withdraw".to_string());
                    }
                    let state = AirdropState(
                        state.0,
                        if state.1 >= TOKEN_1 {
                            state.1.saturating_add(amount)
                        } else {
                            amount
                        },
                        state.2.saturating_sub(amount),
                    );
                    m.insert(user, state.clone());
                    Ok(state)
                }
            }
        })
    }

    pub fn ban_users(users: Vec<Principal>) -> Result<(), String> {
        AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            for user in users {
                if let Some(state) = m.get(&user) {
                    m.insert(user, AirdropState(0, state.1, state.2));
                }
            }
        });
        Ok(())
    }

    // get airdrop logs in reverse order, return the next index to fetch.
    pub fn logs(prev: Option<u64>, take: usize) -> Vec<types::AirdropLog> {
        AIRDROP_LOGS.with(|r| {
            let log_store = r.borrow();
            let latest = log_store.len();
            if latest == 0 {
                return vec![];
            }

            let prev = prev.unwrap_or(latest);
            if prev > latest || prev == 0 {
                return vec![];
            }

            let mut idx = prev - 1;
            let mut logs: Vec<types::AirdropLog> = Vec::with_capacity(take);
            while let Some(log) = log_store.get(idx) {
                logs.push(types::AirdropLog::from((idx, log)));

                if idx == 0 || logs.len() >= take {
                    break;
                }
                idx -= 1;
            }

            logs
        })
    }
}

pub mod luckydraw {
    use super::*;

    // insert luckydraw state and append a log.
    // return the log or an error message when append failed.
    pub fn insert(
        user: Principal,
        now_sec: u64,
        token_amount: u64,
        icp_amount: u64,
        random: u64,
    ) -> Result<types::LuckyDrawLog, String> {
        let log = LuckyDrawLog(user, now_sec, token_amount, icp_amount, random);
        let idx = LUCKYDRAW_LOGS
            .with(|r| r.borrow_mut().append(&log))
            .map_err(|err| format!("failed to append luckydraw log, error {:?}", err))?;
        Ok(types::LuckyDrawLog::from((idx, log)))
    }

    // get luckydraw logs in reverse order, return the next index to fetch.
    pub fn logs(
        prev: Option<u64>,
        take: usize,
        user: Option<Principal>,
    ) -> Vec<types::LuckyDrawLog> {
        LUCKYDRAW_LOGS.with(|r| {
            let log_store = r.borrow();
            let latest = log_store.len();
            if latest == 0 {
                return vec![];
            }

            let prev = prev.unwrap_or(latest);
            if prev > latest || prev == 0 {
                return vec![];
            }

            let mut idx = prev - 1;
            let mut max_loop = 1000;
            let mut logs: Vec<types::LuckyDrawLog> = Vec::with_capacity(take);
            while let Some(log) = log_store.get(idx) {
                match user {
                    Some(ref id) => {
                        if &log.0 == id {
                            logs.push(types::LuckyDrawLog::from((idx, log)));
                        }
                    }
                    None => {
                        logs.push(types::LuckyDrawLog::from((idx, log)));
                    }
                }

                if idx == 0 || max_loop == 0 || logs.len() >= take {
                    break;
                }
                idx -= 1;
                max_loop -= 1;
            }

            logs
        })
    }
}

pub mod prize {
    use super::*;

    pub fn get_info(prize: &Prize) -> Option<PrizeInfo> {
        PRIZE_INFO.with(|r| r.borrow().get(prize))
    }

    pub fn add(prize: Prize, info: PrizeInfo) -> bool {
        PRIZE_INFO.with(|r| {
            let mut m = r.borrow_mut();
            if m.contains_key(&prize) {
                return false;
            }
            m.insert(prize.clone(), info);

            PRIZE_REC.with(|r| {
                r.borrow_mut()
                    .insert(prize, PrizeRecipients(BTreeMap::new()));
            });
            true
        })
    }

    pub fn add_refund_job(prize: Prize) -> Result<(), String> {
        let check_time = prize.1 + prize.2 as u32 + 1;
        PRIZE_REFUND.with(|r| {
            r.borrow_mut()
                .push(&PrizeRefund(check_time, prize))
                .map_err(|err| format!("failed to add refund job, error {:?}", err))
        })
    }

    pub fn handle_refund_jobs() {
        while let Some(job) = PRIZE_REFUND.with(|r| r.borrow().peek()) {
            let now_sec = ic_cdk::api::time() / SECOND;
            if job.0 as u64 * 60 > now_sec {
                break;
            }

            let job = match PRIZE_REFUND.with(|r| r.borrow_mut().pop()) {
                Some(job) => job,
                None => break,
            };

            let prize = job.1;
            let mut info = match PRIZE_INFO.with(|r| r.borrow().get(&prize)) {
                Some(info) => {
                    if info.4 < prize.4 {
                        info
                    } else {
                        continue;
                    }
                }
                None => continue,
            };

            let claimed = PRIZE_REC.with(|r| {
                r.borrow()
                    .get(&prize)
                    .map(|recipients| recipients.0.values().map(|v| *v as u64).sum())
                    .unwrap_or(0)
            });
            let refund = (prize.3 - info.2) as u64 * TOKEN_SMALL_UNIT - claimed;
            if refund == 0 {
                continue;
            }
            let refund = refund * (TOKEN_1 / TOKEN_SMALL_UNIT);
            let user = match LUCKYCODE.with(|r| r.borrow().get(&prize.0)) {
                Some(user) => user,
                None => continue,
            };

            info.3 = refund;
            info.5 = now_sec;
            PRIZE_INFO.with(|r| r.borrow_mut().insert(prize.clone(), info));
            PRIZE_LOG.with(|r| {
                let mut m = r.borrow_mut();
                let mut logs = m
                    .get(&prize.0)
                    .unwrap_or_else(|| PrizeClaimLogs(Vec::new()));
                if logs.0.len() >= 1024 {
                    logs.0 = logs.0.split_off(512);
                }
                logs.0.push((prize.clone(), now_sec, refund));
                m.insert(prize.0, logs);
            });

            let _ = airdrop::deposit(user, refund);
        }
    }

    pub fn clear_failed(prize: &Prize) {
        PRIZE_INFO.with(|r| r.borrow_mut().remove(prize));
        PRIZE_REC.with(|r| r.borrow_mut().remove(prize));
    }

    pub fn claim(
        user: u32,
        prize: Prize,
        luck: u64,
        now_sec: u64,
        rand: u64,
    ) -> Result<(u64, u64), String> {
        let mut info = PRIZE_INFO.with(|r| match r.borrow().get(&prize) {
            Some(info) => {
                if info.4 >= prize.4 {
                    return Err("the prize have been fully claimed".to_string());
                }
                if info.5 > 0 {
                    return Err("the prize has already ended".to_string());
                }
                Ok(info)
            }
            None => Err("the prize is not found".to_string()),
        })?;

        let (amount, filled, avg) = PRIZE_REC.with(|r| {
            let mut m = r.borrow_mut();
            let mut recipients = m
                .get(&prize)
                .unwrap_or_else(|| PrizeRecipients(BTreeMap::new()));
            if recipients.0.contains_key(&user) {
                return Err("you have already claimed the prize".to_string());
            }
            if prize.4 as usize <= recipients.0.len() {
                return Err("the prize have been fully claimed".to_string());
            }

            let remain = prize.4 - recipients.0.len() as u16;
            let claimed: u64 = recipients.0.values().map(|v| *v as u64).sum();
            let balance = prize.3 as u64 * TOKEN_SMALL_UNIT - claimed;
            let avg = balance / remain as u64;
            let amount = if info.0 == 0 || remain == 1 || rand == 0 {
                avg
            } else {
                luck_amount(luck, avg, remain as u64)
            };

            let filled = prize.4 + 1 - remain;
            recipients.0.insert(user, amount as u32);
            m.insert(prize.clone(), recipients);
            Ok((
                amount * (TOKEN_1 / TOKEN_SMALL_UNIT),
                filled,
                avg * (TOKEN_1 / TOKEN_SMALL_UNIT),
            ))
        })?;

        info.4 = filled;
        if filled >= prize.4 {
            info.5 = now_sec;
        }
        PRIZE_INFO.with(|r| r.borrow_mut().insert(prize.clone(), info));
        PRIZE_LOG.with(|r| {
            let mut m = r.borrow_mut();
            let mut logs = m.get(&user).unwrap_or_else(|| PrizeClaimLogs(Vec::new()));
            if logs.0.len() >= 1024 {
                logs.0 = logs.0.split_off(512);
            }
            logs.0.push((prize, now_sec, amount));
            m.insert(user, logs);
        });
        Ok((amount, avg))
    }

    pub fn try_add_airdrop(
        issuer: u32,
        now_sec: u64,
        expire: u16,
        claimable: u32, // 0 for airdrop
        quantity: u16,
    ) -> Option<String> {
        if quantity == 0 || claimable > 0 {
            return None;
        }

        let prize = Prize(issuer, (now_sec / 60) as u32, expire, claimable, quantity);
        let ok = PRIZE.with(|r| {
            let mut m = r.borrow_mut();
            if m.contains_key(&prize) {
                return false;
            }
            m.insert(prize.clone(), Principals(BTreeSet::new()));
            true
        });
        if ok {
            ISSUER_PRIZE.with(|r| {
                let mut m = r.borrow_mut();
                let mut prizes = m
                    .get(&issuer)
                    .unwrap_or_else(|| IssuerPrizes(BTreeMap::new()));
                prizes.0.insert((prize.1, prize.2, prize.3, prize.4), 0);
                m.insert(issuer, prizes);
            });
            Some(prize.encode(&(*keys::AIRDROP_KEY), None))
        } else {
            None
        }
    }

    pub fn claim_airdrop(user: Principal, prize: Prize) -> Result<u64, String> {
        PRIZE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&prize) {
                Some(mut users) => {
                    if users.0.len() >= prize.4 as usize {
                        return Err("the airdrop code have been fully claimed".to_string());
                    }
                    if users.0.contains(&user) {
                        return Err("you have claimed".to_string());
                    }
                    users.0.insert(user);
                    m.insert(prize.clone(), users);
                    Ok(())
                }
                None => Err("airdrop code is not found".to_string()),
            }
        })?;
        ISSUER_PRIZE.with(|r| {
            let mut m = r.borrow_mut();
            let mut prizes = m
                .get(&prize.0)
                .unwrap_or_else(|| IssuerPrizes(BTreeMap::new()));
            prizes
                .0
                .entry((prize.1, prize.2, prize.3, prize.4))
                .and_modify(|curr| *curr += 1)
                .or_insert(1);
            m.insert(prize.0, prizes);
        });
        Ok(prize.3 as u64 * TOKEN_1 / prize.4 as u64)
    }

    pub fn list_airdrop(issuer: u32) -> IssuerPrizes {
        ISSUER_PRIZE.with(|r| {
            r.borrow()
                .get(&issuer)
                .unwrap_or(IssuerPrizes(BTreeMap::new()))
        })
    }

    pub fn claim_logs(user: u32, prev: Option<u64>, take: usize) -> Vec<types::PrizeClaimLog> {
        PRIZE_LOG.with(|r| match r.borrow().get(&user) {
            None => vec![],
            Some(log_store) => {
                let latest = log_store.0.len() as u64;
                if latest == 0 {
                    return vec![];
                }

                let prev = prev.unwrap_or(latest);
                if prev > latest || prev == 0 {
                    return vec![];
                }

                let mut idx = prev - 1;
                let mut logs: Vec<types::PrizeClaimLog> = Vec::with_capacity(take);
                while let Some((prize, claimed_at, amount)) = log_store.0.get(idx as usize) {
                    if let Some(info) = PRIZE_INFO.with(|r| r.borrow().get(prize)) {
                        let name = naming::get(&prize.0).map(|n| n.0);
                        logs.push(types::PrizeClaimLog {
                            prize: types::PrizeOutput::from(prize, &info, name, None),
                            claimed_at: *claimed_at,
                            amount: Nat::from(*amount),
                        })
                    }
                    if idx == 0 || logs.len() >= take {
                        break;
                    }
                    idx -= 1;
                }

                logs
            }
        })
    }

    pub fn issue_logs(issuer: u32, prev_ts: u32) -> Vec<types::PrizeOutput> {
        let key = *keys::PRIZE_KEY;
        let name = naming::get(&issuer).map(|n| n.0);
        PRIZE_INFO.with(|r| {
            let du = 60 * 24 * 30;
            let start_ts = if prev_ts > du { prev_ts - du } else { 0 };
            let mut logs: Vec<types::PrizeOutput> = Vec::new();
            for (prize, info) in r
                .borrow()
                .range(Prize::range_bounds_of(issuer, start_ts, prev_ts))
            {
                logs.push(types::PrizeOutput::from(
                    &prize,
                    &info,
                    name.clone(),
                    if prize.4 > 1 {
                        Some(prize.encode(&key, None))
                    } else {
                        // can't re-generate code for directional prize.
                        None
                    },
                ));
            }
            logs.reverse();
            logs
        })
    }
}

pub mod user {
    use super::*;

    // add a user to the active user set, return true if the user is not in the set
    pub fn active(user: Principal) -> bool {
        ACTIVE_USERS.with(|r| r.borrow_mut().insert(user))
    }

    pub fn deactive(user: Principal) {
        ACTIVE_USERS.with(|r| r.borrow_mut().remove(&user));
    }
}

pub mod naming {
    use super::*;

    pub fn get(code: &u32) -> Option<NamingState> {
        NAMING_STATE.with(|r| r.borrow().get(code))
    }

    pub fn get_by_name(name: &str) -> Option<(u32, NamingState)> {
        if let Some(code) = NAMING.with(|r| r.borrow().get(&name.to_lowercase())) {
            return NAMING_STATE.with(|r| r.borrow().get(&code).map(|v| (code, v)));
        }
        None
    }

    pub fn try_set_name(code: u32, name: NamingState) -> bool {
        NAMING.with(|r| {
            let mut m = r.borrow_mut();
            let ln = name.0.to_lowercase();
            if m.contains_key(&ln) {
                return false;
            }

            m.insert(ln, code);
            NAMING_STATE.with(|r| r.borrow_mut().insert(code, name));
            true
        })
    }

    pub fn try_update_name(code: u32, old: &str, name: NamingState) -> bool {
        NAMING.with(|r| {
            let mut m = r.borrow_mut();
            let ln = name.0.to_lowercase();
            let lon = old.to_lowercase();
            if m.get(&lon) != Some(code) {
                return false;
            }

            if ln != lon {
                if m.contains_key(&ln) {
                    return false;
                }

                m.remove(&lon);
                m.insert(ln, code);
            }
            NAMING_STATE.with(|r| r.borrow_mut().insert(code, name));
            true
        })
    }

    pub fn remove_name(code: u32, name: &str) -> bool {
        NAMING.with(|r| {
            let mut m = r.borrow_mut();
            let ln = name.to_lowercase();
            if m.get(&ln) != Some(code) {
                return false;
            }

            m.remove(&ln);
            NAMING_STATE.with(|r| r.borrow_mut().remove(&code));
            true
        })
    }
}

pub mod state {
    use super::*;

    pub fn is_manager(caller: &Principal) -> bool {
        STATE_HEAP.with(|r| {
            r.borrow()
                .managers
                .as_ref()
                .map(|ms| ms.contains(caller))
                .unwrap_or(false)
        })
    }

    pub fn airdrop_amount_balance() -> (u64, u64) {
        STATE_HEAP.with(|r| {
            let s = r.borrow();
            // default to 100 PANDA tokens
            (s.airdrop_amount.unwrap_or(100), s.airdrop_balance)
        })
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE_HEAP.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE_HEAP.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn next_lucky_code() -> u32 {
        STATE_HEAP.with(|r| {
            let mut s = r.borrow_mut();
            let code = s.lucky_code.unwrap_or(2000000).saturating_add(1);
            s.lucky_code = Some(code);
            code
        })
    }

    pub fn load() {
        STATE.with(|r| {
            let mut s = r.borrow().get().clone();
            if s.lucky_code.is_none() {
                s.lucky_code =
                    Some(LUCKYCODE.with(|r| (r.borrow().len() as u32).saturating_add(1000000)));
            }
            STATE_HEAP.with(|h| {
                *h.borrow_mut() = s;
            });
        });
    }

    pub fn save() {
        STATE_HEAP.with(|h| {
            STATE.with(|r| {
                r.borrow_mut()
                    .set(h.borrow().clone())
                    .expect("failed to set STATE data");
            });
        });
    }
}

pub mod notification {
    use super::*;

    pub fn list() -> Vec<types::Notification> {
        NOTIFICATIONS.with(|r| r.borrow().values().cloned().collect())
    }

    pub fn add(arg: types::Notification) {
        NOTIFICATIONS.with(|r| {
            r.borrow_mut().insert(arg.id, arg);
        });
    }

    pub fn remove(ids: Vec<u8>) {
        NOTIFICATIONS.with(|r| {
            r.borrow_mut().retain(|k, _| !ids.contains(k));
        });
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::ops::RangeBounds;

    const PRIZE_TEST_MEMORY_ID: MemoryId = MemoryId::new(249);
    thread_local! {
        static PRIZE_TEST: RefCell<StableBTreeMap<Prize, u64, Memory>> = RefCell::new(
            StableBTreeMap::init(
                MEMORY_MANAGER.with_borrow(|m| m.get(PRIZE_TEST_MEMORY_ID)),
            )
        );
    }

    #[test]
    fn test_bound_max_size() {
        let principal =
            Principal::from_text("i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe")
                .unwrap();
        let v = SysPrizeSubsidy(u64::MAX, u16::MAX, u32::MAX, u8::MAX, u32::MAX, u16::MAX);
        let v = v.to_bytes();
        println!(
            "SysPrizeSubsidy max_size: {:?}, {}",
            v.len(),
            hex::encode(&v)
        );

        let v = AirdropState(u32::MAX, u64::MAX, u64::MAX);
        let v = v.to_bytes();
        println!("AirdropState max_size: {:?}, {}", v.len(), hex::encode(&v));

        let v = AirdropLog(principal, u64::MAX, u64::MAX, u32::MAX);
        let v = v.to_bytes();
        println!("AirdropLog max_size: {:?}, {}", v.len(), hex::encode(&v));

        let v = LuckyDrawLog(principal, u64::MAX, u64::MAX, u64::MAX, u64::MAX);
        let v = v.to_bytes();
        println!("LuckyDrawLog max_size: {:?}, {}", v.len(), hex::encode(&v));

        let v = Prize(u32::MAX, u32::MAX, u16::MAX, u32::MAX, u16::MAX);
        let v = v.to_bytes();
        println!("Prize max_size: {:?}, {}", v.len(), hex::encode(&v));

        let v = PrizeRefund(
            u32::MAX,
            Prize(u32::MAX, u32::MAX, u16::MAX, u32::MAX, u16::MAX),
        );
        let v = v.to_bytes();
        println!("PrizeRefund max_size: {:?}, {}", v.len(), hex::encode(&v));

        let v = NamingState("N".repeat(64), u64::MAX, u32::MAX, u32::MAX);
        let v = v.to_bytes();
        println!("NamingState max_size: {:?}, {}", v.len(), hex::encode(&v));
    }

    #[test]
    fn test_prize_range_bounds_of() {
        let p1 = Prize(1, 1, 0, 0, 0);
        let p2 = Prize(1, 2, 0, 0, 0);
        assert!(p1 < p2);

        let p1 = Prize(2, 1, 0, 0, 0);
        let p2 = Prize(1, 10, 0, 0, 0);
        assert!(p1 > p2);

        let rb = Prize::range_bounds_of(10000, 0, u32::MAX);
        assert_eq!(
            rb.start_bound(),
            ops::Bound::Excluded(&Prize(10000, 0, 0, 0, 0))
        );
        assert_eq!(
            rb.end_bound(),
            ops::Bound::Excluded(&Prize(10000, u32::MAX, 0, 0, 0))
        );
        assert!(rb.contains(&Prize(10000, 1, 0, 0, 0)));
        assert!(rb.contains(&Prize(10000, 1, 1, 0, 0)));
        assert!(rb.contains(&Prize(10000, u32::MAX - 1, 0, 0, 0)));
        assert!(!rb.contains(&Prize(10001, 1, 1, 0, 0)));

        PRIZE_TEST.with(|r| {
            let mut m = r.borrow_mut();
            m.clear_new();
            m.insert(Prize(10000, 1, 0, 0, 0), 1);
            m.insert(Prize(10000, 2, 0, 0, 0), 2);
            m.insert(Prize(10001, 1, 0, 0, 0), 3);
            m.insert(Prize(10000, 3, 0, 0, 0), 4);
            m.insert(Prize(999, 1, 0, 0, 0), 5);
            m.insert(Prize(10001, 2, 0, 0, 0), 6);
            m.insert(Prize(10000, 2, 1, 0, 0), 7);

            let vs: Vec<u64> = m.iter().map(|(_, v)| v).collect();
            assert_eq!(vs, vec![5, 1, 2, 7, 4, 3, 6]);

            let vs: Vec<u64> = m
                .range(Prize::range_bounds_of(10000, 0, u32::MAX))
                .map(|(_, v)| v)
                .collect();
            assert_eq!(vs, vec![1, 2, 7, 4]);
            m.clear_new();
        });
    }
}
