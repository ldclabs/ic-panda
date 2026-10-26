#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=83:nat; evidence=blob "\ae\98\ec\69\dd\de\22\0f\b7\f0\bd\a2\77\98\0c\31\3f\af\b5\bb\a2\05\ac\5c\4e\2a\ef\07\ca\3b\99\47"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.5\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.5.\n\n1. feat: support signing in to the dMsg iOS app with dmsg-login/1; besides loopback callbacks, the login page accepts exactly one HTTPS callback, https://dmsg.net/_/ios/auth/callback, and says that signing in returns to the iOS app;\n2. feat: serve /.well-known/apple-app-site-association as JSON without redirects, associating the callback path and web credentials with the signed app S9Z2WAMAV8.net.dmsg.ios so that iOS returns the sign-in to the app;\n3. security: the callback page, if opened outside the app's sign-in session, removes the response from the address bar, sends no referrer, never forwards a delegation and asks to start again in the app;\n4. i18n: the sign-in page no longer assumes a computer, and the new iOS texts are translated into all six supported languages.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
