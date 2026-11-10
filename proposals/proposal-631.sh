#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=92:nat; evidence=blob "\ca\25\c5\8f\2a\66\c7\d8\e3\f5\42\2f\18\67\c2\c5\32\9d\2d\0a\d0\0b\c9\4b\73\91\34\c8\c5\ff\7f\a8"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.14\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.14.\n\nImprove public search and AI-reader discovery with stable section anchors, plain-text descriptions, page-specific Open Graph metadata and an x-default language alternative for the English and Chinese reading pages. Align homepage canonical and Open Graph URLs with the sitemap, and exclude file-request routes from crawler discovery.\n\nPublish the IndexNow host-verification file with explicit plain-text and revalidation headers. Add an operator-run URL notification helper that previews by default and verifies the live key and page contents against the deployed build before submitting canonical sitemap URLs. Build and test do not send notifications, and successful submission does not guarantee indexing.\n\nExtend generated-content checks and document the release workflow and remaining SPA routing limitations. This proposal publishes website assets only; Bing Webmaster site verification, URL submission and search-index acceptance remain separate operational steps.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
