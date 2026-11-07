#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=89:nat; evidence=blob "\86\e1\ba\5a\3a\18\1a\86\fe\88\e7\1e\a1\ed\42\c5\ac\30\fe\26\1d\d1\08\9e\17\8e\c8\77\35\1a\0f\0f"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.11\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.11.\n\nUpdate the Terms of Service to clarify that App Store Plus, Pro and Max subscriptions renew automatically each year. Explain subscription management and cancellation, Apple billing and refund handling, and restoring purchases with the original Apple Account and the same dMsg account. Distinguish these rules from wallet payments and manual renewals, and make clear that deleting the app or dMsg account does not cancel an Apple subscription.\n\nUpdate the English terms, their downloadable text and SHA-256 digest, and all 22 translations. Replace the claim that every purchase records the current terms digest with instructions for verifying the published English text.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
