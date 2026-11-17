#!/usr/bin/env bash
set -euo pipefail

# Submit after the dmsg_auth upgrade in proposal-640.sh has executed.
# Function ID 3045 was unused and not reserved on 2026-11-17;
# recheck the registry and pending registration proposals before submitting.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register admin_set_android_signers on dmsg_auth";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.11/src/dmsg_auth/dmsg_auth.did";
        summary = "Registers generic function 3045 to call admin_set_android_signers on dmsg_auth (mpld4-maaaa-aaaas-qhepa-cai), with validate_admin_set_android_signers on the same canister validating and rendering future execution payloads. Topic: CriticalDappOperations.\n\nReplace the Android WebAuthn signing-certificate allowlist with up to eight distinct, nonzero SHA-256 certificate hashes. An empty list disables new Android WebAuthn authentication while preserving the https://dmsg.net web origin. Updating the allowlist does not revoke already issued renewable sessions.\n\nThis proposal registers the callable function; a separate execution proposal supplies the production signing-certificate hashes. It does not configure or enable Android authentication by itself.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_045 : nat64;
            name = "admin_set_android_signers on dmsg_auth";
            description = opt "Replace the Android WebAuthn signing-certificate SHA-256 allowlist. An empty list disables new Android WebAuthn authentication; existing sessions are not revoked.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "mpld4-maaaa-aaaas-qhepa-cai";
                target_canister_id = opt principal "mpld4-maaaa-aaaas-qhepa-cai";
                validator_method_name = opt "validate_admin_set_android_signers";
                target_method_name = opt "admin_set_android_signers";
              }
            };
          }
        };
      }
    };
  },
)'
