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
        title = "Register initialize_keys on dmsg_cose";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.1.5/src/dmsg_cose/dmsg_cose.did";
        summary = "Registers generic function 3011 to call initialize_keys on dmsg_cose (wcwxh-xqaaa-aaaaj-qsjwq-cai), with validate_initialize_keys on the same canister validating and rendering future execution payloads. Topic: CriticalDappOperations.\n\nInitialize the content-root vetKD public key and verify its configured fingerprint. The deployment key name, derivation domain and expected fingerprint remain fixed.\n\nThis proposal registers the callable function; a separate execution proposal supplies the configuration payload.";
        action = opt variant {
          AddGenericNervousSystemFunction = record {
            id = 3_011 : nat64;
            name = "initialize_keys on dmsg_cose";
            description = opt "Initialize the content-root vetKD public key and verify its configured fingerprint. The deployment key name, derivation domain and expected fingerprint remain fixed.";
            function_type = opt variant {
              GenericNervousSystemFunction = record {
                topic = opt variant { CriticalDappOperations };
                validator_canister_id = opt principal "wcwxh-xqaaa-aaaaj-qsjwq-cai";
                target_canister_id = opt principal "wcwxh-xqaaa-aaaaj-qsjwq-cai";
                validator_method_name = opt "validate_initialize_keys";
                target_method_name = opt "initialize_keys";
              }
            };
          }
        };
      }
    };
  },
)'
