#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=70:nat; evidence=blob "\c0\d2\41\db\df\b0\db\3b\67\41\67\c1\e7\5f\74\11\d2\20\9b\b8\3b\ee\7a\ee\76\ff\f1\7e\ef\37\dc\67"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.1.0\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.1.0.\n\n1. feat: rebuild the landing page around the dMsg desktop app, the dmsg command line and AI agents: one local dmsg daemon holds the keys and the approval queue, agents reach it through MCP with capabilities the owner grants, and websites ask through connect.dmsg.net without a browser extension;\n2. feat: add five interactive figures, each a simulation with sample data: the approval gate, a desktop window and a terminal sharing one daemon, an MCP agent run with its audit record, the dmsg-connect/1 sequence, and a privacy demo that encrypts the visitor's text in the browser to show what the cloud holds;\n3. feat: introduce the Gate mark with new favicon, touch, PWA and share images, and remove the retired Chrome extension wording;\n4. feat: add the dmsg.net sign-in page for the desktop app, and Privacy Policy, Terms of Service and Support pages with a published terms digest;\n5. feat: let /legacy pair with the desktop app to read and hand over legacy messages and shared channels, localized in all six languages;\n6. fix: pause animations off screen and in hidden tabs, show still scenes when reduced motion is requested, and refine Arabic typography.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
