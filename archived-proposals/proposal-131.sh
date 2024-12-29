#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register ic_message_frontend canister";
        url = "https://github.com/ldclabs/ic-panda/tree/main/src/ic_message_frontend";
        summary = "Register the canister for the standalone version of ICPanda Message app (https://dmsg.net).\n\nRefer: https://github.com/ldclabs/ic-panda/blob/main/canister_ids.json";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "2fvu6-tqaaa-aaaap-akksa-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json