#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Remove the generic function \"admin_set_managers\"";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/ic_panda_luckypool/src/api_admin.rs";
        summary = "Remove the generic function \"admin_set_managers\" on ic_panda_luckypool (a7cug-2qaaa-aaaap-ab3la-cai), which validator_method_name is wrong and can not be executed.";
        action = opt variant {
            RemoveGenericNervousSystemFunction = 1_001
        };
    }
)' > proposal-message.json

# quill send proposal-message.json