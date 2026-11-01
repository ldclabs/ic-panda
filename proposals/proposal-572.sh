#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=85:nat; evidence=blob "\b1\41\ce\c8\62\49\58\69\f8\e1\40\90\9a\28\ed\65\54\d0\82\18\6c\44\19\fb\fe\51\63\75\bf\b9\3f\5f"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.7\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.7.\n\n1. feat: add a product guide readable before JavaScript starts, standalone English and Chinese reading pages, Markdown guides and a localized Documentation link in the footer, while preserving the existing SPA fallback for login, profile and legacy links;\n2. docs: publish selected public architecture, protocol, CLI and SDK document snapshots, llms.txt, llms-full.txt, structured data, robots.txt and a sitemap to help search engines and agents discover public information. Include a build manifest with the source revision and content hashes;\n3. build: share membership figures between the landing page and guides, configure content types and cache revalidation for public documents, validate generated assets during the build, and preview the actual distribution output;\n4. fix: support certified account snapshot schema 5 with an explicit recovery_enabled flag in the public profile reader, while retaining schema 4 compatibility and rejecting malformed snapshots;\n5. privacy: update all 23 localized privacy policies to explain notification filtering metadata, recovery reminders, retention and cleanup, and account linking through a shared iOS notification token, including consent before enabling notifications for another account.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
