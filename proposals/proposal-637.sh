#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=94:nat; evidence=blob "\17\a9\73\ac\ef\30\bb\2a\37\a9\48\ac\98\23\18\34\06\fd\c7\a2\5b\62\e4\ef\ce\97\58\01\7a\2b\e9\dd"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.16\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.16.\n\nRework dmsg.net around a private workspace for the AI era: encrypted passwords, notes and files, vault sync across authorized devices, private file sharing and collection, and scoped AI agent access. Clarify explicit file backups, plan limits, recovery prerequisites and ICPanda DAO governance across all 23 supported locales. Refresh the public reading pages, machine-readable guides, structured data and social images.\n\nShow the full filename and type on shared-file pages by decrypting metadata in the browser without consuming a download. Add online text and Markdown reading, raster image previews and full-screen viewing after complete integrity verification. Saving after reading reuses the verified content and the same download session. Keep password protection, block active Markdown content and automatic external image requests, and introduce file sharing and collection together, with Plus eligibility for creating file request links.\n\nThe public share metadata endpoint must be deployed before this website update. Local frontend checks, tests, production build and generated-content checks passed; two environment-gated live tests were skipped. Live R2 and file-request integration were not verified. This proposal commits the prepared website asset batch only; desktop and iOS binaries are released separately.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
