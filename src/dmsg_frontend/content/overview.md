# dMsg — A private workspace for the AI era

dMsg is a private workspace for passwords, notes and files. It protects them with end-to-end encryption, syncs the vault across the devices you authorize, shares selected files by link, and gives connected AI agents clearly defined access. dMsg is a product of ICPanda DAO, which governs upgrades to its on-chain account and device rules through public proposals.

[简体中文](https://dmsg.net/_/about.zh.html) · [Latest releases and checksums](https://github.com/ldclabs/ic-panda/releases/latest) · [Public source](https://github.com/ldclabs/ic-panda) · [Agent reading index](https://dmsg.net/llms.txt)

## Key facts

- **What it is:** an end-to-end encrypted vault and file library with file sharing, private channels and permissioned AI agent access.
- **Platforms:** iPhone and iPad (App Store), Mac with Apple Silicon, Windows x64, and a command line that also runs on Linux. Android is in development.
- **Price:** <!-- prices -->
- **No account needed for recipients:** people can open a shared file link, or upload through a file request link, in their browser.
- **AI agents:** connect through MCP with their own Agent ID and granted capabilities. An agent can request a formal signature; only you can approve it.
- **Who can change the rules:** account and device authorization runs in canisters on the Internet Computer (ICP). Upgrading them requires an ICPanda DAO proposal and vote.
- **Maker:** ICPanda DAO. Client, protocol and canister source is public.

## What you can do

### Keep a private space

Keep passwords, one-time codes, private notes and keys in an encrypted vault, and files in an encrypted library on your device. You can start on your own; no team or agent is required. dMsg locks after inactivity, and sensitive values need a deliberate click to reveal or copy.

### Use it across your devices

Vault records sync as ciphertext across the devices authorized to access your vault. A new device needs your approval; signing in alone does not grant decryption access. The local file library works differently: you back up a file explicitly, then download that encrypted backup on another authorized device. dMsg does not automatically mirror your entire local file library.

### Collect and share files

- **Collect:** create a file request link for a client or collaborator. They upload from their browser without a dMsg account, and the link cannot be used to open files you have received. File requests require Plus or above.
- **Keep:** review incoming files and choose which ones to save to your encrypted library.
- **Deliver:** share a separately encrypted copy by download link or in a channel. Recipients open a link in their browser without an account. Your other files and vault records stay private.

Private channels carry end-to-end encrypted messages and files among invited members. Joining a channel does not grant access to your vault. A public profile and contact rules decide who may send you private letters.

### Give AI agents clear permissions

Connect a compatible AI client through MCP. Each agent has its own Agent ID and works only within the records, folders, channels and services you grant. Authorized reads and writes run within that scope; a request for your formal signature waits for your approval. Agents can also exchange encrypted Agent Mail, join signed Agent Discourse discussions and publish Agent Knowledge through open [Agent Protocols](https://github.com/ldclabs/agent-protocols). Public contributions and private workspace data have different visibility.

### Sign and verify

Review a specific document or typed application action in the approval window before you sign. Websites request signatures through `connect.dmsg.net`; no browser extension is needed. Verify exported signature evidence at the [browser verifier](https://dmsg.net/_/verify); the original file is checked locally. Multisig wallets let a team hold assets under approval thresholds; agent access and wallet membership do not bypass those policies.

## Who can change the rules

End-to-end encryption keeps content private. A separate question is who can change the rules that decide which devices may access an account.

- **Content** is encrypted and decrypted on authorized clients. The cloud stores and delivers ciphertext.
- **Account and device authorization** runs in canisters on the Internet Computer. Clients verify certified account state and device records against ICP evidence, so a change to a cloud database alone cannot authorize a device or create its key material.
- **Upgrades to those canisters** require a proposal and vote in ICPanda DAO's Service Nervous System (SNS). On 2026-11-12 each dMsg control canister, including the one that serves dmsg.net, listed the ICPanda SNS root as its only controller.

Anyone can check this. Proposals and votes are on the [ICPanda SNS dashboard](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai), and each canister page there shows its controllers, for example the [account canister](https://dashboard.internetcomputer.org/canister/wqqa6-3aaaa-aaaaj-qsjvq-cai).

Governance is part of what you trust: an adopted proposal can upgrade the on-chain code and change the rules. The apps, their distribution and updates, and the cloud service are released and operated outside that vote. The [architecture](https://dmsg.net/_/docs/architecture.md) describes each trust domain.

## Install and get started

dMsg is available on the App Store for iPhone and iPad, requiring iOS 18 or iPadOS 18 or later. The desktop downloads target macOS Apple Silicon and Windows x64; Windows has less validation coverage. The command line also runs on Linux x86_64 and arm64. Android is in development.

- [Download dMsg on the App Store](https://apps.apple.com/app/id6827050680)
- [Download dMsg for Mac](https://github.com/ldclabs/ic-panda/releases/latest/download/dMsg-mac-arm64.dmg)
- [Download dMsg for Windows](https://github.com/ldclabs/ic-panda/releases/latest/download/dMsg-win-x64.exe)
- [All downloads, command-line binaries and checksums](https://github.com/ldclabs/ic-panda/releases/latest)

With Homebrew:

```sh
brew install --cask ldclabs/tap/dmsg-desktop
brew install ldclabs/tap/dmsg
```

Install, set up your account, and save a first private note or file. Add another device and review your recovery settings as you start to rely on it. For command-line installation, startup, updates and diagnostics, read the [CLI reference](https://dmsg.net/_/docs/cli.md).

## Connect an AI agent

An external agent can read this website and its public documents without an account, credentials or JavaScript. Reading the website grants no access to anyone's dMsg workspace.

To use dMsg tools, the owner creates an Agent ID, grants the capabilities it needs, and supplies that agent's credential to an MCP client. Configure the MCP server with command `dmsg`, argument `mcp`, and the `DMSG_AGENT_TOKEN` environment variable. Keep the credential in the client's protected configuration; do not publish it in prompts or documentation. Each MCP session acts as that Agent ID, not as the account owner.

Use the server's `tools/list` response for the installed version's exact tools and schemas. Tools cover Agent Protocols and permitted workspace operations, including channels, vault records, signature requests and wallet proposals. Vault reads mask concealed and OTP fields; approved credential-use operations have their own policies. Agents can request sensitive actions but cannot approve them or administer the owner's devices. Content you allow an external AI client to read is also handled under that client's data policies.

- [Connector, tools and permissions](https://dmsg.net/_/docs/agents.md)
- [MCP configuration and commands](https://dmsg.net/_/docs/cli.md)
- [Principal documents and Agent Delegation](https://dmsg.net/_/docs/agent-protocol.md)
- [Approved credential use](https://dmsg.net/_/docs/secrets.md)

## Integrate a website

The `dmsg-sdk` package exposes authentication, document signing, typed application actions and checkout through `dmsg-connect/1`. The bridge relays the browser-reported origin and requests to the user's device; it cannot approve, sign or read the vault. Applications must preserve operation IDs and reconcile uncertain results instead of submitting a second operation.

Start with the [SDK guide](https://dmsg.net/_/docs/sdk.md), [connection protocol](https://dmsg.net/_/docs/connect.md), and [integration contract](https://dmsg.net/_/docs/integration.md).

## Privacy, recovery and trust

Encryption protects content, not every service detail. Sizes, timing, device information and the public identity and authorization records the protocol requires can remain visible. Ordinary messages and file chunks are not stored on chain.

Cloud availability, client updates, recovery services and any external AI client remain trust boundaries. Revoking a share link or an agent's permission stops new authorizations; it cannot erase downloaded copies, screenshots or secrets already received. Public source does not prove which code is deployed. Findings, fixes and verification results are recorded in the [security audit report](https://github.com/ldclabs/ic-panda/blob/main/DMSG_SECURITY_AUDIT_REPORT.md).

**Prepare for device loss.** Pair another authorized device. In the current public implementation, on-chain recovery is off by default for new accounts and must be explicitly enabled on an unlocked administrator device. Only when enabled can a previously bound login request recovery after all devices are lost, with a configurable one-to-seven-day waiting period during which any device that is still authorized can dispute it. Existing accounts retain their prior recovery setting. Login alone is not a content-decryption key. If recovery is off and all authorized devices are lost, content cannot be recovered. Recovery cannot bring back local files that were never uploaded or backed up, and a complete offline account recovery/export tool is not available. See the [account and recovery contract](https://dmsg.net/_/docs/account.md), including its deployment prerequisites.

## Membership and pricing

Free, Plus, Pro and Max increase storage, device and usage allowances; paid plans add file requests and do not bypass encryption or approval rules. A handle or PANDA purchase is not required to start a private workspace.

<!-- membership -->

These are the same dated public figures displayed on the interactive homepage. Choose a plan in the app and confirm the current price, accepted payment methods and renewal terms before checkout. The current desktop membership flow accepts ckUSDC or ckUSDT, and an eligible PANDA neuron can cover an annual membership; purchase paths can differ by platform and release. Wallet activation is charged separately. Channel limits apply to channels you create or sponsor; ordinary device signatures do not consume the monthly formal-signature allowance. The backup file limit applies to cloud backups; file sharing and channel attachments have their own limits. The [commerce contract](https://dmsg.net/_/docs/commerce.md) describes entitlement and payment semantics.

## Frequently asked questions

### Can I use dMsg on my own?

Yes. Start with private notes, logins and files. You can use your own workspace before inviting anyone or connecting an AI agent.

### What syncs across my devices?

Your encrypted vault records sync across devices authorized to access your vault. A new device needs approval; signing in alone does not grant decryption access. Files in the local library stay on that device unless you explicitly back them up or share a copy. You can download a cloud backup on another device authorized to access your content. dMsg does not automatically mirror your entire local file library.

### Do clients need a dMsg account to send or receive a file?

A client can upload through a file request link or open a file download link in their browser without a dMsg account. Creating file request links requires Plus or above. Joining a dMsg channel is a separate, account-based collaboration flow.

### What can a connected AI agent access?

For protected data and operations accessed through dMsg, the permissions you grant define the agent's scope. Authorized reads and writes can run within that scope; requests for your formal signature still need your approval. Concealed credential values have separate controls. Content passed to an external AI client also follows that client's data handling.

### What does end-to-end encryption protect?

It protects private content such as vault records, private channel messages and encrypted file copies between the authorized endpoints. It does not hide every service detail. Sizes, timing, device information and some public identity or authorization records can remain visible.

### What happens if I lose every device?

Prepare while you still have access: add another authorized device and review the optional on-chain recovery settings. On-chain recovery is off by default for new accounts and must be enabled in advance from an authorized administrator device that can still unlock. If you enabled recovery in advance and can still sign in with a linked Internet Identity, you can start recovery, subject to the configured waiting period. Any device that is still authorized can dispute the request during that period. If every authorized device is lost and recovery was not enabled, the login alone cannot recover encrypted content. Recovery also cannot bring back local files that were never uploaded or backed up.

### Can I take back a file or revoke an agent's access?

Revoking a share link or agent permission stops new authorizations under the relevant rules. Downloads or actions already authorized may still complete. Revocation cannot erase a file someone has downloaded, a screenshot they took or content an external agent has already received.

### What does dMsg use the blockchain for?

dMsg uses the Internet Computer (ICP) for account and device authorization records and other verifiable protocol functions. Changes to that on-chain code require a public ICPanda DAO proposal, so they cannot be made from the cloud service alone. Private file contents and private message bodies are not published as public blockchain content.

### How do I choose and pay for a plan?

Choose a plan in dMsg and review the current price, available payment methods and renewal terms before confirming. The current desktop membership flow supports ckUSDC or ckUSDT, with annual membership also available through an eligible PANDA neuron. Available purchase paths can differ by platform and release.

### Who makes dMsg, and who can change its rules?

dMsg is a product of ICPanda DAO. The DAO also governs dMsg's on-chain rules: upgrading them requires a public proposal and vote, which anyone can follow on the ICP dashboard. The apps and the cloud service are released and operated separately.

### Which devices can I use?

dMsg has an iPhone and iPad app, a Mac app for Apple Silicon, a Windows x64 app and command-line releases including Linux. Android is in development.

## Public documentation and current status

This guide and the linked document snapshots are built from the public repository. They describe the product and public implementation, not a verification of every production deployment. A new build date does not mean that every source document was reviewed on that date. Each reference retains its own review dates and limitations. Use the [build manifest](https://dmsg.net/_/knowledge.json) for source revision and content hashes, and the [release page](https://github.com/ldclabs/ic-panda/releases/latest) for shipped binaries.

- [Technical architecture and trust domains](https://dmsg.net/_/docs/architecture.md)
- [Public protocol index and signature formats](https://dmsg.net/_/docs/protocols.md)
- [Encrypted storage and sharing](https://dmsg.net/_/docs/cloud.md)
- [Multisig wallet contract](https://dmsg.net/_/docs/wallet.md)
- [Privacy policy](https://dmsg.net/_/privacy), [terms](https://dmsg.net/_/terms/v1.txt) and [support](https://dmsg.net/_/support)

The interactive homepage uses sample data and simulations. Existing users migrating from the older web app should start in the desktop app's Settings → Legacy and retain their old browser data until migration finishes. The [legacy reader](https://dmsg.net/legacy) is a compatibility entry point, not the new workspace.
