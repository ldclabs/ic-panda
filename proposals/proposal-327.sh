#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add a generic function \"transferPosition\" to transfer position in ICPSwap";
        url = "https://github.com/ICPSwap-Labs/icpswap-validators";
        summary = "The `transferPosition` function is used to transfer position in ICPSwap pool 5fq4w-lyaaa-aaaag-qjqta-cai.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 2_000 : nat64;
                name = "`transferPosition` function";
                description = opt "It is used to transfer position in ICPSwap.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        topic = opt variant { ApplicationBusinessLogic };
                        validator_canister_id = opt principal "7i4z4-fyaaa-aaaap-anzsa-cai";
                        target_canister_id = opt principal "5fq4w-lyaaa-aaaag-qjqta-cai";
                        validator_method_name = opt "validate_transfer_position";
                        target_method_name = opt "transferPosition";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json