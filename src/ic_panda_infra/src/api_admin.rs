use candid::Principal;
use ic_cose_types::validate_principals;
use std::collections::BTreeSet;

use crate::{is_controller, is_controller_or_manager, store, types};

#[ic_cdk::update(guard = "is_controller")]
fn admin_add_managers(mut args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.managers.append(&mut args);
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller")]
fn admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    store::state::with_mut(|r| {
        r.managers.retain(|p| !args.contains(p));
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_set_ips_setup_fee(fee: u64) -> Result<(), String> {
    store::state::with_mut(|r| {
        r.ips_setup_fee = fee;
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
async fn admin_set_ips_discount(discount: u16) -> Result<(), String> {
    if discount > 100 {
        Err("discount should be in range [0, 100]".to_string())?;
    }

    store::state::with_mut(|r| {
        r.ips_discount = discount;
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_add_ips_canisters(args: Vec<types::ServiceInfo>) -> Result<(), String> {
    store::state::with_mut(|r| {
        args.into_iter().for_each(|svc| {
            r.ips_canisters.insert(svc.id, (svc.subnet, svc.nodes));
        });
        Ok(())
    })
}

#[ic_cdk::update(guard = "is_controller_or_manager")]
fn admin_remove_ips_canisters(args: BTreeSet<Principal>) -> Result<(), String> {
    store::state::with_mut(|r| {
        args.iter().for_each(|id| {
            r.ips_canisters.remove(id);
        });
        Ok(())
    })
}

#[ic_cdk::update]
fn validate_admin_add_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}

#[ic_cdk::update]
fn validate_admin_remove_managers(args: BTreeSet<Principal>) -> Result<(), String> {
    validate_principals(&args)?;
    Ok(())
}
