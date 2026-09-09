#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Register the frontend asset canister";
        url = "https://nns.ic0.app/proposal/?u=d7wvo-iiaaa-aaaaq-aacsq-cai&proposal=12";
        summary = "In order to grant Commit permissions for the Governance canister, we deregistered the frontend asset canister in Proposal #11. Now that the permission granted, we can re-registered this asset canister to the SNS.\n\nRefer: https://nns.ic0.app/proposal/?u=d7wvo-iiaaa-aaaaq-aacsq-cai&proposal=11";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {principal "c63a7-6yaaa-aaaap-ab3gq-cai"};
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json