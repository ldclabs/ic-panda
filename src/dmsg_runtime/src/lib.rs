#![doc = include_str!("../README.md")]
//! Implementation support for the reference ICP canisters. Not a wire protocol.
mod budget;
mod certified;
pub mod ledger;
pub mod stable_types;
pub mod storage;
pub use budget::Budget;
pub use certified::Certification;
use dmsg_types::*;
use serde::de::DeserializeOwned;

pub const WINDOW: usize = 64;

pub async fn call<In, Out>(id: candid::Principal, method: &str, args: In) -> Result<Out>
where
    In: candid::utils::ArgumentEncoder + Send,
    Out: candid::CandidType + DeserializeOwned,
{
    let response = ic_cdk::call::Call::bounded_wait(id, method)
        .with_args(&args)
        .await
        .map_err(|_| Error::ExecutionUnknown)?;
    response.candid().map_err(|_| Error::ExecutionUnknown)
}
