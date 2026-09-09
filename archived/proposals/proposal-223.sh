#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Remove the generic function \"withdraw\"";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/42";
        summary = "Remove the generic function \"withdraw\" on ICDex Trader (6sul7-xqaaa-aaaap-ahdsq-cai), which is not used.";
        action = opt variant {
            RemoveGenericNervousSystemFunction = 1_004
        };
    }
)' > proposal-message.json

# quill send proposal-message.json