use ciborium::{from_reader, into_writer};
use getrandom::register_custom_getrandom;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use rand::{rngs::StdRng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, time::Duration};

use crate::{ai, types};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub chat_count: u64,
    pub ai_config: u32,
    pub ai_tokenizer: u32,
    pub ai_model: u32,
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

#[derive(Default)]
pub struct AIModel {
    pub config: Vec<u8>,
    pub tokenizer: Vec<u8>,
    pub model: Vec<u8>,
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const FS_METADATA_MEMORY_ID: MemoryId = MemoryId::new(1);
const FS_DATA_MEMORY_ID: MemoryId = MemoryId::new(2);

thread_local! {
    static RNG: RefCell<Option<StdRng>> = const { RefCell::new(None) };
    static STATE: RefCell<State> = RefCell::new(State::default());

    static AI_MODEL: RefCell<Option<AIModel>> = const { RefCell::new(None) };

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE_STORE")
    );

    // `FS_CHUNKS_STORE`` is needed by `ic_oss_can::ic_oss_fs` macro
    static FS_CHUNKS_STORE: RefCell<StableBTreeMap<ic_oss_can::types::FileId, ic_oss_can::types::Chunk, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(FS_DATA_MEMORY_ID)),
        )
    );
}

ic_oss_can::ic_oss_fs!();

async fn set_rand() {
    let (rr,) = ic_cdk::api::management_canister::main::raw_rand()
        .await
        .expect("failed to get random bytes");
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&rr);
    RNG.with(|rng| {
        *rng.borrow_mut() = Some(StdRng::from_seed(seed));
    });
}

fn custom_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    RNG.with(|rng| rng.borrow_mut().as_mut().unwrap().fill_bytes(buf));
    Ok(())
}

pub fn init_rand() {
    ic_cdk_timers::set_timer(Duration::from_secs(0), || ic_cdk::spawn(set_rand()));
    register_custom_getrandom!(custom_getrandom);
}

pub fn load_model(args: &types::LoadModelInput) -> Result<(), String> {
    AI_MODEL.with(|r| {
        // let start = ic_cdk::api::performance_counter(1);
        *r.borrow_mut() = Some(AIModel {
            config: fs::get_full_chunks(args.config_id)?,
            tokenizer: fs::get_full_chunks(args.tokenizer_id)?,
            model: fs::get_full_chunks(args.model_id)?,
        });
        // ic_cdk::println!(
        //     "load_model_instructions: {}",
        //     ic_cdk::api::performance_counter(1) - start
        // );
        Ok(())
    })
}

pub fn run_ai<W>(args: &ai::Args, prompt: &str, sample_len: usize, w: &mut W) -> Result<u32, String>
where
    W: std::io::Write,
{
    AI_MODEL.with(|r| match r.borrow().as_ref() {
        None => Err("AI model not loaded".to_string()),
        Some(m) => {
            let mut ai = ai::TextGeneration::load(args, &m.config, &m.tokenizer, &m.model)
                .map_err(|err| format!("{:?}", err))?;
            ai.run(prompt, sample_len, w)
                .map_err(|err| format!("{:?}", err))
        }
    })
}

pub mod state {
    use super::*;

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn load() -> State {
        STATE_STORE.with(|r| {
            let s = r.borrow().get().clone();
            STATE.with(|h| {
                *h.borrow_mut() = s.clone();
            });
            s
        })
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                r.borrow_mut()
                    .set(h.borrow().clone())
                    .expect("failed to set STATE_STORE data");
            });
        });
    }
}
