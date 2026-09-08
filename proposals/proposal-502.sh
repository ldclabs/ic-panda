#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=69:nat; evidence=blob "\a1\ea\49\fd\ca\2c\65\9c\68\8d\c3\18\31\44\3c\e7\6d\dd\d9\6b\81\ac\6c\ea\2d\cf\a4\71\d2\a6\14\c8"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.0.0\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.0.0.\n\n1. feat: redesign the landing page around the dMsg Chrome workspace and preserve access to the read-only legacy message archive;\n2. feat: add English, Chinese, Russian, Arabic, French and Spanish translations with persistent language selection and right-to-left layout support;\n3. fix: use the id.ai authorization endpoint and simplify Internet Identity sign-in for OAuth deep links;\n4. fix: improve landing navigation, page metadata, mobile layouts and localized amount formatting;\n5. fix: keep dialog drafts independent and refresh wallet and file-preview state correctly when inputs change.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
