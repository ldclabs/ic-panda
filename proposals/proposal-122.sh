#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=53:nat; evidence=blob "\23\7e\3a\23\1b\14\b4\a2\95\b5\b9\6e\22\b8\74\40\4a\1c\3c\40\2d\fd\66\4c\74\41\06\91\9f\6a\75\2c"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v2.2.4\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v2.2.4.\n\n1. fix: fetching latest message.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

quill send proposal-message.json