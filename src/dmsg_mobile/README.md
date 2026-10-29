# dMsg mobile core

In-process Rust host for the native iOS application. See `src/dmsg_ios/README.md`
for build instructions and the verified release boundaries.

`notifications.rs` exposes named settings, status, reminder pagination, observed
versions, snooze, Apple registration, reconciliation, local presentation results
and opaque-route resolution. It uses the same scoped host/session fence and
`Hold` network steps as other mobile operations. No arbitrary daemon RPC is
exported. Shared-account background notices are owned by the remote provider;
local native plans cannot create a second background presentation for them.
