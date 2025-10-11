use crate::{helper::msg_caller, store};

#[ic_cdk::query]
fn info() -> Result<store::StateInfo, String> {
    Ok(store::state::info())
}

#[ic_cdk::query]
fn evm_address() -> Result<String, String> {
    let caller = msg_caller()?;
    let addr = store::state::evm_address(&caller);
    Ok(addr.to_string())
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
