#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=93:nat; evidence=blob "\c0\0d\d9\42\80\5c\6c\3e\57\aa\91\f5\78\d6\8f\e9\47\b6\d4\d9\98\d6\3f\b1\c1\4d\d4\af\18\3c\63\9d"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.15\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.15.\n\nAnnounce the public App Store release of dMsg for iPhone and iPad. Add App Store download buttons to the homepage and download section using the region-neutral link https://apps.apple.com/app/id6827050680. Update platform availability and installation FAQs across all 23 supported locales; Android remains in development.\n\nPoint download buttons on shared-file and file-request pages to the App Store on iPhone and iPad, including iPadOS browsers using a desktop user agent. Remove the unpublished-app fallback. Update the English and Chinese reading guides, generated Markdown and LLM guides, and SoftwareApplication metadata to include iOS and iPadOS.\n\nLocal type checks, tests, production build and generated-content checks passed. Responsive checks covered all 23 locales at 320, 390 and 1440 CSS pixels, with keyboard and RTL checks. Live share/request integration tests and physical-device installation were not run. This proposal commits the prepared website asset batch only; desktop and iOS binaries are released separately.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
