//! Read-only release check for raw Candid captured from a pinned ledger's ICRC-3 query.
//! Prints block-format evidence only; no payer or recipient addresses are exported.
use icrc_ledger_types::icrc3::blocks::GetBlocksResult;
fn main() {
    let path = std::env::args().nth(1).expect("raw Candid hex file");
    let text = std::fs::read_to_string(path).expect("read query response");
    let text = text.trim();
    assert!(text.len().is_multiple_of(2), "hex length");
    let bytes: Vec<u8> = text
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).expect("hex response")
        })
        .collect();
    let reply: GetBlocksResult = candid::decode_one(&bytes).expect("ICRC-3 response");
    let mut checked = 0;
    for block in reply.blocks {
        let id = dmsg_runtime::ledger::block_index(block.id).expect("supported index");
        if let Ok(tx) = dmsg_runtime::ledger::parse_transfer(id, &block.block) {
            println!("{{\"block_index\":{id},\"fee_atomic\":{},\"sender_timestamp_present\":{},\"memo_bytes\":{}}}",tx.fee.map_or("null".into(),|f|f.to_string()),tx.created_at_time.is_some(),tx.memo.as_ref().map_or(0,Vec::len));
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "no supported transfer in this window; query another bounded window"
    );
}
