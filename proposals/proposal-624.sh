#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.7"
shasum -a 256 -c <<< "25dd7d486c8ae80b1dd06c6bc0297f7b5adfd8b307d2586eed177b3322bbce30  $CANISTERS_PATH/dmsg_commerce.wasm.gz"

SUMMARY="Upgrade dmsg_commerce to canisters-v3.1.7 to support App Store sandbox subscriptions on production accounts.

Separate the Apple transaction environment from the business deployment. Add an explicit accept_sandbox opt-in for sandbox annual subscriptions while keeping the App Store configuration in Production. TestFlight can use existing production accounts and services; sandbox terms follow Apple's actual expiry and do not grant storage packs.

Give formal cash, PANDA and production App Store memberships priority over sandbox memberships. Isolate Apple transaction identities, upgrades and refunds by environment. Sandbox history does not block formal subscription purchases or consume the first cash purchase's refund window.

Preserve old Candid and stable records, recover legacy contract environments from recorded transactions, and reject incomplete transaction history instead of treating an existing membership as absent.

This upgrade adds the support introduced in commit df85b8c and its follow-up fixes; it does not enable sandbox admission by itself. Execute before proposal 625, which separately enables accept_sandbox through set_app_store_config. The Apple verifier gateway and client release settings are configured separately.

Module: dmsg_commerce.wasm.gz of canisters-v3.1.7, SHA-256 25dd7d486c8ae80b1dd06c6bc0297f7b5adfd8b307d2586eed177b3322bbce30."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "uwist-sqaaa-aaaaj-qsjyq-cai" --wasm-path "$CANISTERS_PATH/dmsg_commerce.wasm.gz" --mode upgrade --title "Upgrade dmsg_commerce canister to canisters-v3.1.7" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.7" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
