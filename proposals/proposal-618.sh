#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=87:nat; evidence=blob "\e9\b5\16\08\1f\ad\fb\f5\ec\f3\f8\cd\1d\4b\8e\ee\b6\7b\b7\7f\24\4c\d7\3a\1d\aa\d5\27\df\88\2e\ab"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.9\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.9.\n\nFix legacy migration falsely reporting OSS ciphertext hash mismatch for valid avatars and attachments. Verify the original OSS source hashes with SHA3-256, matching the historical uploader, while preserving SHA-256 for migration archive digests and rejecting corrupted files. Regression tests cover Local, ECDH and VetKey archives, multipart files, altered content and hashes, and older objects without a source hash.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
