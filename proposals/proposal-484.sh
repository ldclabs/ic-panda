#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Revise ICPanda SNS description";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai";
        summary = "Revise ICPanda SNS description";
        action = opt variant {
            ManageSnsMetadata = record {
                description = opt "An on-chain Builder DAO creating open infrastructure for digital sovereignty. Sovereign Minds: https://anda.ai | Sovereign Markets: https://tokenlist.ing";
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
