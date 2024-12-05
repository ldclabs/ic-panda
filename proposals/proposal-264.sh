#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_message_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=38:nat; evidence=blob "\72\55\bf\1e\42\e8\61\1b\c4\e1\2d\1a\36\8f\3d\a5\24\dd\b7\fc\12\11\4a\ce\3b\2a\11\83\e9\d6\a2\ea"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_message_frontend v2.8.3\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release ic_message_frontend v2.8.3.\n\n1. fix: fix enter key event for sending message.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
