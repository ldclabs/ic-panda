#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=58:nat; evidence=blob "\d2\52\00\fc\90\80\c5\c4\85\81\9e\6e\20\1c\20\4a\b8\d6\b5\b7\6a\53\74\65\a2\23\48\d9\0c\4d\7f\b5"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v2.3.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v2.3.0.\n\n1. chore: 1. improve UI for dMsg.net.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

quill send proposal-message.json