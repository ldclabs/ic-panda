#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=76:nat; evidence=blob "\96\79\4d\04\07\48\c7\f6\af\12\cd\b2\de\5b\85\0c\fe\b8\90\3e\e8\74\c0\12\7a\6b\46\d1\b9\c5\ee\ce"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.3.2\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.3.2.\n\n1. fix: allow HTTPS connections to dmsg.net subdomains in the site Content Security Policy so public profile data can be loaded from the dMsg API;\n2. fix: support one-hour legacy migration pairing validity and update the guidance in all six languages, giving longer exports more time to complete before the pairing expires;\n3. improve: show a rotating loading indicator on the active inventory or encrypted migration export button, expose its busy state to assistive technology, and keep the stop-reading action available until the operation finishes.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
