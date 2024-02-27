use candid::Principal;

#[ic_cdk::query]
fn whoami() -> Principal {
    ic_cdk::caller()
}

ic_cdk::export_candid!();
