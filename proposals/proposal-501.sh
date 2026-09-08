#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=86:nat; evidence=blob "\d0\a4\1a\39\f3\bb\f7\b3\2a\c4\ab\b2\09\ac\b2\e1\8b\cc\7f\c8\43\24\56\b8\e4\dd\87\73\10\4b\4a\62"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v3.1.2\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v3.1.2.\n\n1. feat: add English, Chinese, Russian, Arabic, French and Spanish translations with persistent language selection and right-to-left layout support;\n2. feat: update the dMsg project section to introduce its Chrome workspace, encrypted vault, private channels and identity signing;\n3. fix: improve language switching, mobile layouts and localized amount formatting while preserving transaction values.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json