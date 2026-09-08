#!/usr/bin/env bash

# Load the environment variables
source "$(pwd)"/proposals/env.sh

quill sns make-proposal --canister-ids-file ./sns_canister_ids.json --pem-file "$PROPOSAL_PEM_FILE" "$PROPOSAL_NEURON_ID" --proposal '(
    record {
        title = "Adopt the ICPanda DAO Whitepaper v1.0: From Sovereign Minds to Sovereign Markets";
        url = "https://forum.dfinity.org/t/icpanda-dao-has-evolved/75567";
        summary = "Adopt the ICPanda DAO Whitepaper v1.0 (August 2026, revised September 7, 2026), From Sovereign Minds to Sovereign Markets, as the DAO governance statement of mission, principles, and long-term direction. The whitepaper describes ICPanda as a Builder DAO developing open infrastructure for digital sovereignty through Anda, TokenList, and dMsg, with PANDA supporting SNS governance, treasury allocation, and community coordination. Upon adoption, it supersedes the 2024 Genesis Whitepaper, which remains public as a historical record. The exact document submitted for this vote is linked in the motion text.";
        action = opt variant {
            Motion = record {
                motion_text = "## Resolution\n\nICPanda DAO adopts Whitepaper v1.0, From Sovereign Minds to Sovereign Markets (August 2026, revised September 7, 2026), as its statement of mission, foundational principles, PANDA institutional role, and long-term direction.\n\nThe exact English document submitted for adoption is whitepaper/en.md at commit 297519a743209e1d1c016aff22f2d0e960c3b380:\nhttps://github.com/ldclabs/ic-panda/blob/297519a743209e1d1c016aff22f2d0e960c3b380/whitepaper/en.md\n\n## Direction\n\nICPanda evolves from its origins as a meme community into a Builder DAO using shared governance, capital, and engineering to build open infrastructure for digital sovereignty. Its three complementary directions are:\n\n- Anda: persistent machine cognition, with portable identity, memory, experience, and skill.\n- TokenList: transparent and verifiable asset creation, issuance, markets, and governance.\n- dMsg: a planned Chrome workspace for private data, encrypted collaboration, and explicit cross-application signing. Implementation, deployment, audits, and release validation of the new design remain to be completed.\n\nPANDA supports SNS governance, allocation of DAO-controlled resources, and community coordination. The DAO commits to openness, evidence, portability, responsible use of resources, and visible, accountable authority.\n\n## Scope and continuity\n\nUpon adoption, this whitepaper supersedes the 2024 Genesis Whitepaper as the current institutional statement. The Genesis edition remains available at https://github.com/ldclabs/ic-panda/blob/main/whitepaper/2024/en.md as a historical record.\n\nThis motion adopts the whitepaper; it does not itself transfer treasury funds, change SNS or token parameters, deploy software, or approve specific product releases. Product specifications and operational roadmaps remain in their respective project documents. Canonical on-chain state takes precedence for supply, balances, treasury movements, and governance parameters.\n\nMaterial changes to the mission, PANDA role, foundational principles, or long-term thesis should normally return to SNS governance. Earlier substantive versions should be preserved and changes recorded; editorial corrections, source updates, and factual clarifications do not require a new institutional mandate.\n\nAfter adoption, record the actual SNS proposal number and adoption date in the whitepaper and update its proposed status to adopted.\n\n## Community discussion\n\nICPanda DAO Has Evolved:\nhttps://forum.dfinity.org/t/icpanda-dao-has-evolved/75567";
            }
        };
    }
)' > proposal-message.json

# quill send proposal-message.json
