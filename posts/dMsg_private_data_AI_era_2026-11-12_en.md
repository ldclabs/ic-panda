# Who Can Change the Rules?

**Private data in the AI era**

*By ICPanda DAO · November 2026*

A contract needs to reach a client. A password needs to be available on your next device. An AI assistant could help with a project, provided it can work with the right material.

Each task raises a question about access. Which copy should leave your device? Who should receive it? What should an assistant be allowed to read or do?

There is another question behind those choices: who can change the software rules that enforce them?

We are building dMsg around both questions. It is a private workspace for passwords, notes and files, with encrypted vault records across authorized devices, selected file sharing, and explicit permissions for connected AI agents. The rules that authorize its accounts and devices run on the Internet Computer, and changing them takes a public proposal to ICPanda DAO.

Our aim is to make that architecture useful in ordinary life: keeping something private, finding it on another device, sending a copy, and giving an assistant a clearly defined role.

## Start with the information you already care about

The first reason to use a private workspace can be a single document: a passport copy, a contract, a project note or a set of credentials.

Those materials have different destinations. Some should stay on one device. Some need a backup. Some need to reach another person. The boundaries should remain understandable as your work moves between them.

In dMsg, vault records sync in encrypted form across devices authorized to access the vault. The local file library works differently: you choose which file versions to back up or share. A cloud backup can then be retrieved on another authorized device. The product does not automatically mirror your entire local file library. [1]

Sharing also creates a separate encrypted edition of a file. Its key and lifecycle are separate from the library's own copy and any backup. A recipient can receive that edition through a browser without creating a dMsg account or installing an app. [2]

You can start with your own workspace and use it on its own. Collaboration and AI become useful when you have a task for them.

## Encryption and device authorization work together

End-to-end encryption protects private content between authorized endpoints. It makes the handling of keys central to the system's security.

Consider adding a new device. A product has to establish which account the device belongs to, which permissions it receives, and how it obtains the key material needed to read existing content.

These are separate steps. Adding a row to a server's device list does not, by itself, decrypt an encrypted file. The client must accept the authorization, and the new device must obtain the appropriate keys.

That distinction matters when assessing a service. Questions about a malicious or compromised backend need to follow the actual path: how public keys are verified, how devices are approved, how keys are distributed, and what recovery and software updates can change.

Existing privacy products already address parts of this problem. Bitwarden documents encryption on the user's device before vault data reaches its servers. Proton Mail documents Key Transparency checks that compare public keys with an auditable directory. These mechanisms deserve to be evaluated on their own terms. [3] [4]

Our focus with dMsg includes the authority behind account and device access, and the power to change the code enforcing it.

In the normal enrollment flow, an authorized administrator device approves the new device. To unlock the vault, the new device also needs permission to do so and the corresponding key material encrypted for that device. Clients verify certified account state, device permissions and content-root commitments against ICP evidence. A change to a cloud database alone cannot substitute for that evidence or create the required key material. [5]

Normal enrollment is one path. Recovery is another, with its own conditions. Both need to be visible in the trust model.

## Who can change the authorization rules?

Software changes over time. Security fixes, new features and protocol improvements all require a way to update it. The important question is how that power is held and exercised.

dMsg separates three responsibilities:

| Responsibility | How dMsg handles it |
| --- | --- |
| Protect private content | Authorized clients encrypt and decrypt it. |
| Enforce critical account and device authorization | ICP canisters execute the relevant rules and provide verifiable evidence. |
| Govern changes to those canisters | ICPanda DAO's SNS governs their upgrades and relevant control-plane changes. |

Canisters are programs running on the Internet Computer. In dMsg, they provide the authority for critical account and device decisions. The cloud handles encrypted content storage, delivery and other service functions. Private file contents and message bodies are not published as public blockchain content, and ordinary notes, file operations and messages do not each require a governance vote. [6]

SNS, the Service Nervous System, provides a proposal and voting process for governing the application canisters under its control. Upgrading a canister requires its controller, and dMsg's canisters have one. When we checked on November 12, 2026, each of them listed the ICPanda SNS root as its only controller: the account, directory, handle, authentication and key-derivation services, payment, commerce and membership, the wallet hub, the bridge page that connects websites to devices, and the canister that serves dmsg.net. Team wallet instances are the exception: the wallet hub controls them, and each accepts an upgrade only after its own members approve it. Proposals are voted on by holders of PANDA neurons, and each proposal and its tally stay on the public record. [7] [8]

You do not have to take that from us. The canister IDs are in the source repository, and the ICP dashboard shows the controllers of any canister, for example the [account canister](https://dashboard.internetcomputer.org/canister/wqqa6-3aaaa-aaaaj-qsjvq-cai). [11]

The practical claim is specific:

**Cloud administration access alone does not let an operator rewrite dMsg's critical account and device authorization rules.**

Here, cloud administration means operating the off-chain storage and delivery service. Those permissions do not provide an independent route to replace the governed authorization code or enroll a device with vault decryption access.

This is the Web3 property we want to make useful: a boundary around who can change a consequential set of rules.

Governance is therefore part of what you trust. An adopted proposal can upgrade a canister, and with it the rules; voting does not make software immutable or automatically safe. The account canister also holds a secret used to derive each device's local unlock key, which is useful only together with a copy of that device's data, and optional on-chain recovery relies on these canisters to release recovery keys under the published conditions. An upgrade could change either. How voting power is distributed and how carefully proposals are reviewed matter as much as the mechanism itself. [6] [12]

The vote also has an edge. The apps, their distribution and updates, the cloud service, and the ICP network are outside it and retain their own responsibilities and risks. [6] [7]

People should be able to inspect those responsibilities. They should be able to understand which components are governed, which version they use, and where other forms of trust remain.

## Give an AI assistant a role that fits the task

An assistant may need to read project material, write a draft or request an action. Each capability has a different consequence.

The useful design question is how much access a particular task requires, and where the user needs to make a decision.

dMsg gives connected agents their own identities and permissions. Its protected interfaces can distinguish reading from writing and limit access to particular services or resources. Authorized operations can run within their granted scope; requests for the user's formal signature still require approval. [9]

That lets permission design follow the work. An assistant preparing a draft needs access to the relevant material. It does not follow that it should also be able to sign the result.

Credential use has a further boundary. Reading a vault record does not automatically authorize the use of its concealed credentials. Supported credential-use requests require a separate, specific permission and approval of the complete operation. The local daemon performs the operation without returning the credential through MCP. The current documented MCP adapter covers creating an issue in a specified GitHub.com repository; broader adapters should be assessed as they become available. [10]

These controls apply to the interfaces and actions dMsg provides. If you authorize an external AI client to read content, that content also enters the client's own data-handling environment. Encryption cannot erase what an authorized recipient has already received.

Our goal is to make the user's decision concrete: what an agent can access, what it can do with that access, and which requests still require approval.

## Recovery and updates belong in the explanation

Privacy products need to explain what happens when normal use goes wrong.

For new dMsg accounts, optional on-chain recovery is off by default. It must be enabled in advance from an authorized administrator device that can still unlock the account. Once prepared, a linked Internet Identity and a waiting period, three days by default, provide a separate recovery path. That path can restore access without a final approval from an old device; any device that is still authorized can dispute the request during the waiting period. It follows that someone who takes over the linked identity can also start a recovery, which is why the dispute window exists. If every authorized device is lost and recovery was not enabled, signing in alone cannot recover your encrypted content. Recovery cannot restore local files that were never uploaded or backed up. [5]

Those conditions matter when deciding what to keep in a private workspace. A login, an approved device and access to encrypted content are related, but they are not interchangeable.

Other boundaries matter too. Service metadata such as timing, size and routing information can remain visible. The cloud can delay or fail to deliver data. A compromised device or malicious client update can undermine protections at the endpoint. [6]

Published code and governance records give people evidence to examine. They are not substitutes for independent security assessment or proof that every deployed component matches a particular source revision.

We want the product's explanations to be as precise as its permission controls.

## This article is also a file-sharing experience

If you opened this article through a dMsg link, you have used its recipient experience: access to a shared file through your browser, without a dMsg account or app installation.

The file is encrypted before upload. The recipient's browser obtains the encrypted content and decrypts it using the sharing credentials. For a password-free link, the decryption key is carried in the complete link, after the #. [2]

This article is public. Anyone with its complete share link can access it while the link remains available. Publishing the link is our choice to make this edition available to that audience.

For other files, sharing terms can include a password, an expiry or a download limit. Revoking a link stops new download authorizations; transfers already authorized may still complete, and copies already saved cannot be recalled. [1] [2]

Receiving this article gives you a way to judge the convenience of that interaction. The architecture and protocol references below explain the mechanisms behind it.

## Make the architecture earn its place in everyday use

We want dMsg to be useful to people who need private information to remain manageable as they move between devices, work with other people and use AI.

That includes people who never connect an agent. It includes people who care about governance, and people who simply want to send a file without asking the recipient to create another account.

We see broad adoption as the accumulation of those ordinary decisions: someone saves a document, uses it again, shares a selected copy, and returns because the experience works.

ICPanda DAO builds dMsg and governs its on-chain rules. We would like that role to be judged the way the product is: by what ships, by how clearly each upgrade is explained, and by whether people who check the record find what we said they would.

## Try dMsg with one file

The client code, protocols and canister implementations are published in the [source repository](https://github.com/ldclabs/ic-panda). The [architecture](https://dmsg.net/_/docs/architecture.md) explains the trust domains, and the [ICPanda DAO governance dashboard](https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai) provides a place to follow proposals and decisions.

We welcome questions about the design, evidence of gaps, and feedback from using the product.

To try dMsg, begin with one file you want to keep private. Save it, decide whether it needs a backup, and choose whether to share a separate copy. There is time to add another device or connect an assistant when you have a reason to.

**[Explore dMsg and get started](https://dmsg.net/).**

Private data moves through real tasks. We are building dMsg to make the permissions around those tasks understandable, verifiable and subject to clear rules about who can change them.

## Sources and further reading

The dMsg implementation references below are pinned to the source revision used for this article. Product releases and supported integrations can change.

1. [Encrypted storage, vault sync and file sharing][1]
2. [File-sharing keys, terms and link implementation][2]
3. [Bitwarden encryption documentation][3]
4. [Proton Mail Key Transparency documentation][4]
5. [dMsg account, device and recovery protocol][5]
6. [dMsg architecture and trust domains][6]
7. [ICP governance and the Service Nervous System][7]
8. [ICPanda DAO governance records][8]
9. [dMsg Agent connector and permissions][9]
10. [dMsg approved credential use and current adapter scope][10]
11. [dMsg mainnet canister IDs][11]
12. [dMsg encryption design and canister-held secrets][12]

[1]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/docs/protocol/cloud.md
[2]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/src/dmsg_client/src/library/link.rs
[3]: https://bitwarden.com/help/what-encryption-is-used/
[4]: https://proton.me/support/key-transparency
[5]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/docs/protocol/account_root.md
[6]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/docs/dmsg_architecture.md
[7]: https://docs.internetcomputer.org/concepts/governance/
[8]: https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai
[9]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/src/dmsg_agent/README.md
[10]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/docs/protocol/secrets.md
[11]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/.icp/data/mappings/ic.ids.json
[12]: https://github.com/ldclabs/ic-panda/blob/3b694ea4021b69e562e168887154354349ade688/docs/dmsg_encryption.md
