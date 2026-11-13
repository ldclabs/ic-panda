#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=95:nat; evidence=blob "\79\4b\73\bd\46\59\d8\97\c2\82\e3\3a\88\f1\97\48\44\e4\01\d9\f4\4a\ed\87\8f\16\6d\ae\31\c1\69\dd"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.17\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.17.\n\nShow a sharer profile only when the link creator explicitly opts in. Existing links remain anonymous. For opted-in links, visitors can open the public profile before entering a file password. Load the certified profile independently so delayed or failed profile reads do not block file metadata, reading or download. The profile identifies the sharer, not the file author or a safety endorsement.\n\nRender the file-sharing and file-request invitation immediately while link data loads. Fetch the share descriptor and metadata authorization concurrently, show public size, expiry and download counts as soon as available, and reveal the filename and type only after manifest verification and decryption. Reuse metadata authorization and renew expired grants after password entry without consuming a download session.\n\nDeploy service support for the optional profile choice before updated clients, and retain the public metadata endpoint required by the sharing page. This proposal commits only the prepared website asset batch; desktop/CLI 1.5.6 and iOS 1.2.0 (build 30) are released separately. Production deployment and live service integration are not verified by this release-note update.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
