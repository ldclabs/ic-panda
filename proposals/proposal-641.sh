#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.11"
shasum -a 256 -c <<< "45cd31cfb13e6a806e0786938832280bc81d185ea9a34d6171c828ac89bcf3de  $CANISTERS_PATH/dmsg_commerce.wasm.gz"

SUMMARY="Upgrade dmsg_commerce to canisters-v3.1.11 to support verified Google Play membership and storage purchases while preserving existing payment and App Store flows.

Add Google Play configuration, verifier-authorized receipt submission and current purchase status handling. Bind purchases to the configured Android package, environment and product mappings; handle refunds, holds and replacements without treating client purchase callbacks as proof of entitlement. Recheck plan admission before restoring previously undelivered purchases, and bound store-backed resource eligibility to one hour.

Add get_membership_terms_v2 to expose Google Play membership sources. Keep the original get_membership_terms response type unchanged; it returns UnsupportedProtocol when the terms contain a Google Play source. Existing certified entitlement and monthly allowance formats remain unchanged. Clients that display Google Play terms must use the v2 query.

Add set_google_play_config and validate_set_google_play_config. Registering the governance function and supplying verifier principals, application identity and product mappings require separate proposals; this upgrade alone does not configure Google Play purchases. Upgrade commerce before deploying clients or verifiers that depend on these APIs. Once Google Play contracts are stored, rolling back to a module that cannot decode the new source variant is unsafe.

This proposal upgrades only dmsg_commerce and supplies no upgrade arguments. The downloaded module hash and embedded Candid interface were checked; the interface remains compatible with the pre-Android service. No live Google Play purchase validation or frontend deployment is part of this proposal.

Module: dmsg_commerce.wasm.gz of canisters-v3.1.11, SHA-256 45cd31cfb13e6a806e0786938832280bc81d185ea9a34d6171c828ac89bcf3de."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "uwist-sqaaa-aaaaj-qsjyq-cai" --wasm-path "$CANISTERS_PATH/dmsg_commerce.wasm.gz" --mode upgrade --title "Upgrade dmsg_commerce canister to canisters-v3.1.11" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.11" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
