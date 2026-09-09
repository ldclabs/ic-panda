#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register IC-TEE Identity Canister";
        url = "https://github.com/ldclabs/ic-tee/tree/main/src/ic_tee_identity";
        summary = "ic_tee_identity is an on-chain authentication service for Trusted Execution Environments (TEEs) on the Internet Computer.";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "e7tgb-6aaaa-aaaap-akqfa-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json