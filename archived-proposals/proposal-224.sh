#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Deregister ICDex Trader canister";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/40";
        summary = "Deregister ICDex Trader canister (6sul7-xqaaa-aaaap-ahdsq-cai), which is not used.";
        action = opt variant {
            DeregisterDappCanisters = record {
                canister_ids = vec {principal "6sul7-xqaaa-aaaap-ahdsq-cai"};
                new_controllers = vec { principal "i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json