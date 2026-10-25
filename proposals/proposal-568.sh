#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=82:nat; evidence=blob "\72\ae\70\37\29\d4\36\73\e9\45\66\40\90\05\f6\9b\28\d2\8e\cb\63\dd\9e\27\6c\57\bc\14\7c\94\6b\b8"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.4\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.4.\n\n1. feat: open encrypted vault snapshots in the browser share page using dmsg-secret-package/1; keep field values hidden until revealed, allow copying revealed values, show notes on demand and save individual attachments;\n2. improve: validate the complete snapshot and attachment SHA-256 digests before displaying content; reject duplicate JSON keys, unsupported fields, invalid sizes, truncated data and trailing bytes, and sanitize suggested attachment filenames;\n3. fix: prevent duplicate downloads and late results from updating a closed share page; clear temporary plaintext buffers after download and clear the key and retained attachment bytes when the page closes;\n4. improve: explain that browser receipt and desktop import use separate download sessions, that importing creates an independent copy, and that revoking a link cannot delete saved copies;\n5. i18n: translate the snapshot controls and guidance into all six supported languages, clarify in the agent demo that concealed values remain masked and credential use needs separate approval, and standardize the Chinese vault name as 保险库.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
