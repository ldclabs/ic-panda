#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=86:nat; evidence=blob "\6d\83\9f\8a\36\8f\cc\5e\68\5e\c5\40\04\68\3f\46\20\0d\23\a7\08\46\0c\f2\a3\2d\db\54\0b\86\e4\a1"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.8\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.8.\n\n1. update: rename Team wallets to Multisig wallets across the landing page and privacy resources in all 23 locales, and update the English and Chinese product guides. Clarify personal or shared asset management, approval thresholds and device co-signing;\n2. update: show the per-file cloud backup limit in membership comparisons: 100 MiB for Free, Plus and Pro, and 1000 MiB for Max. Keep the localized landing page and generated HTML and Markdown reading tables aligned, with build checks for the published limits and responsive layouts for long translations and right-to-left languages.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
