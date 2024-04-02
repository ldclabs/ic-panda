#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function to \"Collect ICP into the Treasury\"";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/ic_panda_luckypool/src/api_admin.rs#L9";
        summary = "Adding a new generic function that transfer the ICP tokens obtained from the Lucky Draw to the ICPanda DAO treasury.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_001 : nat64;
                name = "Collect ICP into the Treasury";
                description = opt "Transfer the ICP tokens obtained from the Lucky Draw to the ICPanda DAO treasury.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                        target_canister_id = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                        validator_method_name = opt "validate_admin_collect_icp";
                        target_method_name = opt "admin_collect_icp";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json