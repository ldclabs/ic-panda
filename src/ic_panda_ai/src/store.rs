use candid::{CandidType, Nat, Principal};
use ciborium::{from_reader, into_writer};
use getrandom::register_custom_getrandom;
use ic_oss_types::{
    crc32_with_initial,
    file::{FileInfo, MAX_CHUNK_SIZE, MAX_FILE_SIZE},
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use rand::{rngs::StdRng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::{borrow::Cow, cell::RefCell, collections::BTreeSet, ops, time::Duration};

use crate::{ai, types};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub chat_count: u64,
    pub file_id: u32,
    pub ai_config: u32,
    pub ai_tokenizer: u32,
    pub ai_model: u32,
    pub managers: BTreeSet<Principal>,
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

// FileId: (file id, chunk id)
// a file is a collection of chunks.
// max chunk size is 64*1024 bytes
#[derive(Clone, Default, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct FileId(pub u32, pub u32);
impl Storable for FileId {
    const BOUND: Bound = Bound::Bounded {
        max_size: 11,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode FileId data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode FileId data")
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct FileMetadata {
    pub parent: u32, // 0: root
    pub name: String,
    pub content_type: String, // MIME types
    pub size: u64,
    pub filled: u64,
    pub created_at: u64, // unix timestamp in milliseconds
    pub updated_at: u64, // unix timestamp in milliseconds
    pub chunks: u32,
    pub status: i8, // -1: archived; 0: readable and writable; 1: readonly
    pub hash: Option<[u8; 32]>,
}

impl Storable for FileMetadata {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode FileMetadata data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode FileMetadata data")
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct FileChunk(pub Vec<u8>);

impl Storable for FileChunk {
    const BOUND: Bound = Bound::Bounded {
        max_size: MAX_CHUNK_SIZE,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Self(bytes.to_vec())
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
    static STATE_HEAP: RefCell<State> = RefCell::new(State::default());

    static AI_MODEL: RefCell<Option<AIModel>> = const { RefCell::new(None) };

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    static FS_METADATA: RefCell<StableBTreeMap<u32, FileMetadata, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(FS_METADATA_MEMORY_ID)),
        )
    );

    static FS_DATA: RefCell<StableBTreeMap<FileId, FileChunk, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(FS_DATA_MEMORY_ID)),
        )
    );
}

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

    pub fn is_manager(caller: &Principal) -> bool {
        STATE_HEAP.with(|r| r.borrow().managers.contains(caller))
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE_HEAP.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE_HEAP.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn load() -> State {
        STATE.with(|r| {
            let s = r.borrow().get().clone();
            STATE_HEAP.with(|h| {
                *h.borrow_mut() = s.clone();
            });
            s
        })
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

pub mod fs {
    use super::*;

    pub fn get_file(id: u32) -> Option<FileMetadata> {
        FS_METADATA.with(|r| r.borrow().get(&id))
    }

    pub fn list_files(prev: u32, take: u32) -> Vec<FileInfo> {
        FS_METADATA.with(|r| {
            let m = r.borrow();
            let mut res = Vec::with_capacity(take as usize);
            let mut id = prev.saturating_sub(1);
            while id > 0 {
                if let Some(meta) = m.get(&id) {
                    res.push(FileInfo {
                        id,
                        parent: meta.parent,
                        name: meta.name,
                        content_type: meta.content_type,
                        size: Nat::from(meta.size),
                        filled: Nat::from(meta.filled),
                        created_at: Nat::from(meta.created_at),
                        updated_at: Nat::from(meta.updated_at),
                        chunks: meta.chunks,
                        hash: meta.hash.map(ByteBuf::from),
                        status: meta.status,
                    });
                    if res.len() >= take as usize {
                        break;
                    }
                }
                id = id.saturating_sub(1);
            }
            res
        })
    }

    pub fn add_file(meta: FileMetadata) -> Result<u32, String> {
        let id = state::with_mut(|s| {
            s.file_id = s.file_id.saturating_add(1);
            s.file_id
        });
        if id == u32::MAX {
            return Err("file id overflow".to_string());
        }

        FS_METADATA.with(|r| r.borrow_mut().insert(id, meta));
        Ok(id)
    }

    pub fn update_file<R>(id: u32, f: impl FnOnce(&mut FileMetadata) -> R) -> Option<R> {
        FS_METADATA.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&id) {
                None => None,
                Some(mut meta) => {
                    let r = f(&mut meta);
                    m.insert(id, meta);
                    Some(r)
                }
            }
        })
    }

    pub fn get_chunk(file_id: u32, chunk_index: u32) -> Option<FileChunk> {
        FS_DATA.with(|r| r.borrow().get(&FileId(file_id, chunk_index)))
    }

    pub fn get_full_chunks(id: u32) -> Result<Vec<u8>, String> {
        let (size, chunks) = FS_METADATA.with(|r| match r.borrow().get(&id) {
            None => Err(format!("file not found: {}", id)),
            Some(meta) => {
                if meta.size != meta.filled {
                    return Err("file not fully uploaded".to_string());
                }
                Ok((meta.size, meta.chunks))
            }
        })?;

        if size > MAX_FILE_SIZE.min(usize::MAX as u64) {
            return Err(format!(
                "file size exceeds limit: {}",
                MAX_FILE_SIZE.min(usize::MAX as u64)
            ));
        }

        FS_DATA.with(|r| {
            let mut filled = 0usize;
            let mut buf = Vec::with_capacity(size as usize);
            if chunks == 0 {
                return Ok(buf);
            }

            for (_, chunk) in r.borrow().range((
                ops::Bound::Included(FileId(id, 0)),
                ops::Bound::Included(FileId(id, chunks - 1)),
            )) {
                filled += chunk.0.len();
                buf.extend_from_slice(&chunk.0);
            }

            if filled as u64 != size {
                return Err(format!(
                    "file size mismatch, expected {}, got {}",
                    size, filled
                ));
            }
            Ok(buf)
        })
    }

    pub fn update_chunk(
        file_id: u32,
        chunk_index: u32,
        now_ms: u64,
        chunk: Vec<u8>,
    ) -> Result<(u32, u32), String> {
        if chunk.is_empty() {
            return Err("empty chunk".to_string());
        }

        if chunk.len() > MAX_CHUNK_SIZE as usize {
            return Err(format!(
                "chunk size too large, max size is {} bytes",
                MAX_CHUNK_SIZE
            ));
        }

        update_file(file_id, |meta| {
            let checksum = crc32_with_initial(chunk_index, &chunk);
            meta.updated_at = now_ms;
            meta.filled += chunk.len() as u64;
            if meta.filled > MAX_FILE_SIZE {
                ic_cdk::trap(&format!("file size exceeds limit: {}", MAX_FILE_SIZE));
            }

            match FS_DATA.with(|r| {
                r.borrow_mut()
                    .insert(FileId(file_id, chunk_index), FileChunk(chunk))
            }) {
                None => {
                    if meta.chunks <= chunk_index {
                        meta.chunks = chunk_index + 1;
                    }
                }
                Some(old) => {
                    meta.filled -= old.0.len() as u64;
                }
            }
            if meta.size < meta.filled {
                meta.size = meta.filled;
            }
            (chunk_index, checksum)
        })
        .ok_or_else(|| format!("file not found: {}", file_id))
    }

    pub fn delete_file(id: u32) -> Result<(), String> {
        let chunks = FS_METADATA.with(|r| {
            if let Some(meta) = r.borrow_mut().remove(&id) {
                return meta.chunks;
            }
            0u32
        });

        FS_DATA.with(|r| {
            for chunk_index in 0..chunks {
                r.borrow_mut().remove(&FileId(id, chunk_index));
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bound_max_size() {
        let v = FileId(u32::MAX, u32::MAX);
        let v = v.to_bytes();
        println!("FileId max_size: {:?}, {}", v.len(), hex::encode(&v));

        let v = FileId(0u32, 0u32);
        let v = v.to_bytes();
        println!("FileId min_size: {:?}, {}", v.len(), hex::encode(&v));
    }

    #[test]
    fn test_fs() {
        assert!(fs::get_file(0).is_none());
        assert!(fs::get_full_chunks(0).is_err());
        assert!(fs::get_full_chunks(1).is_err());

        let f1 = fs::add_file(FileMetadata {
            name: "f1.bin".to_string(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(f1, 1);

        assert!(fs::get_full_chunks(0).is_err());
        let f1_data = fs::get_full_chunks(f1).unwrap();
        assert!(f1_data.is_empty());

        let f1_meta = fs::get_file(f1).unwrap();
        assert_eq!(f1_meta.name, "f1.bin");

        assert!(fs::update_chunk(0, 0, 999, [0u8; 32].to_vec()).is_err());
        let (chunk_1, checksum_1) = fs::update_chunk(f1, 0, 999, [0u8; 32].to_vec()).unwrap();
        let (chunk_2, checksum_2) = fs::update_chunk(f1, 1, 1000, [0u8; 32].to_vec()).unwrap();
        assert_eq!(chunk_1, 0);
        assert_eq!(chunk_2, 1);
        assert_ne!(checksum_1, checksum_2);
        let f1_data = fs::get_full_chunks(f1).unwrap();
        assert_eq!(f1_data, [0u8; 64]);

        let f1_meta = fs::get_file(f1).unwrap();
        assert_eq!(f1_meta.name, "f1.bin");
        assert_eq!(f1_meta.size, 64);
        assert_eq!(f1_meta.filled, 64);
        assert_eq!(f1_meta.chunks, 2);

        let f2 = fs::add_file(FileMetadata {
            name: "f2.bin".to_string(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(f2, 2);
        fs::update_chunk(f2, 0, 999, [0u8; 16].to_vec()).unwrap();
        fs::update_chunk(f2, 1, 1000, [1u8; 16].to_vec()).unwrap();
        fs::update_chunk(f1, 3, 1000, [1u8; 16].to_vec()).unwrap();
        fs::update_chunk(f2, 2, 1000, [2u8; 16].to_vec()).unwrap();
        fs::update_chunk(f1, 2, 1000, [2u8; 16].to_vec()).unwrap();

        let f1_data = fs::get_full_chunks(f1).unwrap();
        assert_eq!(&f1_data[0..64], &[0u8; 64]);
        assert_eq!(&f1_data[64..80], &[2u8; 16]);
        assert_eq!(&f1_data[80..96], &[1u8; 16]);

        let f1_meta = fs::get_file(f1).unwrap();
        assert_eq!(f1_meta.size, 96);
        assert_eq!(f1_meta.filled, 96);
        assert_eq!(f1_meta.chunks, 4);

        let f2_data = fs::get_full_chunks(f2).unwrap();
        assert_eq!(&f2_data[0..16], &[0u8; 16]);
        assert_eq!(&f2_data[16..32], &[1u8; 16]);
        assert_eq!(&f2_data[32..48], &[2u8; 16]);

        let f2_meta = fs::get_file(f2).unwrap();
        assert_eq!(f2_meta.size, 48);
        assert_eq!(f2_meta.filled, 48);
        assert_eq!(f2_meta.chunks, 3);
    }
}
