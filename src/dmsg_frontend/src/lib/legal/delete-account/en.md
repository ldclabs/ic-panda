# Delete your dMsg account

Last updated: 2026-11-25.

This page explains how to request permanent deletion of a **dMsg** cloud service account and associated cloud data, including after uninstalling the dMsg Android, iOS or desktop app. dMsg is developed under **ICPanda DAO** governance; mobile support requests are handled through `hi@yiwen.ai`.

## 1. Delete inside the dMsg app (fastest)

On an authorized administrator device, open the **Account** tab in dMsg:

- **Android:** Open **Account security › Delete service account**.
- **iOS:** Open **Account security › Delete account**.
- Review unsynced changes, active channels you own and any wallet responsibilities. Follow the instructions, enter the account ID shown and confirm the deletion request.

Desktop users can use the email request below.

Local-only workspaces (**Use on device**) never create a cloud service account. To erase a local-only workspace, remove it inside the app or uninstall the app and clear its device data.

## 2. Request deletion without the app

If you have already uninstalled dMsg or no longer have access to an authorized device, send an email to [hi@yiwen.ai](mailto:hi@yiwen.ai?subject=dMsg%20Account%20Deletion%20Request) with the subject **dMsg Account Deletion Request** and include only the minimum information needed to open a request:

- Your **dMsg Account ID** (20-character identifier) or **public handle**, if you have it;
- A short confirmation that you want to permanently delete the dMsg cloud service account.

**Never send secrets.** Never include private keys, recovery materials, seed phrases, encrypted vault backups, backup passwords, one-time codes or login sessions. dMsg support will never ask for them, and no link in an email or webpage can execute irreversible deletion directly.

## 3. Verification and processing

- We acknowledge your request by email and assign a tracking reference ID. Duplicate requests for the same account are linked to the same deletion job.
- Before authorizing deletion, support must independently verify that you control the dMsg account. An account ID, public handle, sender address or store order reference alone cannot establish control. If support needs further information, it will reply with the verification steps.
- If you still have an authorized administrator device, you can confirm deletion in the app. If you enabled on-chain recovery and retain your bound Passkey or Internet Identity, you can reinstall dMsg, complete recovery and then use in-app deletion.
- You can submit an email request even if you lost your device. If account control cannot be verified, support will explain the outcome and leave the account intact. Removing the app does not request cloud deletion.

- Once ownership is verified and any unsettled paid inbox orders are resolved, support triggers the same tracked server cleanup job used by in-app deletion (`requested` → `purging` → `deleted`) and replies with completion confirmation only after server cleanup finishes.

## 4. What is deleted and what is retained

When service-account deletion completes, dMsg permanently blocks further service access to the account and deletes:

- Account-owned cloud vault, note and file ciphertexts;
- Account inbox messages and file-request uploads;
- Public avatar copies stored in cloud object storage and their public CDN cache;
- Device push tokens, notification preferences, view receipts, browser connection mailboxes and hosted agent relay state.

The following data is **not** deleted or is retained under explicit limits:

- **Other people's copies and other devices:** Messages or files already received and saved by other people, and local encrypted copies stored on other devices (remove local account data separately on each device).
- **Public blockchain history:** Finalized ledger transactions and entries already recorded in immutable public history remain visible.
- **Financial and anti-replay records:** Minimal settled transaction, billing, refund and anti-replay records are retained as needed for accounting, fraud prevention and legal obligations.
- **Abuse reports:** Moderation and abuse report records are retained for up to **90 days** from creation and then automatically purged.

## 5. Cancel subscriptions separately

Deleting your dMsg service account does **not** automatically cancel an active subscription purchased through Google Play or the Apple App Store.

- **Google Play:** Manage or cancel subscriptions at [play.google.com/store/account/subscriptions](https://play.google.com/store/account/subscriptions).
- **Apple App Store:** Manage or cancel subscriptions in your Apple Account settings.

For more information, read the [Privacy Policy](/_/privacy), [Terms of Service](/_/terms) and [Support](/_/support) pages.
