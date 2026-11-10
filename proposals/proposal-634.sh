#!/usr/bin/env bash
set -euo pipefail

# Load the environment variables. Run from the repository root.
source "$(pwd)"/proposals/env.sh

# Before submitting, the current controller must add SNS root:
# icp canister settings update dmsg_auth -e ic --add-controller d7wvo-iiaaa-aaaaq-aacsq-cai
# Confirm the installed AuthInit.governance is dwv6s-6aaaa-aaaaq-aacta-cai.
# RegisterDappCanisters changes controllers, not that fixed application setting.
# After execution, verify SNS root lists dmsg_auth and is its sole controller.

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal '(
    record {
        title = "Register dmsg_auth with ICPanda SNS";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.9/src/dmsg_auth/README.md";
        summary = "Register dmsg_auth (mpld4-maaaa-aaaas-qhepa-cai) with ICPanda SNS and transfer its canister control to SNS root (d7wvo-iiaaa-aaaaq-aacsq-cai), so future upgrades are governed by PANDA holders through SNS proposals.

dmsg_auth provides first-party Passkey identities and issues fixed 12-hour IC delegations to hardware device keys. Remembered devices can renew on demand; account bindings, device approval, content keys and recovery remain in the existing account services.

The current controller must first add SNS root as a controller. Successful registration leaves SNS root as the sole controller. Preserve the existing canister ID and identity state; this proposal does not reinstall or upgrade dmsg_auth.

The installed governance principal must be ICPanda SNS governance (dwv6s-6aaaa-aaaaq-aacta-cai) for subsequent administrative execution proposals. This registration does not change that application setting. Separate proposals register admin_set_limits and admin_set_admission_key; no limits or admission key are changed here.";
        action = opt variant {
            RegisterDappCanisters = record {
                canister_ids = vec {
                    principal "mpld4-maaaa-aaaas-qhepa-cai"; // dmsg_auth
                };
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
