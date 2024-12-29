# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file $PROPOSAL_PEM_FILE $PROPOSAL_NEURON_ID --proposal "(
    record {
        title = \"Motion Proposal: ICPanda DAO Thanksgiving Event\";
        url = \"https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai\";
        summary = \"The ICPanda DAO team proposes hosting a Thanksgiving event on X platform, starting at 00:00 UTC on November 11, 2024, with two main activities!\";
        action = opt variant {
            Motion = record {
                motion_text = \"The ICPanda DAO team proposes hosting a Thanksgiving event on X platform, starting at 00:00 UTC on November 11, 2024, with two main activities:\n\n**Activity 1**: Each eligible participant will receive 5,000 PANDA tokens by 00:00 UTC on November 28, 2024, based on the following conditions:\n1. The user has registered a username on dMsg.net and completed basic profile information.\n2. The user follows ICPanda DAO on X platform.\n3. The user replies to or quotes the event post on X with a message containing their username.\nEligible users will receive 5,000 PANDA tokens within 24 hours (no additional notice).\n\n**Activity 2**: On November 28, 2024, at 00:00 UTC, three users who registered a username on dMsg.net will be randomly selected, with each receiving 1,000,000 PANDA tokens.\n\nThe event post on X platform will go live after 00:00 UTC on November 11, 2024.\";
            }
        };
    }
)" > proposal-message.json

# quill send proposal-message.json
