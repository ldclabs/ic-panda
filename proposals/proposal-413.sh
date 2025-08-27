#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Update ICPanda SNS metadata";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai";
        summary = "Update ICPanda SNS description";
        action = opt variant {
            ManageSnsMetadata = record {
                name = opt "ICPanda";
                description = opt "We’re building the foundational infrastructure and applications that empower AI agents to thrive as first-class citizens in the Web3 ecosystem. Anda.AI | dMsg.net";
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
