use candid::Principal;
use ciborium::from_reader;
use ic_agent::{
    hash_tree::{Label, LookupResult},
    Agent,
};
use ic_certification::{
    hash_tree::{HashTreeNode, SubtreeLookupResult},
    Certificate, HashTree,
};
use icrc_ledger_types::icrc3::archive::{GetArchivesArgs, GetArchivesResult};
use icrc_ledger_types::icrc3::blocks::{DataCertificate, GetBlocksRequest, GetBlocksResult};

use super::{agent::query_call, format_error};

#[derive(Clone)]
pub struct Icrc1Agent {
    pub agent: Agent,
    pub canister_id: Principal,
}

impl Icrc1Agent {
    pub async fn icrc3_get_blocks(
        &self,
        args: Vec<GetBlocksRequest>,
    ) -> Result<GetBlocksResult, String> {
        let output =
            query_call(&self.agent, &self.canister_id, "icrc3_get_blocks", (args,)).await?;

        Ok(output)
    }

    pub async fn icrc3_get_blocks_in(
        &self,
        canister_id: &Principal,
        args: Vec<GetBlocksRequest>,
    ) -> Result<GetBlocksResult, String> {
        let output = query_call(&self.agent, canister_id, "icrc3_get_blocks", (args,)).await?;

        Ok(output)
    }

    pub async fn icrc3_get_archives(
        &self,
        args: GetArchivesArgs,
    ) -> Result<GetArchivesResult, String> {
        let output = query_call(
            &self.agent,
            &self.canister_id,
            "icrc3_get_archives",
            (args,),
        )
        .await?;

        Ok(output)
    }

    pub async fn icrc3_get_tip_certificate(&self) -> Result<DataCertificate, String> {
        let output: Option<DataCertificate> = query_call(
            &self.agent,
            &self.canister_id,
            "icrc3_get_tip_certificate",
            (),
        )
        .await?;

        output.ok_or_else(|| "DataCertificate not found".to_string())
    }

    pub async fn verify_root_hash(
        &self,
        certificate: &Certificate,
        root_hash: &[u8; 32],
    ) -> Result<(), String> {
        self.agent
            .verify(certificate, self.canister_id)
            .map_err(format_error)?;

        let certified_data_path: [Label<Vec<u8>>; 3] = [
            "canister".into(),
            self.canister_id.as_slice().into(),
            "certified_data".into(),
        ];

        let cert_hash = match certificate.tree.lookup_path(&certified_data_path) {
            LookupResult::Found(v) => v,
            _ => {
                return Err(format!(
                    "could not find certified_data for canister: {}",
                    self.canister_id
                ))
            }
        };

        if cert_hash != root_hash {
            return Err("certified_data does not match the root_hash".to_string());
        }
        Ok(())
    }

    pub async fn get_certified_chain_tip(&self) -> Result<([u8; 32], u64), String> {
        let DataCertificate {
            certificate,
            hash_tree,
        } = self.icrc3_get_tip_certificate().await?;

        let certificate: Certificate = if let Some(certificate) = certificate {
            from_reader(certificate.as_slice()).map_err(format_error)?
        } else {
            return Err("Certificate not found in the DataCertificate".to_string());
        };

        let hash_tree: HashTree = from_reader(hash_tree.as_slice()).map_err(format_error)?;
        self.verify_root_hash(&certificate, &hash_tree.digest())
            .await?;

        let last_block_hash_vec = lookup_leaf(&hash_tree, "tip_hash")?;
        if let Some(last_block_hash_vec) = last_block_hash_vec {
            let last_block_hash: [u8; 32] = match last_block_hash_vec.clone().try_into() {
                Ok(last_block_hash) => last_block_hash,
                Err(_) => {
                    return Err(format!(
                "DataCertificate last_block_hash bytes: {}, cannot be decoded as last_block_hash",
                hex::encode(last_block_hash_vec)
            ))
                }
            };

            let last_block_index_vec = lookup_leaf(&hash_tree, "last_block_index")?;
            if let Some(last_block_index_vec) = last_block_index_vec {
                let last_block_index_bytes: [u8; 8] = match last_block_index_vec.clone().try_into()
                {
                    Ok(last_block_index_bytes) => last_block_index_bytes,
                    Err(_) => {
                        return Err(format!(
                    "DataCertificate hash_tree bytes: {}, cannot be decoded as last_block_index",
                    hex::encode(last_block_index_vec)
                ))
                    }
                };
                let last_block_index = u64::from_be_bytes(last_block_index_bytes);

                return Ok((last_block_hash, last_block_index));
            } else {
                return Err(
                    "certified hash_tree contains tip_hash but not last_block_index".to_string(),
                );
            }
        }

        Err("certified hash_tree don't contains tip_hash".to_string())
    }
}

fn lookup_leaf(hash_tree: &HashTree, leaf_name: &str) -> Result<Option<Vec<u8>>, String> {
    match hash_tree.lookup_subtree([leaf_name.as_bytes()]) {
        SubtreeLookupResult::Found(tree) => match tree.as_ref() {
            HashTreeNode::Leaf(result) => Ok(Some(result.clone())),
            _ => Err(format!(
                "`{}` value in the hash_tree should be a leaf",
                leaf_name
            )),
        },
        SubtreeLookupResult::Absent => Ok(None),
        _ => Err(format!(
            "`{}` not found in the response hash_tree",
            leaf_name
        )),
    }
}
