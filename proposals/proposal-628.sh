#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables
source "$(pwd)"/proposals/env.sh

CANISTERS_PATH="$(pwd)/misc/canisters/canisters-v3.1.8"
shasum -a 256 -c <<< "104f778098d1a6676bf4fa5809fc3550bd7ec7f8a89d4abba5c1182673ec9b76  $CANISTERS_PATH/dmsg_wallet_hub.wasm.gz"

SUMMARY="Upgrade dmsg_wallet_hub to canisters-v3.1.8 to enforce wallet-creation approval deadlines and preserve unresolved cycles top-up payments.

Recheck the verified WalletCreate authorization deadline after the ledger balance call returns, before recording a paid activation or consuming its quota. An approval that expires during the cross-canister calls can no longer create a wallet.

During activation revenue collection, treat only an explicit CMC Refunded response as a completed refund. Other errors, including InvalidTransaction and TransactionTooOld, retain the payment block and leave collection pending so it can retry without sending the payment again.

This proposal upgrades the hub only. Wallet module registration is a separate proposal, and existing wallets upgrade only after their members approve the exact version and module hash.

Module: dmsg_wallet_hub.wasm.gz of canisters-v3.1.8, SHA-256 104f778098d1a6676bf4fa5809fc3550bd7ec7f8a89d4abba5c1182673ec9b76."

quill sns make-upgrade-canister-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" --target-canister-id "ghcje-dyaaa-aaaas-qg7va-cai" --wasm-path "$CANISTERS_PATH/dmsg_wallet_hub.wasm.gz" --mode upgrade --title "Upgrade dmsg_wallet_hub canister to canisters-v3.1.8" --summary "$SUMMARY" --url "https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.8" "$PROPOSAL_NEURON_ID" > proposal-message.json

# quill send proposal-message.json
