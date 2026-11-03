#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

# The wallet Wasm must already be staged on dmsg_wallet_hub.
# Keep the staging area unchanged until this proposal has executed.
BLOB="$(didc encode --format blob \
    --defs src/dmsg_wallet_hub/dmsg_wallet_hub.did \
    --method admin_register_version \
    '(blob "\f8\cd\ed\27\7a\bc\b6\aa\ec\bf\00\b7\51\c5\b1\a1\8f\17\ba\89\10\fe\6a\e2\7b\0a\38\ab\ba\1b\e8\ae", "canisters-v3.1.5")')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
        title = \"Register dmsg_wallet canisters-v3.1.5 on dmsg_wallet_hub\";
        url = \"https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.5\";
        summary = \"This proposal executes admin_register_version() on dmsg_wallet_hub (ghcje-dyaaa-aaaas-qg7va-cai) through generic function 3037 to register the staged wallet Wasm with release notes canisters-v3.1.5.\n\nModule SHA-256: f8cded277abcb6aaecbf00b751c5b1a18f17ba8910fe6ae27b0a38abba1be8ae.\n\nThe registered version becomes available for new wallets. Existing wallets upgrade only after their members approve the exact version and module hash.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 3037 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
