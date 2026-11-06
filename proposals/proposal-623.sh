#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.6"
shasum -a 256 -c <<< "25f3ba14f7018e84464b4643d4f24443987deae21a65816c2c859b7bea99e9d0  $CANISTERS_PATH/membership.wasm.gz"

SUMMARY="Upgrade membership to canisters-v3.1.6 to extend PANDA qualification leases from one hour to at most one day.

Leases remain anchored to the start of the SNS qualification check and cannot exceed the subscription end. Active eligible claims normally refresh near expiry, reducing SNS neuron reads from roughly hourly to roughly daily and allowing longer resource leases for file uploads.

Eligibility changes may therefore take up to a day to be detected. Confirmed ineligibility still terminates the claim immediately; its neuron remains reserved until previously issued leases expire, which can now take up to a day.

Execute only after the dmsg_commerce upgrade in proposal 622 and after any other active product adapters support day-long qualification leases. Older adapters reject the longer leases.

Module: membership.wasm.gz of canisters-v3.1.6, SHA-256 25f3ba14f7018e84464b4643d4f24443987deae21a65816c2c859b7bea99e9d0."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE --target-canister-id "wmu2p-maaaa-aaaaj-qsjxq-cai" --wasm-path "$CANISTERS_PATH/membership.wasm.gz" --mode upgrade --title "Upgrade membership canister to canisters-v3.1.6" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.6" $PROPOSAL_NEURON_ID > proposal-message.json

# quill send proposal-message.json
