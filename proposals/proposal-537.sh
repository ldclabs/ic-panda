#!/usr/bin/env bash

# quill can not support the topic field, so this calls manage_neuron with icp-cli and submits the
# proposal at once. Run it as an identity that controls the proposal neuron.
icp canister call -n ic dwv6s-6aaaa-aaaaq-aacta-cai manage_neuron '(
  record {
    subaccount = blob "\84\5a\11\4e\6c\35\0d\a9\24\ea\9c\6b\21\cf\f5\04\e2\02\19\e8\3b\60\a6\2c\96\da\36\ad\41\0e\e0\dd";
    command = opt variant {
      MakeProposal = record {
        title = "Add a generic function to seal the legacy ic_message_channel_02 canister";
        url = "https://github.com/ldclabs/ic-panda/blob/canisters-v3.0.0/src/ic_message_types/src/migration_canister.rs";
        summary = "Adds generic function 1132, which calls admin_legacy_seal on the legacy ic_message_channel_02 canister (zof5a-5yaaa-aaaai-acr2q-cai); validate_admin_legacy_seal on the same canister validates and renders each proposal. Topic: CriticalDappOperations.\n\nIt makes the legacy ic_message_channel_02 canister read-only for the dMsg cutover once no write is in flight and the baseline review is recorded. Business writes never reopen. The payload is the 32-byte cutover ID.\n\nThe legacy dMsg canisters ic_message, ic_message_channel, ic_message_channel_02, ic_message_channel_03, ic_message_profile and ic_name_identity freeze in three steps for the migration to the new dMsg services: record the baseline review, drain, and seal once no write is in flight.";
        action = opt variant {
            AddGenericNervousSystemFunction = record {
                id = 1_132 : nat64;
                name = "`admin_legacy_seal` on ic_message_channel_02";
                description = opt "Makes the legacy ic_message_channel_02 canister read-only for the dMsg cutover once no write is in flight and the baseline review is recorded. Business writes never reopen. The payload is the 32-byte cutover ID.";
                function_type = opt variant {
                    GenericNervousSystemFunction = record {
                        topic = opt variant { CriticalDappOperations };
                        validator_canister_id = opt principal "zof5a-5yaaa-aaaai-acr2q-cai";
                        target_canister_id = opt principal "zof5a-5yaaa-aaaai-acr2q-cai";
                        validator_method_name = opt "validate_admin_legacy_seal";
                        target_method_name = opt "admin_legacy_seal";
                    }
                };
            }
        };
      }
    };
  },
)'
