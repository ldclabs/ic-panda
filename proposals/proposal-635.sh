#!/usr/bin/env bash
set -euo pipefail

# Submit after proposal 634 has registered dmsg_auth with SNS.
# Confirm AuthInit.governance is dwv6s-6aaaa-aaaaq-aacta-cai before execution proposals.
# Function IDs 3043/3044 were unused and not reserved on 2026-11-11; recheck before submitting.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register admin_set_limits on dmsg_auth";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.9/src/dmsg_auth/dmsg_auth.did";
        summary = "Registers generic function 3043 to call admin_set_limits on dmsg_auth (mpld4-maaaa-aaaas-qhepa-cai), with validate_admin_set_limits on the same canister validating and rendering future execution payloads. Topic: ApplicationBusinessLogic.\n\nSet the maximum identity count, daily registration budget, per-minute interactive request and renewal budgets, and pending-challenge capacity. All budgets must be positive and pending challenges must be between 1 and 100,000. Existing identities and sessions are retained.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_043 : nat64;
            name = "admin_set_limits on dmsg_auth";
            description = opt "Set the maximum identity count, daily registration budget, per-minute interactive request and renewal budgets, and pending-challenge capacity. All budgets must be positive and pending challenges must be between 1 and 100,000. Existing identities and sessions are retained.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { ApplicationBusinessLogic };
                validator_canister_id = opt principal "mpld4-maaaa-aaaas-qhepa-cai";
                target_canister_id = opt principal "mpld4-maaaa-aaaas-qhepa-cai";
                validator_method_name = opt "validate_admin_set_limits";
                target_method_name = opt "admin_set_limits";
              }
            };
          }
        };
      }
    };
  },
)'
