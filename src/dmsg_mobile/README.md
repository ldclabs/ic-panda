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
