#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.12"
shasum -a 256 -c <<< "d27b16bebe4d80aff0f48a24824f975d747c9fc3cafb50d57f9358e11ff02845  $CANISTERS_PATH/dmsg_commerce.wasm.gz"

SUMMARY="Upgrade dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai) to canisters-v3.1.12. Governance can authorize issuers to grant Plus, Pro or Max membership for 30 or 365 days as a promotion or compensation. Each grant ID can be applied once. Authorized issuers can retrieve the receipt or revoke the grant. The canister also adds an eligibility check for issuing guest passes.

Paid production contracts take precedence over grants, and grants take precedence over sandbox contracts. A grant alone does not qualify for an annual storage add-on.

The new get_membership_terms_v3 query reports promotional and compensation terms. The older queries return UnsupportedProtocol for sources they cannot represent. Upgrade commerce before deploying clients and services that read v3 terms. Grants are disabled by default and require a separate governance action to configure issuers and enable issuance.

Module SHA-256 (dmsg_commerce.wasm.gz): d27b16bebe4d80aff0f48a24824f975d747c9fc3cafb50d57f9358e11ff02845."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "uwist-sqaaa-aaaaj-qsjyq-cai" --wasm-path "$CANISTERS_PATH/dmsg_commerce.wasm.gz" --mode upgrade --title "Upgrade dmsg_commerce canister to canisters-v3.1.11" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.11" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
