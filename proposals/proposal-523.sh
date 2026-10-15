#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# Runs generic function 1118 (registered by proposal 522) on ic_message, still on canisters-v3.0.0 whose
# validate_admin_legacy_acknowledge_baseline is a query. SNS calls the validator when the proposal is
# submitted, so a proposal ID from `quill send` already shows the validator ran. If the proposal is adopted,
# the digest is recorded on ic_message for good: a later acknowledgement with another digest fails with
# LegacyBaselineConflict, so make it from the final baseline review.
#
# The payload is this call's candid_hex in the freeze plan (src/dmsg_handle/README.md, 迁移流程 step 2),
# made after the upgrades 516-521 to canisters-v3.0.0 executed. The cutover ID is generated once
# (openssl rand -hex 32) and is used again for the name import plan:
#   node src/dmsg_legacy/scripts/legacy-inventory.mjs --ids .icp/data/mappings/ic.ids.json --output misc/legacy-inventory/20261015
#   node src/dmsg_legacy/scripts/legacy-freeze-inputs.mjs misc/legacy-inventory/20261015/inventory.json misc/release/canisters-v3.0.0 canisters-v3.0.0 misc/legacy-freeze
#   # review misc/legacy-freeze/baseline.json: clear `unresolved`, put each canister's upgrade proposal in evidence_ref
#   # (ic_message 516, ic_message_channel 517, ic_message_channel_02 518, ic_message_channel_03 519,
#   # ic_message_profile 520, ic_name_identity 521)
#   node src/dmsg_legacy/scripts/legacy-freeze-plan.mjs misc/legacy-freeze/targets.json misc/legacy-freeze/baseline.json <cutover hex> misc/legacy-freeze/freeze-plan.json
FREEZE_PLAN="${FREEZE_PLAN:-$(pwd)/misc/legacy-freeze/freeze-plan.json}"
# ic_message.wasm.gz of canisters-v3.0.0, the module the plan must be pinned to.
MODULE="3b6593e25116f453774d3a24119f13e8d1e88f07051f2a7dae76f8a402e389be"
CALL="$(jq -ec '[.stages[].calls[] | select(.canister == "nscli-qiaaa-aaaaj-qa4pa-cai" and .method == "admin_legacy_acknowledge_baseline")] | if length == 1 then .[0] else error("no single call") end' "$FREEZE_PLAN")"
[ "$(jq -r .expected_module_sha256 <<< "$CALL")" = "$MODULE" ] || { echo "the freeze plan is not pinned to canisters-v3.0.0" >&2; exit 1; }
HEX="$(jq -r .candid_hex <<< "$CALL")"
EVIDENCE="$(jq -er .baseline_evidence "$FREEZE_PLAN")"
[ "$HEX" = "4449444c016d7b010020$EVIDENCE" ] || { echo "unexpected payload $HEX" >&2; exit 1; }
export BLOB="blob \"$(sed 's/../\\&/g' <<< "$HEX")\""

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute admin_legacy_acknowledge_baseline() on ic_message\";
        url = \"https://github.com/ldclabs/ic-panda/blob/canisters-v3.0.0/src/ic_message_types/src/migration_canister.rs\";
        summary = \"This proposal executes admin_legacy_acknowledge_baseline() on the legacy ic_message canister (nscli-qiaaa-aaaaj-qa4pa-cai) through generic function 1118.\n\nIt records baseline review ${EVIDENCE}, the SHA-256 of the reviewed baseline that the freeze plan was made from: the upgrade of ic_message to canisters-v3.0.0 (proposal 516) stopped the canister first, so no write the old module started is still in flight. admin_legacy_seal requires this record.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_118 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
