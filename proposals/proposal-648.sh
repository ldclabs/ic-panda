#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Write the signed proposal message to stdout for review and separate submission.
payload=$(didc encode --format blob '(vec {blob "\05\bd\ce\89\dc\ed\b9\0a\98\d8\83\84\45\8d\16\2c\25\7a\2f\80\06\a7\5c\6f\bd\41\fe\d9\78\22\59\d6"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Admit the dMsg Google Play signer for Android passkeys\";
      url = \"https://dmsg.net/\";
      summary = \"Call admin_set_android_signers on dmsg_auth (mpld4-maaaa-aaaas-qhepa-cai). The deployed allowlist is currently empty. Replace it with the Google Play app signing certificate SHA-256 05:BD:CE:89:DC:ED:B9:0A:98:D8:83:84:45:8D:16:2C:25:7A:2F:80:06:A7:5C:6F:BD:41:FE:D9:78:22:59:D6 for net.dmsg.android. This admits the matching Android WebAuthn origin during passkey verification. The verifier continues to check the credential, signature and challenge. This is a configuration change to the deployed module.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 3045 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json