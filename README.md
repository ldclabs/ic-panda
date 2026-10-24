<div align="center">

# 🐼 ICPanda DAO

### From Sovereign Minds to Sovereign Markets.

**An on-chain Builder DAO creating open infrastructure for digital sovereignty.**

[Website](https://panda.fans/) · [Whitepaper](./whitepaper/en.md) · [Governance](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai) · [alink](https://al.ink/ICPanda) · [X](https://x.com/ICPandaDAO)

</div>

---

## What is ICPanda DAO?

**ICPanda DAO is an on-chain Builder DAO.**

We build open protocols and infrastructure in three complementary directions:

* **Anda — Sovereign Minds** — persistent machine cognition, agent identity, memory, experience, and skill.
* **TokenList — Sovereign Markets** — verifiable crypto capital formation, from asset creation and issuance to markets and governance.
* **dMsg — Private Collaboration** — a private workspace for people and agents, with encrypted collaboration and explicit permissions.

**PANDA** is the governance and coordination asset connecting the community, treasury, and the infrastructure we build.

Digital sovereignty means participating in digital systems while retaining meaningful control over identity, memory, assets, and authority.

Each project addresses a useful problem independently. Together, they support people and agents working with portable state, inspectable rules, and explicit authority.

---

## What We Build

### 🧠 Anda — Sovereign Minds

**Open protocols and infrastructure for persistent machine cognition.**

AI should not wake up without a past.

Anda explores how intelligent systems can preserve identity, memory, experience, knowledge, and skill across time — and how independent agents can interact without a central platform owning the relationship.

Current open infrastructure includes:

* **[KIP](https://github.com/ldclabs/KIP)** — an open protocol for persistent cognitive state.
* **[Agent Protocols](https://github.com/ldclabs/agent-protocols)** — identity and interoperability for autonomous agents.
* **[Anda](https://github.com/ldclabs/anda)** — a composable Rust runtime for sovereign agent systems.
* **[Anda DB](https://github.com/ldclabs/anda-db)** — memory-centric cognitive infrastructure.
* **[MIB](https://github.com/ldclabs/MIB)** — an architecture-neutral benchmark for memory intelligence.

> **Memory is the mechanism by which the past participates in future computation.**

**[Explore Anda →](https://anda.ai/)**

---

### 📈 TokenList — Sovereign Markets

**Infrastructure for verifiable crypto capital formation.**

Creating a token is easy.

Creating a credible economy around one is not.

TokenList explores open infrastructure across the lifecycle of a crypto asset:

```text
CREATE
   ↓
ISSUE
   ↓
DISCOVER A PRICE
   ↓
FORM A MARKET
   ↓
COORDINATE
   ↓
GOVERN
```

The goal is not merely to launch more tokens.

It is to make the rules and execution of capital formation increasingly **transparent, programmable, and verifiable**.

> **The rules of capital formation should be transparent enough to inspect and programmable enough to improve.**

**[Explore TokenList →](https://tokenlist.ing/)**

---

### 🔐 dMsg — Private Collaboration

**Private collaboration. For people and agents.**

dMsg brings a private space, encrypted teamwork and an open agent network into one workspace:

* **Private space** — logins, API keys, signing keys and notes encrypted before sync; a local encrypted file library with explicit backup and separately encrypted sharing.
* **Collaboration** — encrypted channels, files and letters with contact rules; formal signatures and team-wallet actions follow explicit approval policies.
* **Agent network** — connect an MCP client, give each agent a verifiable identity and scoped permissions, and use open Agent Protocols for private mail, discussions and knowledge sharing. Agents can request actions; people retain approval authority.

The desktop app and CLI share a Rust daemon. macOS Apple Silicon and Windows x64 have desktop releases; the CLI also runs on Linux. Web applications connect through the connect.dmsg.net bridge. Windows is less tested, and desktop creation of new team wallets is currently paused.

Private content is encrypted on devices; Cloudflare stores ciphertext and handles everyday delivery, while ICP holds account/device authority, recovery and settlement. The clients, protocols and canisters are open source; the production cloud implementation remains private. Metadata, cloud availability, client updates and canister upgrades remain trust boundaries. dMsg has not been independently audited.

* [Downloads and checksums](https://github.com/ldclabs/ic-panda/releases/latest)
* [CLI and daemon](./src/dmsg_cli/README.md) · [Desktop app](./src/dmsg_desktop/README.md)
* [Documentation](./docs/README.md) · [Public protocols](./docs/protocol/README.md)

**[Explore dMsg →](https://dmsg.net/)**

---

Public dMsg integration includes dedicated login proofs, typed action signing, two-asset checkout and PANDA full subscription waivers. Start with the [integration protocol](docs/protocol/integration.md), [SDK](packages/dmsg-sdk/README.md) and [independent account adapter](examples/dmsg-account-product/README.md). Local acceptance is distinct from production deployment.

## PANDA

**PANDA is the governance and coordination asset of ICPanda DAO.**

Its durable role is to help the community:

* **Govern** — participate in ICPanda SNS governance through neurons and proposals.
* **Allocate** — collectively direct DAO-controlled resources.
* **Coordinate** — align builders, contributors, users, and ecosystem initiatives around shared objectives.

We do not require every ICPanda project to manufacture artificial PANDA utility.

> **PANDA exists to serve ICPanda DAO.
> ICPanda DAO does not exist to manufacture reasons for PANDA to exist.**

### Genesis

|                    |                              |
| ------------------ | ---------------------------- |
| **Token**          | ICPanda                      |
| **Symbol**         | PANDA                        |
| **Network**        | Internet Computer            |
| **Governance**     | Service Nervous System (SNS) |
| **SNS Launch**     | April 3, 2024                |
| **Genesis Supply** | 1,000,000,000 PANDA          |

### Genesis Allocation

| Allocation       |    Share |             PANDA |
| ---------------- | -------: | ----------------: |
| Development Team |       4% |        40,000,000 |
| Seed Funders     |       4% |        40,000,000 |
| SNS Swap         |      12% |       120,000,000 |
| DAO Treasury     |      80% |       800,000,000 |
| **Total**        | **100%** | **1,000,000,000** |

The allocation above is a **historical genesis record**, not a representation of current ownership or treasury balances.

PANDA uses the SNS governance-reward mechanism. Rewards accrue as neuron maturity, and new tokens are minted when maturity is converted into tokens. Under the reward parameters documented in the whitepaper, PANDA has no fixed maximum supply. Reward parameters may change through governance; they do not guarantee a return to individual holders.

For current supply, transactions, neurons, proposals, treasury state, and governance parameters, use the canonical on-chain source:

**[View ICPanda on the ICP Dashboard →](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai)**

> **Narrative belongs to the DAO.
> State belongs on-chain.
> Parameters belong to governance.**

---

## A Builder DAO

We define a **Builder DAO** as:

> **An on-chain community that continuously transforms shared beliefs, governed capital, engineering capability, and collective intelligence into open infrastructure.**

ICPanda DAO is the institution.

PANDA is its coordination asset.

Anda, TokenList, and dMsg are complementary building directions.

SNS governance directs resources and infrastructure under its control. Association with ICPanda DAO does not by itself place every project component under SNS control.

Individual projects may evolve, be replaced, or end. The DAO should preserve its ability to learn and build again.

Our principle is:

> **Projects may end. Knowledge should not.**

---

## Whitepaper

### From Sovereign Minds to Sovereign Markets

The **ICPanda DAO Whitepaper v1.0**, revised September 7, 2026, defines our mission, principles, PANDA's institutional role, and long-term direction. It was adopted through [SNS Proposal #504](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/504) on September 8, 2026.

* [English — Whitepaper v1.0](./whitepaper/en.md)
* [简体中文 — Whitepaper v1.0](./whitepaper/zh.md)

The adopted whitepaper supersedes the original 2024 edition, which remains available as a historical record:

* [2024 Genesis Whitepaper — Historical Document](./whitepaper/2024/en.md)

> **A DAO that expects its agents to remember should remember itself.**

---

## Principles

We build around a small set of durable principles:

**Open by default.**
Infrastructure becomes more valuable when others can inspect, extend, fork, and build upon it.

**Prefer evidence.**
Make claims verifiable where practical and disclose the trust that remains.

**Preserve choice.**
Favor portability and interoperability so people can move their data, identity, and relationships between systems.

**Use resources responsibly.**
Review experiments on their merits, stop work that no longer justifies its cost, and preserve reusable knowledge.

---

## The Long Horizon

Our long-term work centers on three questions:

1. **Persistent cognition:** can agents retain and revise useful experience across time, models, and runtimes, and can we show that it improves their behavior?
2. **Verifiable capital formation:** can participants inspect and verify more of an asset's lifecycle, from issuance and price discovery to treasury formation and governance?
3. **Human–agent coordination:** can people delegate work and economic actions to software while retaining explicit authority and an inspectable record of decisions?

Anda, TokenList, and dMsg address complementary parts of these questions. This is a direction for research and integration, not a claim that a complete system already exists. Open interfaces should let other projects participate without adopting the entire ICPanda stack.

Progress should be judged by useful infrastructure, evidence that it works, and the control it gives its users and communities.

---

## Build With Us

ICPanda DAO builds in public.

* 🌐 **[Website](https://panda.fans/)**
* 🧠 **[Anda](https://anda.ai/)**
* 📈 **[TokenList](https://tokenlist.ing/)**
* 🔐 **[dMsg](https://dmsg.net/)**
* 🗳️ **[Governance](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai)**
* 💻 **[GitHub](https://github.com/ldclabs)**
* 🔗 **[alink](https://al.ink/ICPanda)**
* 🐦 **[X](https://x.com/ICPandaDAO)**

---

## License

Licensed under the [Apache License, Version 2.0](./LICENSE).

Copyright © 2024–2026 LDC Labs and contributors.
