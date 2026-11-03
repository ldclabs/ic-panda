# dMsg mobile core

In-process Rust host for the native iOS application. See `src/dmsg_ios/README.md`
for build instructions and the verified release boundaries.

Account registration, device joining, recovery and ordinary login default to
local device binding when the platform supplies a credential store and
supported presence hardware. Verification runs outside the scoped client
host, so locking cancels its result; it does not add a chain update. Old
unbound fast unlock first requires login, and an explicit unbind is remembered.

`notifications.rs` exposes named settings, status, reminder pagination, observed
versions, snooze, Apple registration, reconciliation, local presentation results
and opaque-route resolution. It uses the same scoped host/session fence and
`Hold` network steps as other mobile operations. No arbitrary daemon RPC is
exported. Shared-account background notices are owned by the remote provider;
local native plans cannot create a second background presentation for them.

`transfer.rs` runs device transfers (dmsg-device-transfer/1) in the mobile
runtime: a receiving session's listener or a sender's connection, one at a time
in `dmsg_client::transfer::Transfers`, each ended once its session's generation
ends. The app advertises or finds the session with Bonjour (`transfer_advert`).

`succession.rs` exposes replacing a device: the new device's request, the old
device's review and approval, listing, running, leaving and stopping each
side's steps, and the copied-workspace import and successor approval. Every
operation is a `Hold` function with source `gui`, so steps that ask create
approval requests the app's approval view decides; wallet unbinding requests
are decidable on the phone like joining.

`display.rs` exposes `cached_display` and `refresh_display` from the shared Rust
snapshot implementation, plus encrypted cache inventory/clearing. Resource lanes
coalesce overlapping vault, channel, Inbox, display and cloud-inventory reads.
Session and cache-clear generations fence queued and late writes. Common channel,
Inbox, mailbox and file-transfer operations use shared Host steps, releasing the
client during network waits. Approved account/root changes and payment journals
retain the existing exclusive mutation runner; ordinary reads do not enter it.
Notification reconciliation does not download Inbox message bodies.


## File transfers (2026-11-02)

`restore_backup` now returns a durable queued download snapshot; it no longer
returns a restored file before a long RPC finishes. Hosts drive `run_downloads`
and observe `download_status`. The iOS task list uses `retry_backup`,
`retry_download`, `cancel_backup` and `cancel_download` for one selected task;
`retry_file_transfers` remains available for callers explicitly retrying all work. `run_backups` continues to drive saved uploads without a
15-second slice. Only terminal verified/committed status means success.

iOS owns these runners at the active unlocked scene, rather than a file detail
view. Navigation does not cancel them. Lock/background cancels foreground work;
returning to the unlocked app resumes saved work. Explicit background ciphertext
handoff remains separate: the OS receives no content/signing keys, retries only
its original slots a bounded number of times and still requires foreground
verification/finalization. Foreground runners do not race an OS handoff.
