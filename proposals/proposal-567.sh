#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=81:nat; evidence=blob "\cd\ee\f9\d2\94\a5\98\55\27\84\71\5f\d2\fa\25\3e\bb\1d\98\ff\b0\e6\31\2e\b2\de\40\57\eb\a4\60\45"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.3\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.3.\n\n1. design: rebuild the landing page hero as the dMsg gatehouse at work: the vault, files, channels, team wallets and the agent's Agent ID sit inside it; you, a teammate, your agent and a website reach it through the gate, and the agent's signed letters leave for Agent Mail, Discourse and Knowledge;\n2. feat: requests wait in a working approval window: approving takes 1.2 seconds, a payment needs a held press, and an ignored request expires after 15 seconds; the scene stops when paused, offscreen or in a hidden tab, switches to a narrow layout below 560px and stays still with reduced motion;\n3. feat: add a desktop app section showing the main window with its sidebar groups, order and shortcuts, a gate rail that locks, each view and six everyday details; cover one-time passwords and clipboard clearing, Inbox contact rules and the signature verifier;\n4. design: render the social cards from the same scene with a waiting request and remove the gradient and shadow from the mark; sections reveal once as they arrive, the header marks the section being read, the story shows each step's time, and arrows mirror in Arabic;\n5. improve: align the Chinese vault term and the Arabic Inbox name with the desktop app, and remove unused strings and CSS;\n6. dependency: update the shared SDK to agent-protocols 0.12.6.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
