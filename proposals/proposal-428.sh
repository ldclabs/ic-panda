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
                description = opt "Breathing life into sovereign AI. We are building the open-source stack for agents to remember, transact, and evolve as first-class citizens in Web3. https://Anda.AI | https://dMsg.net";
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
