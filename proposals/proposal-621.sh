#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=88:nat; evidence=blob "\90\c6\d1\35\08\55\23\cb\d7\44\16\fc\9a\be\f3\a3\ca\72\c6\97\fb\ed\ff\af\4a\80\d4\1e\33\a8\6a\75"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.10\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.10.\n\nAdd a public file-request page where people can check the requester's public profile and upload files without an account or app. Files are encrypted in the browser for the requester; the upload link does not grant access to received files. Returning in the same browser shows upload history and allows replacing or withdrawing files before the requester saves them.\n\nStream large uploads in bounded chunks, preserve upload reservations across lost replies, and cancel active requests before cleanup. Display file limits consistently in MiB/GiB. Improve share-link pages with clear account-free download guidance and device-appropriate dMsg installation links, including on expired or revoked links. Localize the new workflows in all 23 supported languages.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
