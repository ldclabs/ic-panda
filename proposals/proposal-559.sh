#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=72:nat; evidence=blob "\ad\fa\1d\9a\60\2d\5d\81\79\bc\fe\bd\4c\b3\19\a2\9d\32\48\47\8d\94\c9\c8\0d\e8\2f\85\f3\6c\c8\25"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.2.0\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.2.0.\n\n1. feat: redraw the hero figure as the dMsg gatehouse: AI agents, websites, teammates and the terminal ask on the left, the vault, wallets, messages and files sit under the dome, and a payment, sign-in or signature stops at the approval window on the right; the share image follows;\n2. feat: add Files and Team wallets sections to the landing page with two new figures, the encrypted file library beside what the cloud holds and an agent's wallet proposal decided under the wallet's policy with its audit chain, and an Available now section whose download buttons and install commands follow the latest desktop release;\n3. feat: add the share link page at /s/<share_id>, which decrypts a shared file in the browser with the key in the link or a password, says why a link is gone (expired, revoked, out of downloads or missing), and resumes a broken download without counting another;\n4. feat: let the dmsg.net login page grant the desktop app logins of up to 30 days instead of 8 hours, for Touch ID fast unlock;\n5. fix: rebuild the /legacy migration panels as numbered steps with accessible radio, select and checkbox controls, translated in all six languages;\n6. fix: ship the Apache-2.0 license of Remix Icon 4.6.0, the release whose icons the site uses.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
