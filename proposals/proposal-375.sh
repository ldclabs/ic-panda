#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Deregister ic_panda_infra canister";
        url = "https://dashboard.internetcomputer.org/canister/f75us-gyaaa-aaaap-ab3wq-cai";
        summary = "Deregister ic_panda_infra canister (f75us-gyaaa-aaaap-ab3wq-cai), which is a empty canister.";
        action = opt variant {
            DeregisterDappCanisters = record {
                canister_ids = vec {principal "f75us-gyaaa-aaaap-ab3wq-cai"};
                new_controllers = vec { principal "i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json