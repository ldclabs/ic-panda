#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=77:nat; evidence=blob "\41\ed\1e\b2\31\4f\3a\ac\89\ea\9b\4d\01\9b\72\e4\d2\63\06\ec\79\d0\f8\c7\bf\90\12\51\73\a3\12\55"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.0\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.0.\n\n1. feature: render public account and handle pages from the same certified PrincipalDocument used by agents, independently verify the directory's ICP proof in the browser, and show the canonical principal ID and update time; replace the retired cloud Profile and device-signature display;\n2. improve: display document names, descriptions, HTTPS avatars, named links and legacy link artwork; allow these image sources in the site Content Security Policy and update the verification wording in all six languages to clarify that profile claims are self-published;\n3. harden: require canonical JCS JSON, enforce the 64 KiB document and 32 KiB metadata limits and display-field bounds, and cross-check the directory configuration against the account home and name registry before rendering;\n4. simplify: remove the retired shared-channel succession and member-claim workflow from the legacy page, while retaining personal history inventory and encrypted migration export.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
