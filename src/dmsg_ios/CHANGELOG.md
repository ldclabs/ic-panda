# dMsg for iOS changelog

This changelog tracks the iOS marketing version in [`project.yml`](project.yml),
independently of the desktop and CLI release. TestFlight build numbers are
tracked separately. Build and device-test evidence is recorded in
[`VALIDATION.md`](VALIDATION.md).

## 1.1.1 - 2026-11-10

Available on the [App Store](https://apps.apple.com/app/id6827050680) for
iPhone and iPad running iOS 18 or iPadOS 18 or later.

### Build 24 reminders

- Keep reminders openable when newer activity arrives after the list loads,
  acknowledging only the displayed version. Reading one letter preserves the
  system notification for other unread letters in the same inbox.
- Add visible Mark as read and Mark all as read actions. The latter covers all
  local accounts and unloaded reminders, clearing badge counts without completing
  pending actions or changing message read receipts.
- Preserve read markers across restarts and same-version updates; newer activity
  becomes unread again. Opening a reminder acknowledges its displayed version.
- Explain network and notification-registration issues separately and open the
  relevant settings. Keep receiver registration retryable even when a cached
  Apple registration was successful.

### Build 23 fixes

- Restore saved Passkey authorization before reading membership execution usage
  after local unlock or account switching. Reuse valid authorization and renew
  it when needed, while keeping local content access independent of online login.
- Stop repeated APNs token callbacks from restarting notification registration.
  Reuse a valid Apple registration when retrying a later notification-settings
  sync, and show a retryable error when native registration fails.
- Consolidate notification settings into one current status, place progress
  inside the relevant action, and reflect remote acknowledgements without
  replacing unsaved edits.
- Fetch again when a new-message hint arrives during an existing refresh.
  Prevent an older response from suppressing the newer read.
- Show specific message-send failure reasons and add diagnostics without message
  content or account identifiers. Retrying preserves the original message ID and
  the next draft; receiving a newer message is not a prerequisite for sending.

### Login and local unlock

- Add native Passkey registration and login with hardware-backed session renewal
  when online access is needed. Keep Internet Identity and the existing flow
  for binding another login identity to an account.
- Support local sign-out and Passkey-confirmed sign-out of all login sessions.
  Offer the latter when the session limit is reached; retain retry guidance for
  other capacity errors. Already-issued delegations can remain valid for up to
  12 hours after signing out all sessions.
- Use the shared production Passkey issuer by default. Server setup and signed
  real-device acceptance remain release prerequisites.
- Keep hardware-protected local content available after Internet Identity
  login expires, with an optional login action on Account for online access.
- Accept a pending login result after returning from the background, while
  still discarding results after logout, account switching or locking.
- Offer explicit choices to use an existing Passkey or create a new one for
  login and account binding. Check service readiness before creating a system
  credential and explain when registration has not been confirmed.
- Resume interrupted account bindings with the original credential and verified
  login instead of creating another Passkey. Keep retries usable throughout the
  original challenge window and discard attempts from a superseded session.
- When a Passkey reaches its session limit, reset sessions for that credential
  and continue the same login or binding without replacing the local workspace.
- Prevent automatic local unlock from reopening the previous account while
  signing into another one.

### Approvals and reminders

- Restore a valid workspace promptly after a system-authentication prompt during
  device approval. Keep approval progress visible and both decision buttons
  disabled until the result arrives; background returns still revalidate the
  session.
- Keep the approval explanation above the action buttons when text wraps.
  Use centered confirmation alerts with an explicit Cancel action for approval,
  rejection and sign-out.
- Show each local account's unread and actionable reminder count in the account
  switcher, so reminders in another workspace are visible before switching.

### Inbox

- Refresh existing free-inbox receiving keys after account security or root
  changes while preserving the signed contact rules.
- Explain pending letters and outdated receiving settings, including whether
  the recipient needs to allow another letter or unlock and sync. Retry the
  original letter instead of creating a duplicate.

### Transfers and wallet operations

- Make foreground visibility, busy operations and transfer phases explicit,
  keeping automatic device-transfer resumption limited to the receiver and
  preserving background ciphertext-transfer retry state.
- Keep uncertain wallet transfers, control changes and cosigner submissions
  on their original request IDs. Restore editing only after the journal
  confirms that a proposal is absent or rejected.

### Membership

- Place a tappable membership badge beside the avatar and name in Account,
  with a stacked layout for accessibility text sizes. Existing App Store
  subscribers open Apple's subscription management directly; other members open
  Plan and usage. Keep one Plan and usage row and remove the separate upgrade button.
- Reload membership after unlocking or returning to the app without requiring a
  tab switch, including when authentication initially hides the Account screen.
- Use App Store subscription management for an existing subscription instead of
  presenting all plans as new purchases. Remove the file-size comparison table
  from Plan and usage; the backup screen still shows the applicable file limit.
- Avoid a canister update and cloud quota synchronization before opening the
  purchase sheet, and check membership and gateway configuration concurrently.
- Distinguish the current membership from a plan change scheduled for the next
  renewal, and refresh the schedule after returning from Apple's subscription
  management.

### Vault

- Copy a field by tapping its value. Use its menu to reveal or hide secrets,
  show values in large type, open websites or view field references, with no
  second reveal confirmation on mobile.
- Read long passwords and keys in a scrollable large-type view, with direct
  copying and automatic clearing when dismissed or backgrounded.
- Compact the detail header and field rows, hide unfilled fields while reading,
  and move timestamps to Item information and favorites to the item menu.
  Keep the editing action visible and show copy feedback beside the field.
- Fix vault history ordering when several versions are saved within the same
  millisecond, preserving newest-first display and correct parent relationships.

### Subscriptions

- Enable App Store in-app purchases in the release configuration. TestFlight
  annual subscription testing uses existing production accounts and requires
  explicit service opt-ins for sandbox purchases.
- Identify test memberships and Apple's test expiry in all 23 supported
  languages. Keep formal memberships ahead of test memberships and isolate
  test upgrades and refunds from formal purchases.
- Suppress misleading plan-expiry alerts for short-lived test subscriptions.
  Hide storage packs in Sandbox and show the storage-purchase hint only when
  a storage product is available.
- Finish StoreKit transactions only after the service confirms delivery, then
  refresh the displayed plan and usage.
- Verify the existing subscription's account before purchase. Offer account
  switching when it belongs to another dMsg account and block a new purchase
  when ownership cannot be verified. Restoring purchases does not transfer a
  subscription between accounts and reports whether anything was applied.
- Keep purchase and restore delivery tied to the originating account and
  session. Returning from Apple's purchase sheet no longer starts a competing
  page reload that could discard delivery state; refresh after delivery instead.
- Clarify annual App Store auto-renewal, cancellation, purchase restoration
  and Apple refund handling in the linked terms of service.

### Localization

- Improve terminology and natural phrasing across all 22 non-English locales,
  including system integrations and the linked privacy and subscription terms.

## 1.1.0 - 2026-11-06

### Files

- Share local files and cloud backups by link, including versions without a
  local copy. Recipients can download in a browser without an account or app.
  List and revoke links from Share links; revocation stops new access but cannot
  recall downloads already started or saved.
- Collect files through request links on Plus and higher plans. Create one link
  per name, retain unfinished names after a partial failure, review arrivals and
  close links from Files.
- Save received files explicitly into `Received files/<title>/<label>`, with
  byte progress. Same-name arrivals remain separate files, and deleting a cloud
  copy leaves the saved copy intact. Stop link batches when leaving their page
  or locking the session.
- Fix photo and video import, and capture directly into the open folder with
  the system camera. Captures are encrypted in the library without being added
  to Photos; cloud backup remains a separate action.
- Add file thumbnails, drag-in import and previews on open. Keep folder and
  preview navigation in a stack, including the iPad detail column, and decode
  file listings off the main thread.

### Sync and offline use

- Receive live vault and channel activity hints while unlocked in the
  foreground, synchronize read positions across devices and refresh channel
  lists without opening each conversation.
- Retry failed sync automatically, preserve saved content during refreshes and
  back off while idle. Fast unlock waits at most one second for the network;
  status stays quiet unless an initial sync, lasting failure or action needs
  attention.
- Resume unanswered sends with their original IDs for up to 24 hours. Use the
  short background window to finish already prepared submissions; new content
  downloads wait for the foreground.
- Keep temporary reads for sharing a backup separate from durable restore jobs.

### Everyday use

- Fix an iPad freeze when switching workspace tabs or opening account pages.
  Transient presentation updates no longer rebuild workspace navigation, and
  visible screens register their feedback host only once.
- Align Vault, Files and Messages tabs and toolbars, remember the last workspace
  tab and restore navigation after locking. Put reminders in one activity menu
  and unread counts on Messages; open the Vault list initially on iPad.
- Group conversation messages by day, jump to the first unread message, keep
  the composer separate and offer invitations immediately after creating a
  conversation. Remove report and block actions from conversations while keeping
  them in the free inbox.
- Copy vault fields with one tap from details or list-row actions. Copies stay
  on this device, expire within 30 seconds and clear on lock. Revealing a sensitive
  field still asks for confirmation, with a 30-second interval per item; copying
  does not extend it. One-time codes display and refresh automatically, and
  expired codes disappear from historical versions too.
- Apply notification and mute changes automatically, preserve edits made during
  refreshes and let a started settings save finish after its page closes.
- Update all 23 supported languages for the new workflows.

### System integrations

- Let the AutoFill snapshot follow the logins explicitly selected once while
  dMsg is open and unlocked. Keep failed publications pending and prevent late
  refreshes from restoring a removed snapshot; an empty selection revokes it.
- Add AutoFill search and current-site sections, share-extension thumbnails and
  a medium-size widget.
