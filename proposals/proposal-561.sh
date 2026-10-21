#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=74:nat; evidence=blob "\a6\b8\fa\8e\b6\c2\dd\38\ea\f0\14\f7\34\c5\37\41\ee\2c\c1\1d\00\04\f9\ed\f5\13\63\e1\7f\34\7d\b1"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.3.0\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.3.0.\n\n1. feat: redesign the landing page around private space, collaboration and the agent network, with a five-step project walkthrough, distinct Agent Mail, Discourse and Knowledge outcomes, optional product demos, and updated navigation, downloads and FAQs in all six languages;\n2. feat: share one vector scene between the animated hero and six localized social cards, showing people and their agent around the private workspace, with separate paths for scoped workspace access and the agent network; add pause controls, respect reduced-motion preferences, stop animation offscreen or in hidden tabs, and improve narrow layouts and RTL artwork;\n3. feat: add public profiles at /<account_id> and /<handle>, verifying certified handle mappings, account device records and signed profile contents before display; keep permanent links tied to account IDs and distinguish unpublished profiles, unavailable services, verification failures and historical profiles signed by revoked devices;\n4. fix: keep the Internet Identity login window open when heartbeat replies are delayed, without multiplying polling or dropping valid pending requests;\n5. fix: move historical profiles to /legacy/users/<username> and /legacy/profile, keep the legacy layout stable when the channel list is empty, and mark migration fields to be ignored by 1Password autofill.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
