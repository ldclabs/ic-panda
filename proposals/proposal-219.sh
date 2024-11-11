#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_message_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=29:nat; evidence=blob "\dd\f7\32\7f\0a\6a\82\dc\5e\56\8c\f0\57\eb\39\a3\97\f9\66\45\69\1e\64\73\41\3f\45\57\68\7d\b2\c7"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to revert ic_message_frontend from v2.6.7 to v2.6.6\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release ic_message_frontend v2.6.6.\n\n1. chore: update dependencies for Svelte5.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
