use candid::{candid_method, Principal};

#[ic_cdk::query]
#[candid_method]
fn whoami() -> Result<Principal, ()> {
    Ok(ic_cdk::caller())
}

ic_cdk::export_candid!();
