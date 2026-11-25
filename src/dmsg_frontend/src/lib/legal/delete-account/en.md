# Delete your dMsg account

Last updated: 2026-11-25.

This page explains how to request permanent deletion of a **dMsg** cloud service account and associated cloud data, including after uninstalling the dMsg Android, iOS or desktop app. dMsg is developed under **ICPanda DAO** governance; mobile support requests are handled through `hi@yiwen.ai`.

## 1. Delete inside the dMsg app (fastest)

If you still have any authorized administrator device (Android, iOS or desktop) signed in to your dMsg account:

- Open **dMsg** and go to **Settings › Account security › Delete service account**.
- Review any unsynced changes, owned active channels or multisig wallet responsibilities.
- Enter your Account ID and complete system confirmation to start permanent deletion immediately.

Local-only workspaces (**Use on device**) never create a cloud service account. To erase a local-only workspace, remove it inside the app or uninstall the app and clear its device data.

## 2. Request deletion without the app

If you have already uninstalled dMsg or no longer have access to an authorized device, send an email to [hi@yiwen.ai](mailto:hi@yiwen.ai?subject=dMsg%20Account%20Deletion%20Request) with the subject **dMsg Account Deletion Request** and include only the minimum information needed to locate your account and verify ownership:

- Your **dMsg Account ID** (20-character identifier) or **public handle**, if you have it;
- If you purchased a paid plan, your **Google Play order ID** (for example `GPA.xxxx-xxxx-xxxx-xxxx`) or **Apple App Store transaction reference**, so we can verify account ownership against the billing binding;
- A short confirmation that you want to permanently delete the dMsg cloud service account.

**Never send secrets.** Never include private keys, recovery materials, seed phrases, encrypted vault backups, backup passwords, one-time codes or login sessions. dMsg support will never ask for them, and no link in an email or webpage can execute irreversible deletion directly.

## 3. Verification and processing

- We acknowledge your request by email and assign a tracking reference ID. Duplicate requests for the same account are linked to the same deletion job.
- **Verification before erasure:**
  - If you still have an authorized device, in-app deletion provides immediate cryptographic proof of account control.
  - If you uninstalled the app and made a Google Play or App Store purchase, support verifies your order ownership against our billing records before triggering deletion.
  - If you enabled on-chain recovery and still hold your bound Passkey or Internet Identity, you may also reinstall dMsg, complete the account's recovery delay, and delete the account directly in-app.
  - Because dMsg is end-to-end encrypted and does not collect an email address or phone number when creating an account, support cannot verify ownership of a free anonymous account from an email address alone (to prevent unauthorized third parties from deleting someone else's account). If you have lost all authorized devices, have no billing record and did not enable on-chain recovery, uninstalling the app removes local device data and remaining cloud ciphertext cannot be decrypted by the service.
- Once ownership is verified and any unsettled paid inbox orders are resolved, support triggers the same tracked server cleanup job used by in-app deletion (`requested` → `purging` → `deleted`) and replies with completion confirmation only after server cleanup finishes.

## 4. What is deleted and what is retained

When service-account deletion completes, dMsg permanently blocks further service access to the account and deletes:

- Account-owned cloud vault, note and file ciphertexts;
- Account inbox messages and file-request uploads;
- Public avatar copies stored in cloud object storage and their public CDN cache;
- Device push tokens, notification preferences, view receipts, browser connection mailboxes and hosted agent relay state.

The following data is **not** deleted or is retained under explicit limits:

- **Other people's copies and other devices:** Messages or files already received and saved by other people, and local encrypted copies stored on other devices (remove local account data separately on each device).
- **Immutable public blockchain records:** Public or certified on-chain records on Internet Computer (ICP) canisters and token ledgers cannot be erased by design.
- **Financial and anti-replay records:** Minimal settled transaction, billing, refund and anti-replay records are retained as needed for accounting, fraud prevention and legal obligations.
- **Abuse reports:** Moderation and abuse report records are retained for up to **90 days** from creation and then automatically purged.

## 5. Cancel subscriptions separately

Deleting your dMsg service account does **not** automatically cancel an active subscription purchased through Google Play or the Apple App Store.

- **Google Play:** Manage or cancel subscriptions at [play.google.com/store/account/subscriptions](https://play.google.com/store/account/subscriptions).
- **Apple App Store:** Manage or cancel subscriptions in your Apple Account settings.

For more information, read the [Privacy Policy](/_/privacy), [Terms of Service](/_/terms) and [Support](/_/support) pages.
