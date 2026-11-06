# dMsg for iOS changelog

This changelog tracks the iOS marketing version in [`project.yml`](project.yml),
independently of the desktop and CLI release. TestFlight build numbers are
tracked separately. Build and device-test evidence is recorded in
[`VALIDATION.md`](VALIDATION.md).

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
