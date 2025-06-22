#!/usr/bin/env bash

# quill can not support topic field, we should use dfx canister call to send proposal:
dfx canister --network ic call dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Add a generic function \"admin_add_allowed_apis\" to ic_cose_canister service";
        url = "https://github.com/ldclabs/ic-cose/blob/main/src/ic_cose_canister/src/api_admin.rs";
        summary = "The `admin_add_allowed_apis` function is used to enable more APIs for ic_cose_canister service.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_117 : nat64;
                name = "`admin_add_allowed_apis` function";
                description = opt "It is used to add API to enable more APIs for ic_cose_canister service.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        topic = opt variant { ApplicationBusinessLogic };
                        validator_canister_id = opt principal "n3bau-gaaaa-aaaaj-qa4oq-cai";
                        target_canister_id = opt principal "n3bau-gaaaa-aaaaj-qa4oq-cai";
                        validator_method_name = opt "validate2_admin_add_allowed_apis";
                        target_method_name = opt "admin_add_allowed_apis";
                    }
                };
            }
        };
      }
    };
  },
)'