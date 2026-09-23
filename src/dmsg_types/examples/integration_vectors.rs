//! Fixed development fixtures; no real identities, balances or rate policy.
use dmsg_protocol::{canonical, integration::*};
use dmsg_types::integration::*;
#[path = "../tests/support/integration.rs"]
mod fixtures;
mod support;
use fixtures::*;
use support::vector;

fn main() {
    let q = quote_panda(&offer(), &app(), &product(), &rate(), NOW).unwrap();
    let values = vec![
        vector("app", canonical(&app())),
        vector("product", canonical(&product())),
        vector(
            "project_offer",
            commitment_bytes("dmsg/commerce/offer/v2", &offer()),
        ),
        vector(
            "account_offer",
            commitment_bytes("dmsg/commerce/offer/v2", &account_offer()),
        ),
        vector(
            "authentication",
            commitment_bytes("dmsg/authentication/request/v1", &authentication()),
        ),
        vector(
            "approval",
            commitment_bytes("dmsg/application/approval/v1", &approval()),
        ),
        vector(
            "panda_quote",
            commitment_bytes("dmsg/commerce/panda-quote/v2", &q),
        ),
        vector(
            "cash_a",
            commitment_bytes("dmsg/commerce/cash-quote/v2", &cash(principal(6))),
        ),
        vector(
            "cash_b",
            commitment_bytes("dmsg/commerce/cash-quote/v2", &cash(principal(7))),
        ),
        vector(
            "decision",
            commitment_bytes("dmsg/commerce/decision/v2", &decision()),
        ),
        vector("max_u128", canonical(&u128::MAX)),
        vector("commerce_version", canonical(&COMMERCE_VERSION)),
    ];
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
