#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=79:nat; evidence=blob "\7f\65\54\64\dd\ad\d5\9d\df\a6\86\a9\1d\91\ef\e7\5e\47\ae\5a\4c\af\34\5b\d9\ae\cf\0b\41\b2\04\8f"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.1\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.1.\n\n1. improve: preserve the certified display spelling of handles and make explicit HTTP/HTTPS URLs in profile descriptions clickable while keeping the remaining text escaped; prioritize avatars and lazy-load link artwork;\n2. performance: parallelize public profile reads, reuse short-lived in-memory proofs, and move requests, decoding and certificate verification into a Web Worker; retain certificate freshness and rollback checks, fall back from invalid deployment hints, and bypass caches when retrying the affected profile;\n3. feature: download encrypted shared files directly through short-lived R2 S3 URLs without cookies or referrers, with matching Content Security Policy support; reuse one download session for retries and URL renewal without consuming another download count, and verify descriptors, sizes, digests and decrypted file integrity before saving;\n4. fix: renew the existing share download session and retry only the failed manifest or chunk once after URL expiry, missing-object responses, connection loss, timeouts, stream failures, throttling or server errors; keep integrity failures terminal and omit bearer URLs from network error messages.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

quill send proposal-message.json
