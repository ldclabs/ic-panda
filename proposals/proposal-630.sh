#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=91:nat; evidence=blob "\8b\15\b1\c0\26\4c\cf\17\9b\1e\13\f0\fd\e0\32\f0\9d\55\75\3f\72\00\4e\56\2a\ed\df\ee\12\e0\0e\15"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.13\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.13.\n\nAdd dmsg-passkey/1 support to the browser login page for Passkey registration, login and confirmation of sign-out from all login sessions, while retaining Internet Identity. Validate requests and return WebAuthn results only to allowlisted application callbacks.\n\nCheck secure context and browser WebAuthn capabilities without excluding external security keys, require discoverable credentials for registration, and provide localized guidance and a return-to-app action in all 23 languages. Distinguish incomplete authentication from explicit cancellation and timeout without exposing raw browser errors.\n\nThis proposal publishes website assets only; Passkey service deployment and real-device acceptance are separate release requirements.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
