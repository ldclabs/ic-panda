#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=90:nat; evidence=blob "\94\4f\8a\92\f8\40\d9\02\4f\1e\f1\c3\43\49\a3\ba\f2\d8\4c\82\4a\05\76\86\31\4d\73\2e\3e\39\d7\21"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.12\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.12.\n\nRemove the outdated unaudited notices from the landing page and multisig wallet demo, along with their strings in all 23 language catalogs and unused styles.\n\nUpdate the English and Chinese product overviews to link to the public security audit report with its findings, fixes and verification results. Retain the documented trust boundaries and the distinction between public source and deployed code.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
