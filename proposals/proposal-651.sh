#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source proposals/env.sh

# Write the signed proposal message to proposal-message.json for review and separate submission.
payload=$(didc encode --format blob '(vec {blob "\05\bd\ce\89\dc\ed\b9\0a\98\d8\83\84\45\8d\16\2c\25\7a\2f\80\06\a7\5c\6f\bd\41\fe\d9\78\22\59\d6"; blob "\76\88\ca\69\25\2c\b9\99\75\23\a2\9e\17\4f\44\8d\dc\5b\fb\81\09\22\61\29\6d\ce\b4\f9\18\f4\0c\ea"; blob "\b3\ee\f5\48\77\ca\0d\36\cc\a3\7a\e0\71\89\4d\a0\57\5c\00\55\8b\ff\21\8f\c5\06\fe\3b\16\02\2c\5f"})')
quill sns make-proposal --canister-ids-file ./sns_canister_ids.json \
  --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal "(
    record {
      title = \"Admit Android 17 Play signing certificates for dMsg passkeys\";
      url = \"https://dmsg.net/\";
      summary = \"Update the dmsg_auth Android WebAuthn signer allowlist for net.dmsg.android. Retain the legacy Play certificate 05:BD:CE:89:DC:ED:B9:0A:98:D8:83:84:45:8D:16:2C:25:7A:2F:80:06:A7:5C:6F:BD:41:FE:D9:78:22:59:D6 and add the current hybrid classical certificate 76:88:CA:69:25:2C:B9:99:75:23:A2:9E:17:4F:44:8D:DC:5B:FB:81:09:22:61:29:6D:CE:B4:F9:18:F4:0C:EA and post-quantum certificate B3:EE:F5:48:77:CA:0D:36:CC:A3:7A:E0:71:89:4D:A0:57:5C:00:55:8B:FF:21:8F:C5:06:FE:3B:16:02:2C:5F. Android 17 treats the post-quantum certificate as the current app identity. The client checks the installed signer before opening the system credential dialog, and the canister checks the signed origin when verifying the credential.\";
      action = opt variant {
        ExecuteGenericNervousSystemFunction = record {
          function_id = 3045 : nat64;
          payload = ${payload};
        }
      };
    }
  )" > proposal-message.json

# quill send proposal-message.json
