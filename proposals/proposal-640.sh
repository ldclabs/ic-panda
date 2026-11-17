#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.11"
shasum -a 256 -c <<< "942a71cb701653943544461d97fe82e91ceceb8b189a32758b18127605d525e5  $CANISTERS_PATH/dmsg_auth.wasm.gz"

SUMMARY="Upgrade dmsg_auth to canisters-v3.1.11 to support Android Passkey authentication with an explicit signing-certificate allowlist.

Accept Android WebAuthn origins only when the signing-certificate SHA-256 hash is configured by governance. Preserve the existing https://dmsg.net web origin and existing identities, sessions and authentication configuration. The new allowlist starts empty, so this upgrade alone does not enable Android Passkey authentication.

Add admin_set_android_signers and validate_admin_set_android_signers for governance to replace the allowlist with up to eight distinct, nonzero certificate hashes. An empty list disables new Android WebAuthn authentication; changing the list does not revoke already issued renewable sessions. Registering this function and configuring production signing certificates require separate proposals.

This proposal upgrades only dmsg_auth and supplies no upgrade arguments. The downloaded module hash and embedded Candid interface were checked; the interface remains compatible with the pre-Android service. This proposal does not deploy the frontend Android login callback.

Module: dmsg_auth.wasm.gz of canisters-v3.1.11, SHA-256 942a71cb701653943544461d97fe82e91ceceb8b189a32758b18127605d525e5."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "mpld4-maaaa-aaaas-qhepa-cai" --wasm-path "$CANISTERS_PATH/dmsg_auth.wasm.gz" --mode upgrade --title "Upgrade dmsg_auth canister to canisters-v3.1.11" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.11" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
