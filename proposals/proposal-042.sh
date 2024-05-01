#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal '(
    record {
        title = "Add ICDex Trader withdraw() method";
        url = "https://github.com/iclighthouse/ICDex-Trader/blob/main/docs/Guide_for_SNS_treasury.md";
        summary = "Add ICDex Trader withdraw() method for sending funds from Trader canister back to SNS Treasury through SNS proposal.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_004 : nat64;
                name = "Add ICDex Trader withdraw() method";
                description = opt "Add ICDex Trader withdraw() method.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        validator_canister_id = opt principal "6sul7-xqaaa-aaaap-ahdsq-cai";
                        validator_method_name = opt "validate_withdraw";
                        target_canister_id = opt principal "6sul7-xqaaa-aaaap-ahdsq-cai";
                        target_method_name = opt "withdraw";
                    }
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json