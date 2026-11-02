#!/usr/bin/env bash
set -euo pipefail

# Submit after proposal 574 has registered the dMsg canisters with SNS.
# Like proposals 522-540, this uses icp-cli to preserve the topic field.
# Running this script submits one proposal immediately; use an identity
# authorized to make proposals for the neuron below.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Register admin_set_custom_domains on dmsg_directory";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_directory/dmsg_directory.did";
        summary = "Registers generic function 3013 to call admin_set_custom_domains on dmsg_directory (wlv43-byaaa-aaaaj-qsjxa-cai), with validate_admin_set_custom_domains on the same canister validating and rendering future execution payloads. Topic: CriticalDappOperations.\n\nReplace the custom domains advertised by the directory while retaining the host of the fixed principal origin.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_013 : nat64;
            name = "admin_set_custom_domains on dmsg_directory";
            description = opt "Replace the custom domains advertised by the directory while retaining the host of the fixed principal origin.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "wlv43-byaaa-aaaaj-qsjxa-cai";
                target_canister_id = opt principal "wlv43-byaaa-aaaaj-qsjxa-cai";
                validator_method_name = opt "validate_admin_set_custom_domains";
                target_method_name = opt "admin_set_custom_domains";
              }
            };
          }
        };
      }
    };
  },
)'
