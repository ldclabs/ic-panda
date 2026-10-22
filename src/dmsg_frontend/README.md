# dMsg web frontend

The public profile, login/connect entry points, verification pages and encrypted file share reader. Development commands are in package.json.

## Encrypted share downloads

The share page starts one random download session and fetches its short-lived S3 URLs without cookies or referrers. Lost start replies reuse the same session ID; renewal uses GET and does not consume another download count. URL expiry or a missing block triggers one authority recheck; corruption fails without saving a file. `src/lib/share/download.ts` validates the descriptor, size and digest; the decryptor checks AEAD and the whole file. The deployed CSP allows HTTPS R2 S3 connections. The bucket must allow the actual share-page origin through CORS. Session IDs and URLs stay in page memory and must not enter logs. See the public [cloud protocol](../../docs/protocol/cloud_zh.md) for expiry and the bounded revocation window.
