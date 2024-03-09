use crate::{icp_transfer_to, is_controller, store, DAO_CANISTER};
use candid::Nat;

#[ic_cdk::update(guard = "is_controller")]
fn admin_update_airdrop_balance(airdrop_balance: u64) {
    store::state::with_mut(|state| state.airdrop_balance = airdrop_balance);
}

#[ic_cdk::update(guard = "is_controller")]
async fn admin_collect_icp(amount: Nat) -> Result<(), String> {
    icp_transfer_to(*DAO_CANISTER, amount)
        .await
        .map_err(|err| format!("failed to collect ICP, {}", err))?;
    Ok(())
}
