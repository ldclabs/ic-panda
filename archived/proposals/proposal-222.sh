#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Remove the generic function \"grant_permission\"";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/10";
        summary = "Remove the generic function \"grant_permission\" on ic_panda_frontend (c63a7-6yaaa-aaaap-ab3gq-cai), which is not used.";
        action = opt variant {
            RemoveGenericNervousSystemFunction = 1_003
        };
    }
)' > proposal-message.json

# quill send proposal-message.json