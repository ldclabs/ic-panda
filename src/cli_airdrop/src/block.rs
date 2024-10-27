use candid::CandidType;
use ciborium::from_reader;
use ic_icrc1::{blocks::generic_block_to_encoded_block, Block};
use ic_icrc1_tokens_u64::U64;
use ic_ledger_core::block::{BlockType, EncodedBlock};
use icrc_ledger_types::icrc3::blocks::{GenericBlock, GetBlocksRequest, ICRC3GenericBlock};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::{collections::BTreeMap, path::PathBuf};

use super::{format_error, ledger::Icrc1Agent, nat_to_u64, to_cbor_bytes};

pub type BlockToken64 = Block<U64>;
// 1~100 blocks
pub type Blocks = BTreeMap<u64, BlockToken64>;
const BATCH_LENGTH: u64 = 100;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct TipBlock {
    pub index: u64,
    pub hash: ByteArray<32>,
}

#[derive(Clone)]
pub struct BlocksClient {
    agent: Icrc1Agent,
    store_dir: PathBuf,
}

impl BlocksClient {
    pub fn new(agent: Icrc1Agent, store_dir: &str) -> Self {
        Self {
            agent,
            store_dir: PathBuf::from(store_dir),
        }
    }

    pub async fn start_synching_blocks(&self) -> Result<TipBlock, String> {
        let prev_tip: Option<u64> = match std::fs::read(self.store_dir.join("tip.txt")) {
            Ok(tip) => Some(
                String::from_utf8_lossy(&tip)
                    .trim()
                    .parse()
                    .map_err(format_error)?,
            ),
            Err(_) => None,
        };
        println!("Previous tip: {:?}", prev_tip);
        let latest_tip = self.agent.get_certified_chain_tip().await?;
        let latest_tip = TipBlock {
            index: latest_tip.1,
            hash: latest_tip.0.into(),
        };
        println!("latest_tip: {:?}", latest_tip);

        let (mut start_index, mut parent_hash): (u64, Option<[u8; 32]>) =
            if let Some(prev_tip) = prev_tip {
                let idx = prev_tip / BATCH_LENGTH;

                match std::fs::read(self.store_dir.join(format!("{}.cbor", idx))) {
                    Ok(data) => {
                        let blks: Blocks = from_reader(data.as_slice()).map_err(format_error)?;
                        if prev_tip == (idx + 1) * BATCH_LENGTH - 1 {
                            let blk = blks
                                .get(&prev_tip)
                                .ok_or_else(|| format!("Block {} not found", prev_tip))?;
                            (
                                prev_tip + 1,
                                Some(BlockToken64::block_hash(&blk.clone().encode()).into_bytes()),
                            )
                        } else {
                            let start_index = idx * BATCH_LENGTH;
                            let blk = blks
                                .get(&start_index)
                                .ok_or_else(|| format!("Block {} not found", start_index))?;
                            (start_index, blk.parent_hash.map(|h| h.into_bytes()))
                        }
                    }
                    Err(err) => Err(format_error(err))?,
                }
            } else {
                (0, None)
            };

        loop {
            let tip = self
                .batch_sync_blocks(&latest_tip, start_index, parent_hash)
                .await?;
            println!(
                "Synced up to block {}, hash: {}",
                tip.index,
                hex::encode(tip.hash.as_slice())
            );
            if tip.index == latest_tip.index {
                if tip.hash != latest_tip.hash {
                    return Err(format!(
                        "The latest tip hash does not match, expected: {}, got: {}",
                        hex::encode(latest_tip.hash.as_slice()),
                        hex::encode(tip.hash.as_slice())
                    ));
                }
                return Ok(tip);
            }

            start_index = tip.index + 1;
            parent_hash = Some(*tip.hash);
        }
    }

    pub async fn batch_sync_blocks(
        &self,
        latest_tip_block: &TipBlock,
        start_index: u64,
        mut parent_hash: Option<[u8; 32]>,
    ) -> Result<TipBlock, String> {
        if start_index % BATCH_LENGTH != 0 {
            return Err(format!(
                "The start index {} is not a multiple of the batch length {}",
                start_index, BATCH_LENGTH
            ));
        }

        let idx = start_index / BATCH_LENGTH;
        let end_index = latest_tip_block.index.min((idx + 1) * BATCH_LENGTH - 1);
        let length = end_index + 1 - start_index;
        let mut blocks: Blocks = BTreeMap::new();

        let res = self
            .agent
            .icrc3_get_blocks(vec![GetBlocksRequest {
                start: start_index.into(),
                length: length.into(),
            }])
            .await?;
        for block in res.blocks {
            let id = nat_to_u64(&block.id);
            if id >= start_index && id <= end_index {
                let encoded = icrc3_block_to_encoded_block(block.block)?;
                blocks.insert(id, Block::decode(encoded)?);
            }
        }
        for archive_query in res.archived_blocks {
            let res2 = self
                .agent
                .icrc3_get_blocks_in(&archive_query.callback.canister_id, archive_query.args)
                .await?;
            for block in res2.blocks {
                let id = nat_to_u64(&block.id);
                if id >= start_index && id <= end_index {
                    let encoded = icrc3_block_to_encoded_block(block.block)?;
                    let blk = BlockToken64::decode(encoded);
                    blocks.insert(id, blk?);
                }
            }
        }

        for index in start_index..=end_index {
            let blk = blocks
                .get(&index)
                .ok_or_else(|| format!("Block {} not found in the response from the IC", index))?;

            if blk.parent_hash.map(|h| h.into_bytes()) != parent_hash {
                return Err(format!(
                    "Block {}'s parent hash does not match the previous block's hash",
                    index
                ));
            }
            parent_hash = Some(BlockToken64::block_hash(&blk.clone().encode()).into_bytes());
        }

        let blk = blocks.get(&end_index).unwrap();
        let tip = TipBlock {
            index: end_index,
            hash: BlockToken64::block_hash(&blk.clone().encode())
                .into_bytes()
                .into(),
        };
        std::fs::write(
            self.store_dir.join(format!("{}.cbor", idx)),
            to_cbor_bytes(&blocks),
        )
        .map_err(format_error)?;
        std::fs::write(self.store_dir.join("tip.txt"), tip.index.to_string())
            .map_err(format_error)?;
        Ok(tip)
    }

    pub fn iter(&self) -> Result<BlocksIter<'_>, String> {
        let tip: u64 = match std::fs::read(self.store_dir.join("tip.txt")) {
            Ok(tip) => String::from_utf8_lossy(&tip)
                .trim()
                .parse()
                .map_err(format_error)?,
            Err(err) => Err(format_error(err))?,
        };
        Ok(BlocksIter {
            client: self,
            next_index: 0,
            end: tip + 1,
            blocks: BTreeMap::new(),
        })
    }
}

pub struct BlocksIter<'a> {
    client: &'a BlocksClient,
    next_index: u64,
    end: u64,
    blocks: Blocks,
}

impl<'a> Iterator for BlocksIter<'a> {
    type Item = (u64, BlockToken64);

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.next_index;
        self.next_index += 1;

        if index >= self.end {
            return None;
        }

        if !self.blocks.contains_key(&index) {
            let idx = index / BATCH_LENGTH;
            let blks: Option<Blocks> =
                match std::fs::read(self.client.store_dir.join(format!("{}.cbor", idx))) {
                    Ok(data) => from_reader(data.as_slice()).ok(),
                    Err(_) => None,
                };
            if let Some(blks) = blks {
                self.blocks.extend(blks);
            }
        }

        if let Some(block) = self.blocks.remove(&index) {
            Some((index, block))
        } else {
            None
        }
    }
}

fn icrc3_block_to_encoded_block(block: ICRC3GenericBlock) -> Result<EncodedBlock, String> {
    generic_block_to_encoded_block(GenericBlock::from(block))
}

// #[cfg(test)]
// mod test {
//     use super::*;

//     #[test]
//     fn self_tag() {}
// }
