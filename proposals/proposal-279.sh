#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Update ICPanda SNS metadata";
        url = "https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai";
        summary = "Update ICPanda SNS name and description";
        action = opt variant {
            ManageSnsMetadata = record {
                name = opt "ICPanda";
                description = opt "A technical panda fully running on the Internet Computer blockchain, building chain-native infrastructures and practical Web3 apps. About us: https://dmsg.net/panda";
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
