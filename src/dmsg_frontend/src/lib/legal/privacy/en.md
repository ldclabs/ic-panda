# dMsg Privacy Policy

Last updated: 2026-10-11.

dMsg is designed so that your private content is encrypted on your own devices. This policy explains what the dMsg website, the legacy web app, the browser extension, the cloud service and the canisters on the Internet Computer (ICP) handle, who can see it, and the choices you have.

We do not sell personal data, we do not use your content for advertising, and the website contains no analytics or advertising trackers.

## 1. Scope

This policy covers dmsg.net, the read-only legacy web app, the dMsg browser extension, the dMsg cloud service and the dMsg canisters. "We" means the team that operates dMsg under the governance of ICPanda DAO. Wallets, Internet Identity, token ledgers and applications you connect have their own privacy practices.

## 2. Private content

- Notes, messages, files and vault items are encrypted on your approved and unlocked devices before they are uploaded. The cloud stores and delivers only ciphertext.
- Content keys stay on your devices, wrapped for each device you approve. For recovery, the account also keeps an envelope that can be opened only through the ICP vetKD service under the account's recovery rules.
- We do not receive your plaintext or your content keys.
- People you share content with can read it and can keep copies.

## 3. Information that is not encrypted

Encryption protects content, not every relationship or action. To run the service, the cloud service and the canisters process:

- account IDs, login principals, device public keys and device identifiers;
- handles, public profiles and the contact rules you set;
- channel membership and the contact relations needed to enforce your rules;
- the size and timing of objects, messages and requests;
- IP addresses and request details that reach the cloud service;
- order, payment, refund and entitlement records.

## 4. Public and on-chain information

Some information is public by design:

- handles and the accounts that hold them;
- directory documents served at id.dmsg.net, including their controllers, delegation service and profile link;
- profile details you choose to publish;
- transactions on ICP token ledgers, including payments to and refunds from dMsg.

Account state that other services must verify, such as plan entitlements and receipts for formal signatures, is published as certified data. Records written on ICP generally cannot be erased.

## 5. Payments and PANDA membership

You pay from your own wallet on ICP ledgers such as ckUSDC and PANDA. The dMsg commerce, payment and handle canisters keep order, escrow, deposit, refund and receipt records, including the paying account and the amounts. If you apply for a PANDA waiver, the membership canister reads your neuron's stake, dissolve state and listed principals from the PANDA SNS.

## 6. Data on your device

- The extension keeps your keys, a local encrypted database, unsent changes, sync state and settings in the browser.
- The website stores your language choice in local storage.
- The legacy app keeps your login session and cached data in the browser's IndexedDB, and its service worker caches the site's files. Its wallet view requests token prices from the ICPSwap API.

Removing the extension or clearing site data deletes these local copies. Content that was not synced to the cloud is lost with them.

## 7. Service providers

We use the following providers only as needed to run dMsg:

- Cloudflare runs the cloud service and processes the ciphertext and request information sent to it.
- ICP node providers run the canisters, and HTTP gateways serve dmsg.net and id.dmsg.net. They may keep standard request logs.
- Internet Identity and the wallets you choose handle your login and your payments.
- The Chrome Web Store distributes the extension.

We may disclose information when the law requires it. Because content is encrypted, we can disclose only the ciphertext and metadata we hold.

## 8. Retention

- Ciphertext stays in the cloud while your account uses it and as your plan allows. When you delete an item, its ciphertext is removed under the rules shown in the extension.
- When a paid plan ends, the Terms of Service describe what happens to content above the Free limits.
- Operational logs are kept only as long as needed for security, abuse prevention and troubleshooting.
- Handles, ledger transactions, signature receipts and other records written on ICP are kept by the canisters and ledgers and generally cannot be deleted.

## 9. Your choices and rights

You can export your data, delete items, revoke devices and change or close your contact rules in the extension. Depending on where you live, you may have rights to access, correct or delete your personal information, or to object to its use. Contact us through the support page. Because we cannot decrypt your content and cannot erase on-chain records, some requests can be met only in part.

## 10. Security and its limits

dMsg uses device encryption, separate approvals for login, devices and signatures, and certified data that clients can verify. No system is perfectly secure. The dMsg canisters are controlled by ICPanda DAO governance, which can upgrade them and change their rules, and extension updates are another point of trust. Please report vulnerabilities privately through the support page.

## 11. Children

dMsg is not intended for children under 13, or under the minimum age required in their country to use online services without parental consent.

## 12. Changes and contact

When this policy changes, we update the date above and announce significant changes on the website or in the extension. For questions or requests, see [Support](/_/support).
