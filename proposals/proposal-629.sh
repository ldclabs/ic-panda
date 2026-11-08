#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

# The wallet Wasm must already be staged on dmsg_wallet_hub.
# Keep the staging area unchanged until this proposal has executed.
BLOB="$(didc encode --format blob \
    --defs src/dmsg_wallet_hub/dmsg_wallet_hub.did \
    --method admin_register_version \
    '(blob "\c2\95\a9\7c\0d\be\81\4a\66\2d\d2\f2\c8\e5\55\0a\36\bb\96\62\c5\ee\c9\d4\03\c3\d9\e6\d7\8d\9d\46", "canisters-v3.1.8")')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
        title = \"Register dmsg_wallet canisters-v3.1.8 on dmsg_wallet_hub\";
        url = \"https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.8\";
        summary = \"This proposal executes admin_register_version() on dmsg_wallet_hub (ghcje-dyaaa-aaaas-qg7va-cai) through generic function 3037 to register the staged wallet Wasm with release notes canisters-v3.1.8.\n\nModule SHA-256: c295a97c0dbe814a662dd2f2c8e5550a36bb9662c5eec9d403c3d9e6d78d9d46.\n\nThe registered version becomes available for new wallets. Existing wallets upgrade only after their members approve the exact version and module hash.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 3037 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
