#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

# dfx canister call dmsg_frontend delete_batch \
#   '(record { batch_id = 78 : nat })' --ic

# build and get batch_id, evidence:
# dfx deploy dmsg_frontend --ic --by-proposal

export BLOB="$(didc encode --format blob '(record {batch_id=84:nat; evidence=blob "\e4\f6\5f\22\60\6a\bf\dd\de\40\04\6e\61\b8\93\ba\41\a1\b0\5b\0a\bb\e1\92\ed\e1\f9\a4\6b\0a\dc\66"})')"

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Execute commit_proposed_batch() to release dmsg_frontend v3.4.6\";
        url = \"https://dMsg.net/\";
        summary = \"This proposal executes commit_proposed_batch() on 2fvu6-tqaaa-aaaap-akksa-cai to release dmsg_frontend v3.4.6.\n\n1. i18n: expand the website from six to 23 locales, including Traditional Chinese, regional language matching, Arabic and Urdu right-to-left layouts, translated privacy, terms and support pages, and localized social cards;\n2. i18n: refine translations and typography for natural wording and consistent terminology across signing, permissions, wallets and storage;\n3. feat: add a Free, Plus, Pro and Max comparison of annual prices, encrypted cloud storage, active devices and channels, monthly formal signatures and team-wallet creation slots. Display a dated snapshot checked against the public commerce v2 catalog and wallet-hub configuration, explain payment options and the separate wallet activation fee, and direct visitors to confirm current terms in the app;\n4. ui: add Pricing to the header navigation, adapt the navigation and benefit columns to narrow windows and long translations, and align the three trust-section icons with their headings, including in right-to-left layouts;\n5. docs: update the mobile-app FAQ in every locale to reflect that iOS is in private beta and Android development is expected to finish soon.\";
        action = opt variant {
            ExecuteGenericNervousSystemFunction = record {
                function_id = 1100 : nat64;
                payload = ${BLOB};
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
