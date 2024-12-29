#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function to \"Set managers for the Lucky Pool canister\"";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/ic_panda_luckypool/src/api_admin.rs#L35";
        summary = "Adding a new generic function that manage the addition, modification, and deletion of app notifications, and update the balance for airdrop.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_001 : nat64;
                name = "Set managers for the Lucky Pool canister";
                description = opt "Managers can currently manage the addition, modification, and deletion of app notifications, and can update the balance for airdrop.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                        target_canister_id = opt principal "a7cug-2qaaa-aaaap-ab3la-cai";
                        validator_method_name = opt "validate_admin_set_managers";
                        target_method_name = opt "admin_set_managers";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json