#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

ic_cdk::export_candid!();
