#!/usr/bin/env bash
set -euo pipefail

# Submit after proposal 574 has registered the dMsg canisters with SNS.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
# Before execution proposals, grant SNS governance ManagePermissions on dmsg_connect.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register revoke_permission on dmsg_connect";
        url = "https://github.com/ldclabs/ic-panda/blob/main/src/dmsg_connect/README.md";
        summary = "Registers generic function 3002 to call revoke_permission on dmsg_connect (acjs3-cyaaa-aaaas-qg7da-cai), with validate_revoke_permission on the same canister validating and rendering future execution payloads. Topic: CriticalDappOperations.\n\nRevoke a principal from Prepare, Commit or ManagePermissions, including removing direct asset publication rights from former deployers.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_002 : nat64;
            name = "revoke_permission on dmsg_connect";
            description = opt "Revoke a principal from Prepare, Commit or ManagePermissions, including removing direct asset publication rights from former deployers.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "acjs3-cyaaa-aaaas-qg7da-cai";
                target_canister_id = opt principal "acjs3-cyaaa-aaaas-qg7da-cai";
                validator_method_name = opt "validate_revoke_permission";
                target_method_name = opt "revoke_permission";
              }
            };
          }
        };
      }
    };
  },
)'
