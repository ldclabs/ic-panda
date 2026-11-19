#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=96:nat; evidence=blob "\ee\c3\5f\fa\66\af\7b\be\8a\7d\19\d3\45\c7\e2\0c\8a\60\ed\0f\96\bd\44\01\bc\35\f0\31\61\7c\c2\2e"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.18\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal calls commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to publish batch 96 of the dMsg website (dmsg_frontend v3.4.18).\n\nThe sign-in page now accepts the Android app's exact HTTPS callback URL. If the callback opens in a browser, the page clears the sign-in fragment from the address bar and shows localized instructions to restart sign-in in the app. For iOS, the site lists both the previous and new Apple team identifiers for sign-in links and shared web credentials during the app transfer.\n\nThe updated privacy notice explains how the app encrypts exported vault backups on the device with a separate password, how a chosen storage provider may retain the encrypted file, and how Password AutoFill stores logins and remembers accounts for an app or website.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
