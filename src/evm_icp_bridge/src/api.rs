use alloy::{
    eips::Encodable2718,
    primitives::{Address, Bytes},
};

use crate::{helper::msg_caller, store};

#[ic_cdk::query]
fn info() -> Result<store::StateInfo, String> {
    Ok(store::state::info())
}

#[ic_cdk::query]
fn my_evm_address() -> Result<String, String> {
    let caller = msg_caller()?;
    let addr = store::state::evm_address(&caller);
    Ok(addr.to_string())
}

#[ic_cdk::query]
async fn my_pending_logs() -> Result<Vec<store::BridgeLog>, String> {
    let caller = msg_caller()?;
    let rt = store::state::with(|s| {
        s.pending
            .iter()
            .filter_map(|item| {
                if item.user == caller {
                    Some(item.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<store::BridgeLog>>()
    });
    Ok(rt)
}

#[ic_cdk::query]
async fn my_finalized_logs(
    prev: Option<u64>,
    take: Option<u64>,
) -> Result<Vec<store::BridgeLog>, String> {
    let caller = msg_caller()?;
    let take = take.unwrap_or(10).min(1000) as usize;
    let rt = store::state::logs(caller, prev, take);
    Ok(rt)
}

#[ic_cdk::update]
async fn bridge(
    from_chain: String,
    to_chain: String,
    icp_amount: u128,
) -> Result<store::BridgeTx, String> {
    let caller = msg_caller()?;
    let now_ms = ic_cdk::api::time() / 1_000_000;
    store::state::bridge(from_chain, to_chain, icp_amount, caller, now_ms).await
}

#[ic_cdk::update]
async fn erc20_transfer_tx(chain: String, to: String, icp_amount: u128) -> Result<String, String> {
    let to_addr = to
        .parse::<Address>()
        .map_err(|err| format!("invalid to address: {}", err))?;
    let caller = msg_caller()?;
    let now_ms = ic_cdk::api::time() / 1_000_000;
    let (_, signed_tx) =
        store::state::build_erc20_transfer_tx(&chain, &caller, &to_addr, icp_amount, now_ms)
            .await?;
    let data = signed_tx.encoded_2718();
    Ok(Bytes::from(data).to_string())
}

#[ic_cdk::update]
async fn erc20_transfer(chain: String, to: String, icp_amount: u128) -> Result<String, String> {
    let to_addr = to
        .parse::<Address>()
        .map_err(|err| format!("invalid to address: {}", err))?;
    let caller = msg_caller()?;
    let now_ms = ic_cdk::api::time() / 1_000_000;
    let (cli, signed_tx) =
        store::state::build_erc20_transfer_tx(&chain, &caller, &to_addr, icp_amount, now_ms)
            .await?;
    let tx_hash = signed_tx.hash().to_string();

    let data = signed_tx.encoded_2718();
    let _ = cli
        .send_raw_transaction(now_ms, Bytes::from(data).to_string())
        .await?;

    Ok(tx_hash)
}
