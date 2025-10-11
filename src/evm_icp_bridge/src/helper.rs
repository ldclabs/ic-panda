use candid::{utils::ArgumentEncoder, Principal};

const ANONYMOUS: Principal = Principal::anonymous();
pub fn msg_caller() -> Result<Principal, String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(caller)
    }
}

pub fn convert_amount(
    src_amount: u128,
    src_decimals: u8,
    target_decimals: u8,
) -> Result<u128, String> {
    if src_decimals == target_decimals {
        Ok(src_amount)
    } else if src_decimals < target_decimals {
        let factor = 10u128
            .checked_pow((target_decimals - src_decimals) as u32)
            .ok_or_else(|| "exponent too large".to_string())?;
        src_amount
            .checked_mul(factor)
            .ok_or_else(|| "multiplication overflow".to_string())
    } else {
        let factor = 10u128
            .checked_pow((src_decimals - target_decimals) as u32)
            .ok_or_else(|| "exponent too large".to_string())?;
        Ok(src_amount / factor)
    }
}

pub async fn call<In, Out>(
    id: Principal,
    method: &str,
    args: In,
    cycles: u128,
) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: candid::CandidType + for<'a> candid::Deserialize<'a>,
{
    let res = ic_cdk::call::Call::bounded_wait(id, method)
        .with_args(&args)
        .with_cycles(cycles)
        .await
        .map_err(|err| format!("failed to call {} on {:?}, error: {:?}", method, &id, err))?;
    res.candid().map_err(|err| {
        format!(
            "failed to decode response from {} on {:?}, error: {:?}",
            method, &id, err
        )
    })
}
