#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register ic_name_identity canister";
        url = "https://github.com/ldclabs/ic-oss";
        summary = "DMSG canisters: ic_name_identity.\n\nRefer: https://github.com/ldclabs/ic-panda/blob/main/canister_ids.json";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "2rgax-kyaaa-aaaap-anvba-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json