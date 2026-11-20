#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Write the signed proposal message to stdout for review and separate submission.
payload=$(didc encode --format blob '(record { batch_id=99:nat; evidence=blob "\c9\89\7a\6a\c5\b7\d6\bc\45\af\69\46\72\0d\af\1d\80\07\7a\13\d3\84\52\b2\aa\c3\23\7c\3c\2b\c3\6d" })')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Publish dMsg website asset batch 99 for Android\";
      url = \"https://dmsg.net/\";
      summary = \"Publish batch 99 on dmsg_frontend (2fvu6-tqaaa-aaaap-akksa-cai). Its evidence is c9897a6ac5b7d6bc45af6946720daf1d80077a13d38452b2aac3237c3c2bc36d. The website associates dmsg.net with the Google Play signing certificate for net.dmsg.android for Android App Links and credential sharing. The privacy policy describes Android notifications, installation verification and purchases. The updated terms cover mobile store billing and remain at /_/terms/v1.txt, with SHA-256 53fc118e607cf750fbb271b43d6c14701c45628bec5413b34cee4966be1a448a. This action publishes website assets only; updating the terms digest registered in commerce requires a separate action.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 1100 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json