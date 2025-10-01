#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register IC Delegation Store Canister";
        url = "https://github.com/ldclabs/ic-panda/tree/main/src/ic_delegation_store";
        summary = "ic_delegation_store is used to relay the delegation information for II authentication.";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "asxpf-ciaaa-aaaap-an33a-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json