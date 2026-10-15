#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# The payload is this call's candid_hex in misc/legacy-freeze/freeze-plan.json, the freeze plan pinned to
# canisters-v3.0.0 that proposal 523 (admin_legacy_acknowledge_baseline on ic_message) took its payload from.
# It was made after the upgrades 516-521, see proposal-523.sh.
# Submit it after the drain executed and legacy_status of 2rgax-kyaaa-aaaap-anvba-cai shows pending = 0:
# the validator otherwise rejects the proposal and lists the writes in flight.
FREEZE_PLAN="${FREEZE_PLAN:-$(pwd)/misc/legacy-freeze/freeze-plan.json}"
# ic_name_identity.wasm.gz of canisters-v3.0.0, the module the plan must be pinned to.
MODULE="fb34c8f2a5180be7bdc2bdcce7a36a87f510f7fbb4a2f1e585ddfdd5ffe9e831"
CALL="$(jq -ec '[.stages[].calls[] | select(.canister == "2rgax-kyaaa-aaaap-anvba-cai" and .method == "admin_legacy_seal")] | if length == 1 then .[0] else error("no single call") end' "$FREEZE_PLAN")"
[ "$(jq -r .expected_module_sha256 <<< "$CALL")" = "$MODULE" ] || { echo "the freeze plan is not pinned to canisters-v3.0.0" >&2; exit 1; }
HEX="$(jq -r .candid_hex <<< "$CALL")"
CUTOVER="$(jq -er .cutover_id "$FREEZE_PLAN")"
[ "$HEX" = "4449444c016d7b010020$CUTOVER" ] || { echo "unexpected payload $HEX" >&2; exit 1; }
export BLOB="blob \"$(sed 's/../\\&/g' <<< "$HEX")\""

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_legacy_seal() on ic_name_identity\";
        url = \"https://github.com/ldclabs/ic-panda/blob/canisters-v3.0.0/src/ic_message_types/src/migration_canister.rs\";
        summary = \"This proposal executes admin_legacy_seal() on the legacy ic_name_identity canister (2rgax-kyaaa-aaaap-anvba-cai) through generic function 1135.\n\nIt makes ic_name_identity read-only for cutover ${CUTOVER}. Business writes never reopen; sign_in, get_delegation and the necessary delegation revocations keep working on the roles fixed at the seal, and legacy_snapshot serves the frozen name-account roles.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_135 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

quill send proposal-message.json
