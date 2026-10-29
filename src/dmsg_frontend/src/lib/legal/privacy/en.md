# dMsg Privacy Policy

Last updated: 2026-10-29.

dMsg encrypts private content on your authorized devices. This policy covers dmsg.net, the read-only legacy web app, the desktop and iOS apps, connect.dmsg.net, the cloud service and the Internet Computer (ICP) canisters. “We” means the team operating dMsg under ICPanda DAO governance. Availability of individual features depends on the app version and service configuration.

We do not sell personal data or use your content for advertising. The website contains no advertising or analytics trackers.

## 1. Content and public information

Vault items, private messages and files are encrypted before upload. Content keys remain on authorized devices or in recovery/device envelopes that the service cannot open. Recipients can read and retain shared copies. Public profiles, avatars, handles, directory documents and information you intentionally publish are public. A report is a separate disclosure: moderators receive your reason, note, account/resource identifiers and only the message text you explicitly choose to include. Reporting does not give moderators the keys to your account.

## 2. Service metadata

The service processes account and login identifiers, device public keys, channel membership, contact and blocking rules, object sizes, request timing, IP addresses and operational request information. Certified account state, signature receipts and entitlements allow clients and services to verify authority. Ledger transactions and public or certified on-chain records generally cannot be erased.

If you turn on notifications, the notification service also processes what it needs to filter alerts before they are sent: opaque resource identifiers, which channels you muted or hid, your quiet hours and time zone, view receipts (when you actually viewed a reminder and up to which message), and which accounts are registered on the same installation. For deadline reminders about a pending account recovery, your device uploads the recovery record, including the new login identity and device public key; the service verifies it and keeps only the reminder and its execution time. The service does not receive channel names, message content or keys to decrypt them. Revoking the registration, removing the account from the device or deleting the service account clears these preferences, view receipts and installation links; delivery and retry records are kept only until they expire.

## 3. Apple services and payments

On iOS, optional notifications use Apple Push Notification service. Apple and our delivery service process an installation identifier, device token and delivery metadata. Notifications contain a generic reminder and an opaque route, not a message body, password or filename. You can disable notifications in dMsg or system settings; already delivered notifications cannot always be recalled. If several accounts receive notifications on one installation, the shared device token lets Apple and our delivery service link them; dMsg asks before you turn on notifications for another account.

App Attest processes an app-generated key identifier, attestation/assertion evidence and challenges to verify an app installation. Face ID, Touch ID and the device passcode are handled by the operating system; dMsg does not receive biometric images or your passcode.

StoreKit purchases are handled by Apple. Our verifier and commerce canister process transaction/product identifiers, an account-binding token, purchase and expiry dates, and refund/revocation status. They do not receive your payment-card details. Other dMsg clients may support ICP wallet payments and PANDA membership, with public ledger and entitlement records. Account deletion does not cancel an Apple subscription; manage subscriptions in your Apple account.

Team-wallet policies and proposals are processed by the wallet canisters; their visibility follows the wallet's settings. Blockchain RPC providers receive the addresses, transaction data and IP information needed for balance queries, fee simulation and broadcasting. Default endpoints include mempool.space, PublicNode, mainnet.base.org and api.mainnet-beta.solana.com; configured alternatives have their own privacy practices.

## 4. Local data and system integrations

The native apps keep a local encrypted workspace, keys protected by the operating system where supported, drafts, pending operations and preferences. Unlocking makes selected content available in app memory. Exported files and copied text may be retained by their destination apps.

The iOS Share extension stages only the items you select for a later account import. Password AutoFill and Files integrations contain only explicitly selected, separately encrypted snapshots. AutoFill requires independent system authentication; snapshots expire after 24 hours. Files access sessions last at most five minutes and end when dMsg locks, but the Files app and other recipients may retain copies already downloaded. Changing source items does not update an exported snapshot automatically.

Explicit background transfers store only bounded ciphertext and transfer authority in a separate device directory accessible after the first device unlock. They do not hold account signing or content keys. The original account must be unlocked to verify and finish a transfer. The system may delay or stop delivery.

The website remembers your language locally. The legacy app uses browser storage for sessions and cached data; its wallet view may request prices from ICPSwap. Removing an app or clearing its data can permanently lose content that was never synchronized or exported.

## 5. Providers and retention

Cloudflare processes cloud ciphertext and request metadata. ICP node providers and gateways operate canisters and websites. Apple distributes the iOS app and provides the optional services described above. Internet Identity, wallet providers and connected applications have their own privacy policies. We may disclose information we hold when legally required, including explicitly submitted reports; we cannot disclose plaintext that we do not possess.

Cloud ciphertext follows the account's storage and deletion rules. Service-account deletion is a tracked cleanup request: it blocks new service access, removes account-owned cloud content and public avatar copies, and reports completion only after cleanup. It does not erase other people's shared copies, local copies on other devices, public ledger history or necessary financial/anti-replay records. Unsettled transactions or owned active channels may need resolution first. Report records are retained for up to 90 days for review; delivery tokens expire unless renewed. Operational logs are kept only as needed for security, abuse prevention and troubleshooting.

## 6. Choices, rights and contact

You can export or delete content, revoke devices or browsers, block senders, report abuse, change contact rules, request service-account deletion and separately remove this device's account data. Depending on your location, you may have rights to access, correct, delete or object to processing of personal information. Contact [Support](https://dmsg.net/_/support) for requests or private vulnerability reports. We explain when encryption, retained transaction records or immutable public records limit a request.

No system is perfectly secure. App updates and canister upgrades under DAO governance remain points of trust. dMsg is not intended for children under 13 or below the applicable minimum age for using online services without parental consent. We update the date above and announce material policy changes in the app or on the website.
