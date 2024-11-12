# ICPanda DAO
🐼 A decentralized Panda meme brand built on the Internet Computer.

## Whitepaper

[English version](./whitepaper/en.md)

[中文版](./whitepaper/zh.md)

## Tokenomics

| Token Name  | Token Symbol | Total Supply (Initial) |
| ----------- | ------------ | ---------------------- |
| **ICPanda** | **PANDA**    | 1,000,000,000          |

### Initial token allocation

| Allocation       | Percentage | Tokens         | Memo                                                                                                                                                    |
| ---------------- | ---------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Development Team | **4%**     | 40,000,000     | Locked for 0～6 months                                                                                                                                  |
| Seed Funders     | **4%**     | 40,000,000     | Locked for 0～3 months                                                                                                                                  |
| SNS Swap         | **12%**    | 120,000,000    | Locked for 0～3 months                                                                                                                                  |
| DAO Treasury     | **80%**    | 800,000,000    |                                                                                                                                                         |
|                  | -- **50%** | -- 500,000,000 | Lucky Pool to All (320,000,000 tokens was [airdropped to holders](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/108)) |
|                  | -- **10%** | -- 100,000,000 | Community Incentive                                                                                                                                     |
|                  | -- **10%** | -- 100,000,000 | CEX Incentive                                                                                                                                           |
|                  | -- **10%** | -- 100,000,000 | DEX Liquidity                                                                                                                                           |


### Token utility

PANDA is the only token issued by ICPanda DAO. By holding PANDA tokens, users can participate in:

1. Governance decisions of ICPanda DAO and receive rewards;
2. Participate in PANDA Prize;
3. Purchasing E2EE messages service;
4. Purchasing other Web3 infrastructure services in the future.

## Contact us

- dMsg: [https://dmsg.net/PANDA](https://dmsg.net/PANDA)
- Web Dapp (Hosted by IC): [https://panda.fans](https://panda.fans)
- OpenChat: [ICPanda DAO Community](https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai)
- Twitter: [https://twitter.com/ICPandaDAO](https://twitter.com/ICPandaDAO)
- GitHub: [https://github.com/ldclabs/ic-panda](https://github.com/ldclabs/ic-panda)

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# Starts the replica
dfx start

# Creates the canisters with the specified IDs
dfx canister create --specified-id rdmx6-jaaaa-aaaaa-aaadq-cai internet_identity
dfx canister create --specified-id ryjl3-tyaaa-aaaaa-aaaba-cai icp_ledger_canister
dfx canister create --specified-id c63a7-6yaaa-aaaap-ab3gq-cai ic_panda_frontend
dfx canister create --specified-id f75us-gyaaa-aaaap-ab3wq-cai ic_panda_infra
dfx canister create --specified-id a7cug-2qaaa-aaaap-ab3la-cai ic_panda_luckypool
dfx canister create --specified-id q5mxo-eyaaa-aaaap-ahfoq-cai ic_panda_ai

# Deploys the ICP Ledger canister with the specified initial values
dfx identity use default
export MINTER_ACCOUNT_ID=$(dfx ledger account-id)
export DEFAULT_ACCOUNT_ID=$(dfx ledger account-id)
dfx deploy --specified-id ryjl3-tyaaa-aaaaa-aaaba-cai icp_ledger_canister --argument "
  (variant {
    Init = record {
      minting_account = \"$MINTER_ACCOUNT_ID\";
      initial_values = vec {
        record {
          \"$DEFAULT_ACCOUNT_ID\";
          record {
            e8s = 21_000_000_000_000_000 : nat64;
          };
        };
      };
      send_whitelist = vec {};
      transfer_fee = opt record {
        e8s = 10_000 : nat64;
      };
      token_symbol = opt \"LICP\";
      token_name = opt \"Local ICP\";
    }
  })
"

# Deploys the ICRC-1 token Ledger canister with the specified initial values
dfx identity use default
export MINTER=$(dfx identity get-principal)
export DEFAULT=$(dfx identity get-principal)
export ARCHIVE_CONTROLLER=$(dfx identity get-principal)
export TOKEN_NAME="ICPanda"
export TOKEN_SYMBOL="PANDA"
export PRE_MINTED_TOKENS=100_000_000_000_000_000
export TRANSFER_FEE=10_000
export TRIGGER_THRESHOLD=2000
export NUM_OF_BLOCK_TO_ARCHIVE=1000
export CYCLE_FOR_ARCHIVE_CREATION=10000000000000
export FEATURE_FLAGS=true

dfx deploy icrc1_ledger_canister --specified-id druyg-tyaaa-aaaaq-aactq-cai --argument "(variant {Init =
record {
     token_symbol = \"${TOKEN_SYMBOL}\";
     token_name = \"${TOKEN_NAME}\";
     minting_account = record { owner = principal \"${MINTER}\" };
     transfer_fee = ${TRANSFER_FEE};
     metadata = vec {};
     feature_flags = opt record{icrc2 = ${FEATURE_FLAGS}};
     initial_balances = vec { record { record { owner = principal \"${DEFAULT}\"; }; ${PRE_MINTED_TOKENS}; }; };
     archive_options = record {
         num_blocks_to_archive = ${NUM_OF_BLOCK_TO_ARCHIVE};
         trigger_threshold = ${TRIGGER_THRESHOLD};
         controller_id = principal \"${ARCHIVE_CONTROLLER}\";
         cycles_for_archive_creation = opt ${CYCLE_FOR_ARCHIVE_CREATION};
     };
 }
})"

# Deploys other canisters
dfx deploy
```

Open the frontend in your default browser

http://c63a7-6yaaa-aaaap-ab3gq-cai.localhost:4943/

## License
Copyright © 2024 [LDC Labs](https://github.com/ldclabs).