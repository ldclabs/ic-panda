# ICPanda DAO

**From Sovereign Minds to Sovereign Markets**

Open infrastructure for digital sovereignty.

[ English | [简体中文](zh.md) ]

**Whitepaper v1.0 · August 2026 · Revised September 7, 2026**

**Status: Adopted by SNS governance through [Proposal #504](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/504) on September 8, 2026.** This document supersedes the [2024 Genesis Whitepaper](2024/en.md), which remains available as a historical record.

## 1. Our Purpose

ICPanda DAO is an on-chain **Builder DAO**: a community that uses shared governance, capital, and engineering to build open infrastructure.

Our work begins with a practical question: **as software takes on more work and economic responsibility, who controls its identity, memory, assets, and authority?**

AI agents can use tools and carry out delegated tasks, but their identity and accumulated experience often remain tied to individual applications. Crypto makes assets programmable, but credible markets still require transparent issuance, distribution, and governance. People using these systems need control over what they share and authorize.

We call the ability to participate in digital systems while retaining meaningful control **digital sovereignty**. It includes portable identity and memory, verifiable ownership and exchange, and accountable governance of shared resources.

ICPanda DAO pursues this through three complementary building directions:

| Direction                         | Purpose                                                                                                         |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| **Anda — Sovereign Minds**        | Help intelligent systems preserve identity, memory, experience, and skill across time and applications.         |
| **TokenList — Sovereign Markets** | Make the formation of tokenized economies transparent and verifiable.                                           |
| **dMsg — Personal Control**       | Give people a private workspace for secrets, encrypted collaboration, and explicit signing across applications. |

**PANDA** supports the institution behind this work through SNS governance, treasury allocation, and community coordination.

Each project addresses a useful problem independently. Their shared direction is infrastructure through which people and agents can work together with portable state, inspectable rules, and explicit authority.

## 2. Anda — Sovereign Minds

[Anda](https://anda.ai/) builds open protocols and infrastructure for persistent machine cognition. Its objective is to let intelligent systems carry useful experience forward as models, runtimes, and applications change.

### Memory that changes behavior

A conversation history can preserve what happened without helping an agent decide what to do next. Useful memory must retain context, distinguish evidence from belief, track change, and influence future behavior.

> **Memory is the mechanism by which the past participates in future computation.**

Consider the difference between an office moving from Building A to Building B and a serial number being corrected from AX-19 to AX-91. The first records a change in the world; the second corrects an earlier mistake. Keeping only the latest text loses that distinction.

The same applies to learning from action. An agent needs to preserve the goal, attempted action, observations, diagnosis, recovery, and outcome. The memory model distinguishes three related concepts:

- **Knowledge** captures what tends to be true.
- **Experience** preserves the process and conditions behind an outcome.
- **Skill** turns lessons from experience into reusable behavior.

A memory system should preserve provenance and uncertainty, revise beliefs when evidence changes, and distinguish confidence in a claim from how readily that claim is recalled. It also needs ways to consolidate, forget, and transfer experience without applying an old lesson to the wrong situation.

### Open components

Anda approaches this problem through components that can be adopted independently:

| Component                                                                  | Role                                                                                                                                            |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| **[KIP — Knowledge Interaction Protocol](https://github.com/ldclabs/KIP)** | A shared model for persistent cognitive state, including propositions, beliefs, evidence, temporal validity, provenance, experience, and skill. |
| **[Agent Protocols](https://github.com/ldclabs/agent-protocols)**          | Verifiable agent identity, capability descriptions, and authenticated interaction across systems.                                               |
| **[Anda Runtime](https://github.com/ldclabs/anda)**                        | A composable Rust foundation for agents, tools, skills, orchestration, and privacy-oriented execution.                                          |
| **[Anda DB](https://github.com/ldclabs/anda-db)**                          | Memory infrastructure combining structured, semantic, full-text, and graph-based access.                                                        |
| **[MIB — Memory Intelligence Benchmark](https://github.com/ldclabs/MIB)**  | An open benchmark for whether memory improves future cognition and behavior.                                                                    |

Identity establishes who an agent is and who controls it; memory preserves its cognitive continuity. Both should remain usable beyond a single platform.

MIB evaluates retrieval, temporal understanding, evidence and uncertainty, experience, skill transfer, and the causal effect of memory on behavior. Its central question is whether relevant history improves later decisions while stale or harmful memories are resisted. It is architecture-neutral: participation does not require KIP or Anda.

Protocols, implementations, and evaluation should remain separable so that others can build alternatives and test the underlying ideas.

## 3. TokenList — Sovereign Markets

[TokenList](https://tokenlist.ing/) builds infrastructure for **verifiable crypto capital formation**: the process through which an asset is created, distributed, traded, and potentially governed by its community.

Token creation is only the first step. A functioning economy also needs credible allocation, price discovery, liquidity, and a clear account of how shared capital and authority will be used.

TokenList's long-term scope follows this lifecycle:

**Create → Issue → Discover a price → Form a market → Coordinate → Govern**

Participants should be able to inspect the asset and supply being offered, the participation and allocation rules, how prices are determined, where proceeds go, and whether execution matches those rules. As ownership becomes distributed, a community may also need ways to govern its treasury and evolve its infrastructure.

### Mechanisms that can be examined and improved

Continuous Clearing Auctions (CCA) are one mechanism relevant to this direction. They allow demand to contribute to price discovery over time under defined rules. Their value depends on their design, including how timing, information, and allocation affect participants.

TokenList should remain open to different mechanisms as evidence and requirements evolve. The principle is to make the rules of capital formation inspectable and their execution verifiable.

Verifiability alone does not establish fairness, economic viability, or legal compliance. Issuers and participants remain responsible for applicable requirements, and applications may need appropriate controls. Transparent execution gives participants better evidence with which to assess a market.

## 4. dMsg — Personal Control

[dMsg](https://dmsg.net/) is evolving from encrypted messaging into a **Chrome extension for private work and cross-application signing**. The new design is defined; implementation, deployment, audits, and release validation remain to be completed.

The planned workspace starts with individual needs and extends into collaboration:

- **Personal vault:** encrypted notes, credentials, key material, and files, with local search and encrypted export.
- **Encrypted collaboration:** channels for messages and files with explicit membership, publishing roles, and access to history.
- **Contact rules:** contacts, invitations, blocking, and optional token payments for unsolicited requests.
- **Identity and signing:** a stable identity and explicit approval of signatures for files, statements, and authorizations.

The extension is the full workspace, with encrypted synchronization between authorized devices. The website is limited to public profiles, signature verification, and installation or access guidance.

### Architecture and trust

The design separates three responsibilities: the **extension** handles plaintext, encryption, decryption, and signing confirmation; **Cloudflare** stores ciphertext and runs everyday collaboration and delivery; **ICP** maintains identity and recovery roots, device control, formal signing authority, name ownership, and payment settlement.

Ordinary messages and channel updates do not require a separate blockchain update or threshold signature. Signed control records let clients verify who authorized changes to shared content.

Content keys stay with authorized endpoints, while Cloudflare sees the metadata needed to operate the service. Signatures authenticate control events but cannot guarantee that the cloud presents the latest history or revocations. Availability, extension updates, and canister upgrade authority remain trust dependencies. The publication model makes the client, protocols, canisters, and recovery tools open source; the production cloud implementation remains private.

### Authority, recovery, and continuity

The authorization model separates signing in, reading shared content, managing devices, and approving a formal signature. Application connections grant only selected capabilities; formal signing requires explicit confirmation of the source, purpose, and payload.

The planned TokenList integration signs a fixed file statement and returns independently verifiable evidence. TokenList retains its own project and publishing permissions: a signature alone cannot establish business authority or the truth of the content.

Independent content recovery requires a complete encrypted backup and separately held recovery material. Login recovery, content recovery, and threshold-key control remain distinct. Device revocation blocks new sensitive authorizations. Protecting future content requires the relevant key rotations to complete; previously received data cannot be erased remotely.

In the planned payment flow, an incoming-request fee buys acceptance without promising a read or reply. On-chain escrow governs settlement and deadline refunds, including refunds without cloud-operator approval when settlement has not been authorized.

The service model uses storage, traffic, signing, and platform fees. Basic use requires no paid handle, PANDA spending, or DMSG mining. Earlier data and token-related rights require explicit migration and governance treatment.

## 5. The DAO and PANDA

ICPanda began in meme culture. A recognizable panda brought people together; SNS governance gave that community a way to direct shared resources. Work on messaging, storage, cryptography, agents, and token issuance gradually shaped the mission described here.

The institution's lasting purpose is to learn, allocate resources, and build. Individual products can evolve or end while their code, protocols, and lessons remain available to others.

### Governance and coordination

PANDA is ICPanda DAO's governance and coordination asset. It serves three related functions:

- **Govern:** holders can lock PANDA in SNS neurons and participate in proposals under the applicable voting rules.
- **Allocate:** governance directs DAO-controlled resources toward research, engineering, security, operations, and other approved work.
- **Coordinate:** the community can align builders, contributors, users, and partners around shared objectives.

The Internet Computer's **Service Nervous System (SNS)** provides the on-chain governance framework. Its authority applies to resources and infrastructure under its control; association with ICPanda DAO does not by itself place every project component under SNS control.

Leadership and expertise remain necessary. Builders need room to make technical and operational decisions, while authority over shared resources should be visible and accountable. Token voting alone cannot guarantee decentralization or sound judgment.

Additional PANDA utility should arise where it improves a real system. Projects should not be required to manufacture token demand.

### Building principles

- **Open by default.** Publish useful code and protocols so others can inspect, extend, and build upon them.
- **Prefer evidence.** Make claims verifiable where practical and disclose the trust that remains.
- **Preserve choice.** Favor portability and interoperability so people can move their data, identity, and relationships between systems.
- **Use resources responsibly.** Review experiments on their merits, stop work that no longer justifies its cost, and preserve reusable knowledge.

These principles apply to the builder as well as its products. Progressive openness requires public development, transparent treasury decisions, clear responsibility, and governance that has real consequences.

## 6. PANDA Tokenomics

PANDA launched through the Internet Computer SNS on **April 3, 2024**, with a genesis supply of **1,000,000,000 PANDA**.

### Genesis allocation

| Allocation       |    Share |             PANDA |
| ---------------- | -------: | ----------------: |
| Development team |       4% |        40,000,000 |
| Seed funders     |       4% |        40,000,000 |
| SNS swap         |      12% |       120,000,000 |
| DAO treasury     |      80% |       800,000,000 |
| **Total**        | **100%** | **1,000,000,000** |

Allocating 80% to the treasury placed most of the genesis supply under community governance.

The original treasury plan assigned 50% of genesis supply to community distribution through the Lucky Pool, and 10% each to community incentives, CEX incentives, and DEX liquidity. A later holder airdrop used part of the community distribution allocation through SNS governance.

These figures describe historical allocation and intent, not current ownership or treasury balances.

### Governance rewards and supply

The SNS reward parameters checked on **September 7, 2026** are:

| Parameter                      |   Value |
| ------------------------------ | ------: |
| Initial annualized reward rate |      8% |
| Final annualized reward rate   |      4% |
| Transition duration            | 2 years |

The SNS schedule uses a quadratic transition between the initial and final rates. After the transition, the rate remains at 4% unless changed through governance. These are governable parameters, not immutable monetary constants. The [SNS settings reference](https://docs.internetcomputer.org/references/sns-settings/#voting-reward-settings) describes the mechanism.

Voting rewards accrue first as neuron maturity. New PANDA is minted when maturity is converted into tokens. The reward rate therefore describes the reward pool, not a guaranteed return to each holder or a fixed rate of supply growth. Actual supply depends on maturity conversion, other ledger activity, and governance decisions.

**PANDA has no fixed maximum supply under the current reward parameters.**

Total supply was approximately **1.0771 billion PANDA on September 7, 2026**. This is a dated snapshot, distinct from genesis supply and circulating supply. For current supply, balances, neurons, proposals, and parameters, consult the [ICPanda SNS dashboard](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai) and its underlying ledger and governance canisters.

## 7. The Long Horizon

Our long-term work centers on three questions:

1. **Persistent cognition:** can an agent retain and revise useful experience across time, models, and runtimes—and can we show that this improves its behavior?
2. **Verifiable capital formation:** can participants inspect and verify more of an asset's lifecycle, from issuance and price discovery to treasury formation and governance?
3. **Human–agent coordination:** can people delegate work and economic actions to software while retaining explicit authority and an inspectable record of decisions?

These questions explain why the projects belong together. A DAO might use agents to research proposals, preserve evidence, and assist with authorized workflows. That requires memory and identity, verifiable economic rules, and clear permissions for the people involved. Anda, TokenList, and dMsg address complementary parts of those requirements.

This is a direction for research and integration, not a claim that a complete system already exists. Open interfaces should let other projects participate without requiring adoption of the entire ICPanda stack.

Specific roadmaps and implementations will change. Progress should be judged by useful infrastructure, evidence that it works, and the control it gives its users and communities.

## 8. Governance of This Whitepaper

This whitepaper defines the DAO's mission, principles, PANDA's institutional role, and long-term direction. Product specifications, pricing, implementation details, and operational roadmaps belong in their respective project documents.

Material changes to the mission, PANDA's role, foundational principles, or this thesis should normally go through SNS governance. New substantive versions should preserve earlier versions and record what changed. Editorial corrections, source updates, and factual clarifications can be made without treating them as a new institutional mandate.

For supply, balances, treasury movements, proposals, and governance parameters, canonical on-chain state takes precedence over this document. Current technical specifications belong in their authoritative repositories.

The [2024 Genesis Whitepaper](2024/en.md) remains a historical record of the DAO's original assumptions and plans. Its preservation allows the community to examine what changed and what it learned.

## Sources

- [ICPanda DAO](https://panda.fans/) — organization and project overview.
- [ICPanda SNS](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai) — governance and on-chain state.
- [Anda](https://anda.ai/) — persistent-cognition infrastructure; component repositories are linked in Section 2.
- [TokenList](https://tokenlist.ing/) — capital-formation infrastructure.
- [dMsg](https://dmsg.net/) — private workspace and identity signing.
- [LDC Labs](https://github.com/ldclabs) — open-source repositories and contributors.

## Disclaimer

This document describes ICPanda DAO's direction and associated initiatives. It is not an offer to sell securities or other financial instruments, or investment, financial, legal, or tax advice. It makes no promises about token value, financial returns, or delivery of a particular project or feature.

Future directions are statements of intent. Crypto assets, AI, decentralized systems, and DAO governance involve technical, market, governance, legal, regulatory, and operational risks. Participants are responsible for their own research and compliance with applicable laws.

## Version Record

| Version                       | Date                                   | Status                                                                                                                    |
| ----------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| [Genesis Edition](2024/en.md) | 2024                                   | Historical                                                                                                                |
| v1.0                          | August 2026; revised September 7, 2026 | Adopted through [SNS Proposal #504](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/504) on September 8, 2026; editorial restructuring, clarified dMsg design scope, and updated on-chain snapshot |

Adoption record: [SNS Proposal #504](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/504) was adopted and executed on September 8, 2026, at 07:09:15 UTC. The adopted English text is preserved at [commit 297519a](https://github.com/ldclabs/ic-panda/blob/297519a743209e1d1c016aff22f2d0e960c3b380/whitepaper/en.md).
