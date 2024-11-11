use candid::{Nat, Principal};
use ic_agent::Agent;
use ic_message_types::profile::UserInfo;
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};

use super::{
    agent::{query_call, update_call},
    format_error,
};

#[derive(Clone)]
pub struct DMsgAgent {
    pub agent: Agent,
    pub canister_id: Principal,
}

impl DMsgAgent {
    pub async fn get_user_by_username(&self, username: &str) -> Result<UserInfo, String> {
        let output: Result<UserInfo, String> = query_call(
            &self.agent,
            &self.canister_id,
            "get_by_username",
            (username,),
        )
        .await?;

        output
    }

    pub async fn send_token_to(
        &self,
        token_canister: &Principal,
        to: Principal,
        amount_e8s: u64,
    ) -> Result<Nat, String> {
        let output: Result<Nat, TransferError> = update_call(
            &self.agent,
            &token_canister,
            "icrc1_transfer",
            (TransferArg {
                from_subaccount: None,
                to: Account {
                    owner: to,
                    subaccount: None,
                },
                fee: None,
                created_at_time: None,
                memo: None,
                amount: amount_e8s.into(),
            },),
        )
        .await?;
        output.map_err(format_error)
    }
}
