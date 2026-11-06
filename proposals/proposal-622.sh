#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.6"
shasum -a 256 -c <<< "d24e11c897f86ad0a3eee7b0d9fe727b68112b4ab0193f176dda1cbca62615aa  $CANISTERS_PATH/dmsg_commerce.wasm.gz"

SUMMARY="Upgrade dmsg_commerce to canisters-v3.1.6 to support day-long PANDA membership leases.

Extend PANDA-backed resource leases from at most one hour to at most one day, bounded by the membership qualification lease and the next contract or catalog boundary. This gives large or slow file uploads more time before their storage reservation expires.

Keep resource leases at one hour for first cash purchases within their refund window and for App Store subscriptions, so refunds can still take effect promptly. Other cash and Free resource leases retain their existing limits.

Execute this upgrade before the membership upgrade in proposal 623: the previous commerce adapter rejects qualification leases longer than one hour.

Module: dmsg_commerce.wasm.gz of canisters-v3.1.6, SHA-256 d24e11c897f86ad0a3eee7b0d9fe727b68112b4ab0193f176dda1cbca62615aa."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "uwist-sqaaa-aaaaj-qsjyq-cai" --wasm-path "$CANISTERS_PATH/dmsg_commerce.wasm.gz" --mode upgrade --title "Upgrade dmsg_commerce canister to canisters-v3.1.6" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.6" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json
