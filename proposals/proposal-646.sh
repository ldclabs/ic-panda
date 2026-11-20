#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Write the signed proposal message to stdout for review and separate submission.
payload=$(didc encode --format blob '(record { of_principal = principal "i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe"; permission = variant { Commit } })')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Revoke direct asset publication by DEV on dMsg Connect\";
      url = \"https://connect.dmsg.net/\";
      summary = \"The DEV principal (i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe) used its Commit permission to publish /.well-known/assetlinks.json on connect.dmsg.net directly. This proposal calls revoke_permission on dmsg_connect (acjs3-cyaaa-aaaas-qg7da-cai) to remove that permission. On execution, SNS governance will be the only currently listed Commit holder. Future asset batches can be reviewed and published through governance function 3000. The file already served at that URL remains in place.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 3002 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json