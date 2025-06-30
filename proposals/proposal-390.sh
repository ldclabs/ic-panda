#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_message_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=60:nat; evidence=blob "\e6\bc\35\8c\55\b2\73\87\87\5a\56\4e\f2\45\7d\73\33\f7\11\92\d5\84\f8\2a\4b\4e\b1\23\ed\88\39\a9"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Release ic_message_frontend v2.12.1 to enable vetKeys\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release ic_message_frontend v2.12.1.\n\n1. fix: auto fetch vetKeys in other devices.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
