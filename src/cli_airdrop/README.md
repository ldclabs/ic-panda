# ICPanda DAO Airdrop Snapshot Tool
This is a tool for generating airdrop snapshots based on Proposal 108 and 184:

- https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/108
- https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/184

## Usage

Syncing PANDA blocks:
```bash
cargo run -p cli_airdrop -- sync --store ./panda_blocks
```

Snapshotting PANDA neurons:
```bash
cargo run -p cli_airdrop -- neurons
# neurons: 1582, airdrop neurons: 968, total: 342.36M (34236399835527016), time elapsed: Ok(11.797192s)
```

It will generate files:
- neurons list: `neurons_[TIMESTAMP].cbor.[HASH]`
- neurons airdrop snapshot: `neurons_airdrops_[TIMESTAMP].cbor.[HASH]`
- snapshot detail: `neurons_logs_[TIMESTAMP].txt`

Snapshotting PANDA accounts:
```bash
cargo run -p cli_airdrop -- ledger --store ./panda_blocks
# block: 299119, airdrop accounts: 261, total: 63.65M (6365383174070470), time elapsed: Ok(3.817676s)
```

It will generate files:
- ledger airdrop snapshot: `ledger_airdrops_[TIMESTAMP].cbor.[HASH]`
- snapshot detail: `ledger_logs_[TIMESTAMP].txt`