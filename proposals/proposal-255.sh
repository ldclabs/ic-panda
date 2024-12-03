#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register DMSG canisters";
        url = "https://github.com/ldclabs/ic-oss";
        summary = "DMSG canisters: ic_dmsg_minter, dmsg_ledger_canister, dmsg_index_canister.\n\nRefer: https://github.com/ldclabs/ic-panda/blob/main/canister_ids.json";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "ql553-iqaaa-aaaap-anuyq-cai"; principal "ocqzv-tyaaa-aaaar-qal4a-cai"; principal "qc6wh-6yaaa-aaaap-anuza-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json