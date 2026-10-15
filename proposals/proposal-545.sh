#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# The payload is this call's candid_hex in misc/legacy-freeze/freeze-plan.json, the freeze plan pinned to
# canisters-v3.0.0 that proposal 523 (admin_legacy_acknowledge_baseline on ic_message) took its payload from.
# It was made after the upgrades 516-521, see proposal-523.sh.
FREEZE_PLAN="${FREEZE_PLAN:-$(pwd)/misc/legacy-freeze/freeze-plan.json}"
# ic_message_profile.wasm.gz of canisters-v3.0.0, the module the plan must be pinned to.
MODULE="6983760b1c65b1e0edd57c320db288ff4b534896c69bb862bf2972eae3fdeeb3"
CALL="$(jq -ec '[.stages[].calls[] | select(.canister == "ijyxz-wyaaa-aaaaj-qa4qa-cai" and .method == "admin_legacy_acknowledge_baseline")] | if length == 1 then .[0] else error("no single call") end' "$FREEZE_PLAN")"
[ "$(jq -r .expected_module_sha256 <<< "$CALL")" = "$MODULE" ] || { echo "the freeze plan is not pinned to canisters-v3.0.0" >&2; exit 1; }
HEX="$(jq -r .candid_hex <<< "$CALL")"
EVIDENCE="$(jq -er .baseline_evidence "$FREEZE_PLAN")"
[ "$HEX" = "4449444c016d7b010020$EVIDENCE" ] || { echo "unexpected payload $HEX" >&2; exit 1; }
export BLOB="blob \"$(sed 's/../\\&/g' <<< "$HEX")\""

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_legacy_acknowledge_baseline() on ic_message_profile\";
        url = \"https://github.com/ldclabs/ic-panda/blob/canisters-v3.0.0/src/ic_message_types/src/migration_canister.rs\";
        summary = \"This proposal executes admin_legacy_acknowledge_baseline() on the legacy ic_message_profile canister (ijyxz-wyaaa-aaaaj-qa4qa-cai) through generic function 1122.\n\nIt records baseline review ${EVIDENCE}, the SHA-256 of the reviewed baseline that the freeze plan was made from and the same review that proposal 523 records on ic_message: the upgrade of ic_message_profile to canisters-v3.0.0 (proposal 520) stopped the canister first, so no write the old module started is still in flight. admin_legacy_seal requires this record.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_122 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
