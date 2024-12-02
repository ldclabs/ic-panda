# Deployment Guide

## Running the project locally

If you want to test your project locally, you can use the following commands:

```bash
# Starts the replica
dfx start

# Creates the canisters with the specified IDs
dfx canister create --specified-id rdmx6-jaaaa-aaaaa-aaadq-cai internet_identity
dfx canister create --specified-id ryjl3-tyaaa-aaaaa-aaaba-cai icp_ledger_canister
dfx canister create --specified-id c63a7-6yaaa-aaaap-ab3gq-cai ic_panda_frontend
dfx canister create --specified-id a7cug-2qaaa-aaaap-ab3la-cai ic_panda_luckypool
dfx canister create --specified-id nscli-qiaaa-aaaaj-qa4pa-cai ic_message
dfx canister create --specified-id nvdn4-5qaaa-aaaaj-qa4pq-cai ic_message_channel
dfx canister create --specified-id ijyxz-wyaaa-aaaaj-qa4qa-cai ic_message_profile
dfx canister create --specified-id 2fvu6-tqaaa-aaaap-akksa-cai ic_message_frontend
dfx canister create --specified-id n3bau-gaaaa-aaaaj-qa4oq-cai ic_cose_canister
dfx canister create --specified-id 5szpn-tiaaa-aaaaj-qncoq-cai ic_oss_cluster
dfx canister create --specified-id 532er-faaaa-aaaaj-qncpa-cai ic_oss_bucket

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

# TODO: Add the following command to configure the dMsg canisters
```

Open the frontend in your default browser

http://2fvu6-tqaaa-aaaap-akksa-cai.localhost:4943/