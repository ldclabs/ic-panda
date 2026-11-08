#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

# Submit after proposal 624 has upgraded dmsg_commerce to canisters-v3.1.7.
# Preserve the mainnet configuration read on 2026-11-07; only enable accept_sandbox.
APP_STORE_CONFIG='(record {
    verifiers = vec {
        principal "xpv5w-qhjai-ybbkt-b47a3-xvigr-l3nz6-e2wtu-zrudz-4jofa-bqtta-rqe";
    };
    bundle_id = "net.dmsg.ios";
    app_apple_id = 6827050680 : nat64;
    environment = variant { Production };
    accept_sandbox = opt true;
    products = vec {
        record {
            product_id = "net.dmsg.ios.plus.annual";
            kind = variant { Plan = variant { Plus } };
        };
        record {
            product_id = "net.dmsg.ios.pro.annual";
            kind = variant { Plan = variant { Pro } };
        };
        record {
            product_id = "net.dmsg.ios.max.annual";
            kind = variant { Plan = variant { Max } };
        };
    };
})'

# After the upgrade, preflight with the same payload:
# dfx --identity anonymous canister --network ic call uwist-sqaaa-aaaaj-qsjyq-cai \
#     validate_set_app_store_config "$APP_STORE_CONFIG" --query --candid src/dmsg_commerce/dmsg_commerce.did
BLOB="$(didc encode --format blob \
    --defs src/dmsg_commerce/dmsg_commerce.did \
    --method set_app_store_config \
    "$APP_STORE_CONFIG")"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
        title = \"Enable App Store sandbox subscriptions on dmsg_commerce\";
        url = \"https://github.com/ldclabs/ic-panda/releases/tag/canisters-v3.1.7\";
        summary = \"Execute set_app_store_config on dmsg_commerce (uwist-sqaaa-aaaaj-qsjyq-cai) through generic function 3035 after the upgrade in proposal 624.\n\nSet accept_sandbox to opt true while retaining the Production environment, bundle net.dmsg.ios, Apple ID 6827050680, existing verifier principal and the Plus, Pro and Max annual product mappings. This admits verified Apple sandbox annual subscriptions on production accounts, including TestFlight purchases; it does not configure sandbox storage packs.\n\nFormal cash, PANDA and production App Store memberships retain priority. Sandbox terms use Apple's actual expiry, and upgrades and refunds remain isolated by Apple environment. No test-account allowlist is required.\n\nThe Apple verifier gateway and client release settings must also permit sandbox purchases; this proposal changes only the governed commerce configuration. Disabling accept_sandbox later stops new sandbox original subscriptions while allowing already registered subscriptions to restore, renew and reconcile refunds or revocations.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 3035 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
