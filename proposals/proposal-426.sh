#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=78:nat; evidence=blob "\8f\33\d4\cd\88\57\cb\0e\2e\ec\4b\27\b1\c4\09\c6\10\f5\b6\54\f3\04\8f\9e\32\f1\a0\1f\23\24\00\6e"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v2.13.3\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v2.13.3.\n\n1. fix: fix CSP header.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json