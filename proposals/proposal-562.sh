#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=75:nat; evidence=blob "\2d\83\c2\f0\d2\36\ce\60\8a\bb\57\b7\2f\0d\01\bb\13\99\42\5b\3c\77\4f\ab\40\55\5b\68\c3\2a\77\1d"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.3.1\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.3.1.\n\n1. fix: distinguish reserved legacy handles from missing accounts on public profile pages; after verifying that no active handle mapping exists, show the reservation status and a link to the read-only legacy profile, with messages in all six languages;\n2. fix: allow encrypted legacy migration exports to include cached channels and messages by encoding IndexedDB ArrayBuffer keys as byte strings, resolving the Unsupported observation type error; add regression coverage for Local, ECDH and VetKey export, encrypted transfer and offline recovery while keeping the original browser database unchanged.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
