use candid::{
    utils::{encode_args, ArgumentEncoder},
    CandidType, Decode, Principal,
};
use ic_agent::{Agent, Identity};

use super::format_error;

pub async fn build_agent(host: &str, identity: Box<dyn Identity>) -> Result<Agent, String> {
    let agent = Agent::builder()
        .with_url(host)
        .with_boxed_identity(identity)
        .with_verify_query_signatures(true)
        .build()
        .map_err(format_error)?;
    if host.starts_with("http://") {
        agent.fetch_root_key().await.map_err(format_error)?;
    }

    Ok(agent)
}

#[allow(dead_code)]
pub async fn update_call<In, Out>(
    agent: &Agent,
    canister_id: &Principal,
    method_name: &str,
    args: In,
) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: CandidType + for<'a> candid::Deserialize<'a>,
{
    let input = encode_args(args).map_err(format_error)?;
    let res = agent
        .update(canister_id, method_name)
        .with_arg(input)
        .call_and_wait()
        .await
        .map_err(format_error)?;
    let output = Decode!(res.as_slice(), Out).map_err(format_error)?;
    Ok(output)
}

pub async fn query_call<In, Out>(
    agent: &Agent,
    canister_id: &Principal,
    method_name: &str,
    args: In,
) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: CandidType + for<'a> candid::Deserialize<'a>,
{
    let input = encode_args(args).map_err(format_error)?;
    let res = agent
        .query(canister_id, method_name)
        .with_arg(input)
        .call()
        .await
        .map_err(format_error)?;
    let output = Decode!(res.as_slice(), Out).map_err(format_error)?;
    Ok(output)
}
