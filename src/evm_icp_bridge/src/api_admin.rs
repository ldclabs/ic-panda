use alloy::primitives::Address;
use candid::{pretty::candid::value::pp_value, CandidType, IDLValue};
use url::Url;

use crate::store;

#[ic_cdk::update(guard = "is_controller")]
async fn admin_add_evm_contract(
    chain_name: String,
    chain_id: u64,
    address: String,
    decimals: u8,
) -> Result<(), String> {
    let address = check_admin_add_evm_contract(&chain_name, chain_id, &address, decimals)?;
    let cli = store::state::evm_client(&chain_name);
    let now_ms = ic_cdk::api::time() / 1_000_000;
    let (cid, gas_price, block_number, symbol, dec) = futures::future::try_join5(
        cli.chain_id(now_ms),
        cli.gas_price(now_ms),
        cli.finalized_block_number(now_ms),
        cli.erc20_symbol(now_ms, &address),
        cli.erc20_decimals(now_ms, &address),
    )
    .await?;

    if chain_id != cid {
        return Err(format!(
            "chain_id mismatch, got {}, expected {}",
            cid, chain_id
        ));
    }
    if decimals != dec {
        return Err(format!(
            "decimals mismatch, got {}, expected {}",
            dec, decimals
        ));
    }

    store::state::with_mut(|s| {
        if s.token_symbol != symbol {
            return Err(format!(
                "token_symbol mismatch, got {}, expected {}",
                symbol, s.token_symbol
            ));
        }

        s.evm_token_contracts
            .insert(chain_name.clone(), (address, decimals, chain_id));
        s.evm_finalized_block
            .insert(chain_name, (block_number, gas_price));
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_add_evm_contract(
    chain_name: String,
    chain_id: u64,
    address: String,
    decimals: u8,
) -> Result<String, String> {
    check_admin_add_evm_contract(&chain_name, chain_id, &address, decimals)?;
    pretty_format(&(chain_name, chain_id, address, decimals))
}

fn check_admin_add_evm_contract(
    chain_name: &str,
    chain_id: u64,
    address: &str,
    decimals: u8,
) -> Result<Address, String> {
    if chain_name.trim().to_ascii_uppercase() != chain_name
        || chain_name.is_empty()
        || chain_name.len() > 8
    {
        return Err("chain_name must be non-empty, up to 8 chars, and all uppercase".to_string());
    }

    let addr = Address::parse_checksummed(address, Some(chain_id))
        .map_err(|err| format!("invalid address: {err:?}"))?;

    store::state::with(|s| {
        if decimals < s.token_decimals {
            return Err(format!(
                "decimals must be >= {}, got {}",
                s.token_decimals, decimals
            ));
        }

        if s.evm_token_contracts.contains_key(chain_name) {
            return Err("chain_id already exists".to_string());
        }

        if s.evm_token_contracts
            .values()
            .any(|(_, _, cid)| *cid == chain_id)
        {
            return Err("chain_id already exists".to_string());
        }
        Ok(())
    })?;
    Ok(addr)
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_set_evm_providers(chain_name: String, providers: Vec<String>) -> Result<(), String> {
    for url in &providers {
        let v = Url::parse(url).map_err(|err| format!("invalid url {url}, error: {err}"))?;
        if v.scheme() != "https" {
            return Err(format!("url scheme must be https, got: {url}"));
        }
    }

    store::state::with_mut(|s| {
        s.evm_providers.insert(chain_name, providers);
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_set_evm_providers(
    chain_name: u64,
    providers: Vec<String>,
) -> Result<String, String> {
    for url in &providers {
        let v = Url::parse(url).map_err(|err| format!("invalid url {url}, error: {err}"))?;
        if v.scheme() != "https" {
            return Err(format!("url scheme must be https, got: {url}"));
        }
    }
    pretty_format(&(chain_name, providers))
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if ic_cdk::api::is_controller(&caller)
        || store::state::with(|s| s.governance_canister == Some(caller))
    {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn pretty_format<T>(data: &T) -> Result<String, String>
where
    T: CandidType,
{
    let val = IDLValue::try_from_candid_type(data).map_err(|err| format!("{err:?}"))?;
    let doc = pp_value(7, &val);

    Ok(format!("{}", doc.pretty(120)))
}
