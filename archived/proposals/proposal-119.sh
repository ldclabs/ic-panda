#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

export BLOB="$(didc encode --format blob '(record {channel=opt (1_000_000_000:nat64)})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Try again: execute admin_update_price() to update the price for creating a channel to 10 PANDA tokens\";
        url = \"https://panda.fans/_/messages\";
        summary = \"This proposal executes admin_update_price() on nscli-qiaaa-aaaaj-qa4pa-cai to update the price for creating a channel to 10 PANDA tokens. The original price was equivalent to recharging 1000 PANDA worth of gas to a channel, while the new price recharges 10 PANDA worth of gas to a channel.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1_104 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json