#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_message_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=17:nat; evidence=blob "\e5\41\df\d6\dd\b7\79\d0\2f\7b\5c\48\67\10\89\1d\39\13\d5\d0\cb\d6\2d\98\6e\26\c6\71\9e\c4\d0\f8"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_message_frontend v2.5.3\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release ic_message_frontend v2.5.2.\n\n1. fix: fix profile page.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json