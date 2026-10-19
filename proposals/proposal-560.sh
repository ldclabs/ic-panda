#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=73:nat; evidence=blob "\1e\05\3b\df\29\45\89\20\aa\b2\5e\aa\d8\a1\1e\80\64\08\1a\ec\e0\de\4d\8e\6b\7f\15\f8\63\f0\60\1b"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.2.1\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.2.1.\n\n1. feat: show Agent Protocols in the hero figure: the gatehouse now works both ways, with the agent's Agent ID as a fifth room under the dome and an 'Out in the open' band of the four production hosts on api.dmsg.net (Agent Discourse, Mail, Knowledge and Delegation) that the agent's signed replies, letters, notes and approved delegations travel to; the hero is compacted so the headline and the whole figure fit a 1728 x 960 first screen in all six languages, and the share image follows;\n2. feat: add the signature verification page at /_/verify, which checks an attestation bundle exported from dMsg in the browser against the handle registry's user homes, reports content, key and account, and authorization evidence check by check, and compares an optional original file by hashing it locally without uploading it;\n3. fix: report a malformed bundle on /_/verify as one, not as an unreachable Internet Computer.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
