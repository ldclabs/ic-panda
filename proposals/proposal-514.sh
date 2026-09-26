#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy ic_panda_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=87:nat; evidence=blob "\30\c1\ef\f7\8a\f4\95\18\7c\93\31\e7\32\fe\65\97\97\aa\90\79\c2\e9\de\b5\77\55\83\1a\6d\94\68\c3"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release ic_panda_frontend v3.2.0\";
        url = \"https://panda.fans/\";
        summary = \"This proposal executes commit_proposed_batch() on c63a7-6yaaa-aaaap-ab3gq-cai to release ic_panda_frontend v3.2.0.\n\n1. feat: turn the landing page into a live ink-wash painting of a panda eating bamboo, painted stroke by stroke in the browser and different on every visit; she follows the cursor and tilts her head when poked;\n2. feat: present every section on a sheet of tracing paper over the painting, with a title block that shows the painting's seed and repaints it on click;\n3. feat: add optional background music, a generative guqin and xun piece synthesized live in the browser, off by default;\n4. fix: keep the page readable without WebGL2 and paint the scene at once when reduced motion is requested.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1000 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json