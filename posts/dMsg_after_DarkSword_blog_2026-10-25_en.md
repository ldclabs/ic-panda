# After DarkSword: Why dMsg Doesn't Trust the Keychain Alone

[中文版](dMsg_after_DarkSword_blog_2026-10-25_zh.md)

*October 25, 2026 · How dMsg protects local data and sensitive approvals—and where those protections end. This describes the implementation in this repository; availability depends on the installed release and device.*

## A web page can become a way into your device

In March 2026, Google's Threat Intelligence Group described **DarkSword**, an iPhone exploit chain targeting iOS 18.4–18.7. Visiting a malicious or compromised website could lead to device compromise. Its GHOSTBLADE payload collected sensitive data, including keychain and messaging data. Google reported that all vulnerabilities in the chain had been patched by iOS 26.3 and urged users to update. [Google's investigation](https://cloud.google.com/blog/topics/threat-intelligence/darksword-ios-exploit-chain)

Macs face related threats. Sophos documented how Atomic macOS Stealer tricks people into supplying their login password and steals sensitive files, including keychain data. [Sophos's investigation](https://www.sophos.com/en-us/blog/atomic-macos-stealer-leads-sensitive-data-theft-on-macos)

That does **not** mean every keychain item becomes readable after any compromise. Protection depends on the platform, access controls and the attack. Apple's Secure Enclave provides a separate hardware boundary, designed to protect sensitive operations even when the main processor's software is compromised. [Apple Platform Security](https://support.apple.com/guide/security/secure-enclave-sec59b0b31ff/web)

The practical question for dMsg is: **if someone copies the local workspace and credential store, what else would they need?**

## What dMsg is

dMsg is one of ICPanda DAO's products: a private workspace with a vault, encrypted files, channels and letters, signatures, team wallets and scoped access for AI agents. Its client, protocols and canisters are open source. These protections are not a claim of independent security certification or proof that dMsg resists a particular exploit chain.

## Layer 1: Encrypt before data leaves the device

Vault items and file versions use separate keys. The account's content root protects the keys needed to read its private data; device-specific envelopes distribute that root using HPKE with X25519. Content encryption uses AES-256-GCM. Files are split into approximately 1 MiB chunks and checked during download. Channels have their own epoch keys.

The cloud stores encrypted content and delivers it to clients. Internet Computer canisters maintain account authority, devices, recovery and relevant execution policies. In the intended protocol, neither service receives plaintext content roots.

Content encryption does not hide all metadata. Routing relationships, sizes, timing and authorization requests remain visible where needed. Formal signing deliberately exposes the authorization data and commitments required for verification. See the [encryption design and its boundaries](../docs/dmsg_encryption.md).

## Layer 2: No additional dMsg password or recovery phrase

You sign in with Internet Identity (II). dMsg does not add its own password or seed phrase. **II itself still has authentication and recovery mechanisms that you must protect.** A compromised II remains a serious threat, as Layer 6 explains.

For login unlock, the account canister releases an unlock secret only to a bound login and an eligible registered device. The client uses it to open the local data key. This is different from returning all your content to anyone who can sign in.

By default, dMsg locks after 15 minutes without activity and releases its content keys. It does not promise forensic erasure of process memory. Explicit background-online and resident-agent options retain limited credentials; they do not retain ordinary access to the locked vault.

## Layer 3: Fast unlock needs the original Secure Enclave

On supported Macs, fast unlock uses a secret derived through a Secure Enclave key agreement. The system credential store holds a wrapped key representation and supporting public material, rather than an exportable plaintext private key.

Copying that material to another Mac does not reproduce the original hardware key. Using it on the original Mac requires Touch ID **or the Mac login password** under the default policy. A password fallback is still a fallback; fast unlock alone is not protection against someone who can satisfy it on the original device.

## Layer 4: Device-bound unlock protects both local entry paths

Optional **device-bound unlock** also makes login unlock require the original Mac's hardware secret. Fast unlock uses that hardware secret too. Both local paths require user presence, and an unavailable hardware key does not silently fall back to login-only wrapping.

This protects a current workspace copied together with its credential store. It does not close the separate cloud recovery path. It also cannot repair an earlier leak: binding rewraps the existing local data key, so an older unbound copy or a previously extracted key may still compromise data protected by that key.

The optional **Touch ID only** policy excludes the password fallback and binds the key to the current enrolled biometric set. Changing that set can invalidate the key. Keep another authorized device or a working recovery route before enabling it.

## Layer 5: Approval keys add a signature the server must check

Every device already signs with a software key. You can also register a Secure Enclave **approval key** and require it for supported account-control, device-control and principal-control operations. Optional policy categories extend it to covered payments and attestations.

The extra signature is bound to the specific approval, including its operation commitment and context. The user home verifies it before accepting an operation subject to the policy. Copying only the software device key and login is insufficient for those operations.

This is not a blanket gate over every external wallet transaction or agent action. A valid P-256 signature also does not, by itself, remotely prove Apple's hardware or access-control settings. And a presence prompt does not give a compromised screen a trusted transaction display.

## Layer 6: Revoke devices—and watch the recovery path

From another administrator device, you can revoke a lost device. It loses eligibility for new unlock secrets and relevant approvals; its approval key retires. Where required, clients rotate content keys and exclude it from new envelopes. A cooperating dMsg client that detects its revocation resets its workspace. None of this retrieves copies an attacker already made or guarantees a remote wipe of an offline machine.

If **all devices are lost**, a bound II login can request recovery. After the waiting period—three days by default, configurable from one to seven—the new device replaces the old devices and login bindings. Old approval keys retire and the hardware approval policy is cleared.

How can it recover encrypted content? Each content-root generation includes an encrypted recovery envelope. The canister authorizes the completed recovery device to obtain a vetKD-derived decryption key for the current generation, encrypted to that device's transport key. The device opens the envelope, recovers the root and then rotates it. Recoverable history depends on the cloud ciphertext and key history still being available; files never backed up from a lost machine are not recovered.

**An attacker who controls your bound II can use this same recovery path.** If no valid device cancels before completion, the attacker can take over the account and recover the backed-up content. Hardware approval keys deliberately do not veto this last-resort path.

The waiting period therefore needs attention, not just a timer. The daemon now checks the **selected account's** certified public recovery status in the background, including while locked, with a check about once a minute when ordinary security reads have not refreshed it. The desktop shows a system notification for a pending request. Clicking opens **Settings → Account security**, where the existing cancellation approval is available after unlocking/signing in. `dmsg status` also warns; `dmsg recovery dispute` starts cancellation from the CLI.

A request notifies once per desktop run; reconnecting does not flood you with repeats, while a new request can notify again. A cached warning survives an outage and daemon restart. Unknown or stale CLI status is labelled as such.

These are local alerts, **not guaranteed push delivery**. The daemon and desktop must be running, the device must be online, and the OS must allow notifications. Sleep, network failure or an unselected account can delay detection. Cancelling one attempt does not repair a compromised II: remove unauthorized authentication and recovery methods too.

## What this doesn't protect against

- **An unlocked, compromised client.** Malware may read plaintext or keys in memory and misuse already granted access. Secure Enclave does not protect all application memory.
- **A malicious or misleading approval screen.** Check the action and the prompt. High-value decisions may need another device or person.
- **Past disclosure.** Binding and revocation do not withdraw copied secrets, exported files or previously issued credentials.
- **Disabled or unavailable protection.** Device binding and approval-key requirements are opt-in. These Secure Enclave integrations currently target macOS; other platforms use their available credential storage.
- **Service and software trust.** Recovery needs cloud data and the relevant services. Metadata, client distribution, canister upgrades and notification delivery remain trust boundaries.

## A five-minute checklist

1. Update the operating system. For users at elevated risk, consider Apple's [Lockdown Mode](https://support.apple.com/en-us/105120), alongside updates.
2. In **Settings → Account security**, bind a supported Mac and register approval keys before requiring them.
3. Consider **Touch ID only**, with a plan for biometric changes or hardware failure.
4. Keep another valid device, leave dMsg running on it, and allow notifications. Review unexpected recovery requests promptly.
5. Protect II's own authentication and recovery methods. After an unauthorized request, secure II as well as cancelling the dMsg recovery.

The aim is to make copied storage and stolen software keys insufficient for more operations, while making the remaining recovery authority explicit and visible.

- Learn more: [dmsg.net](https://dmsg.net/)
- Technical detail: [dMsg encryption design](../docs/dmsg_encryption.md) · [Account and recovery protocol](../docs/protocol/account_root.md)
- Source: [ICPanda DAO / ic-panda](https://github.com/ldclabs/ic-panda)
