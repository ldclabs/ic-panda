#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=80:nat; evidence=blob "\c1\14\ed\e7\be\9c\b0\62\be\26\d9\fc\25\a4\d7\52\de\14\42\c0\4e\7e\2f\90\92\cf\96\10\a8\8f\dd\f6"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.2\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.2.\n\n1. improve: redesign browser signature verification with clearer file selection, readable check results and expandable technical details; link to the signer profile only after the signature, account binding and authorization evidence verify, and mark the original file as unchecked when only its digest is available;\n2. fix: clear stale verification results when either input file changes or the optional original is removed; prevent duplicate submissions and file changes during verification, and keep errors, timestamps and status messages consistent across all six supported languages;\n3. design: unify login, OAuth handoff, encrypted share and legacy identity pages with the public site; refine public profile cards, archive entry pages and responsive layouts;\n4. improve: make policy and help pages easier to read, add a mobile document index and collapse terms verification details while retaining access to the published text, version and SHA-256 digest;\n5. dependency: update the shared SDK to agent-protocols 0.12.5.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
