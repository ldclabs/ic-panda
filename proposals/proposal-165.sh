#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Upgrade ICPanda SNS to next available version";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposals";
        summary = "Upgrade ICPanda SNS to next available version on SNS-W";
        action = opt variant {
            UpgradeSnsToNextVersion = record {}
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
