# dmsg-device-transfer/1: direct transfer between devices

English | [简体中文](device_transfer_zh.md)

Drafted 2026-10-30. The protocol core is implemented in `dmsg_client::transfer`
and fixed by [transfer-v1 vectors](../../tests/vectors/transfer/transfer-v1.json).
The hosts' transports (a TCP listener and Bonjour on macOS and iOS) carry it;
they do not change the bytes described here.

A one-time, end-to-end encrypted transfer of local content from one active
device of an account to another over a local connection. It moves what the
account's cloud does not hold, such as the dMsg Files library, when a device
replaces another (a succession) or alongside it. Both devices must be active
devices of the same account in the same environment. Nothing is exported to a
file: the content goes from one device's workspace to the other's.

## 1. Roles and session

| Role | Does |
| --- | --- |
| Receiver | Opens a session with a random 16-byte `session_id`, listens, advertises it and asks for what it lacks. Commits records into its own workspace. |
| Sender | Connects to the session, wraps a new session key to the receiver and serves records and chunks from its manifest. Changes nothing on its side. |

- The receiver advertises the Bonjour service `_dmsg-xfer._tcp` with the
  instance name `hex(digest("dmsg/device-transfer/advert/v1", session_id)[0..8])`
  and a TXT record `v=1`. Only a device that knows the session ID, from a
  succession token, a QR code or a short code, finds it. Without Bonjour the
  receiver shows `address:port` to type in.
- `digest(domain, value) = SHA-256(CBOR([1, domain, value]))` with core
  deterministic encoding, as in every dMsg contract.
- Both devices read the account's certified state when the session starts and
  again before the end, and check that the other device is active, that its
  ID, signing key and HPKE key are those certified, and that neither is
  revoked.

## 2. Hello

The first frame is in plain text: a 4-byte big-endian length, then the
canonical CBOR of

```text
Hello = {
  header: {
    format: 1,
    environment: "local" | "staging" | "production",
    account: AccountId,             // 12-byte Xid
    from: bytes .size 32,           // sender device ID
    to: bytes .size 32,             // receiver device ID
    session_id: bytes .size 16,
    enc: bytes,                     // HPKE encapsulated key
    key: bytes,                     // the 32-byte session key K, sealed
    manifest: bytes .size 32,       // manifest digest (section 4)
  },
  signature: bytes .size 64,        // sender's Ed25519 signature
}
```

- `K` is 32 random bytes. `(enc, key) = HPKE-Seal(receiver hpke_pub, info, K)`
  in base mode (RFC 9180: DHKEM X25519 HKDF-SHA256, HKDF-SHA256, AES-256-GCM),
  with `info` = AAD = `CBOR(["dmsg/device-transfer/v1", account, from, to, session_id])`.
- `signature = Ed25519(sender signing key, digest("dmsg/device-transfer/v1", header))`.
- The receiver waits at most 10 seconds for the hello and refuses one whose
  `format`, `environment`, `account`, `to` or `session_id` is not its own,
  whose `from` is itself or not active, whose signature does not verify, or
  whose key does not open. A hello is at most 4,096 bytes.

## 3. Frames

Every later frame is a 4-byte big-endian length followed by
`AES-256-GCM(K, nonce, aad, CBOR(body))`:

- `nonce = direction ‖ 0x000000 ‖ sequence` (1 + 3 + 8 bytes, big-endian
  sequence). `direction` is 0 from sender to receiver and 1 back; `sequence`
  counts each direction's frames from 0.
- `aad = CBOR(["dmsg/device-transfer/v1", session_id, direction, sequence])`.
- A body is at most 1,114,112 bytes (1 MiB + 64 KiB): one sealed 1 MiB chunk
  with its framing. A frame replayed, reordered, reflected or moved to
  another session does not open. Either side gives up after 120 seconds
  without the next frame.

Bodies, each a CBOR map with a single key naming its type:

| Body | Sent by | Content |
| --- | --- | --- |
| `have` | receiver, first | `{manifest: digest / null}`: the manifest kept from an earlier connection of this session. The sender then does not send it again. |
| `manifest` | sender | `{entries: [Entry], last: bool}`: part of the manifest, entries in name order, at most 2,048 per part. |
| `want` | receiver | `{records: [name], blobs: [digest]}`: at most 256 in all. The replies follow in that order, chunks first. |
| `blob` | sender | `{digest, bytes}`: a sealed chunk. |
| `record` | sender | `{key, value}`: a record's canonical CBOR, empty when the record is gone. |
| `done` | receiver | `{categories, committed}`: it went through every entry of the chosen categories, whose manifest digest is `committed`. |
| `ack` | sender | It checked `done`. |
| `abort` | either | `{code}`, such as `INTEGRITY_FAILED` or `FORBIDDEN`. |

The sender serves only names and digests its manifest lists, and refuses
anything else with `FORBIDDEN`.

## 4. Manifest

```text
Entry = {
  key: text,                 // record name, e.g. "lib-file:<id>"
  sha256: bytes .size 32,    // SHA-256 of the record's canonical CBOR
  size: uint,                // its length
  blobs: bytes,              // SHA-256 of each sealed chunk it references, 32 bytes each
  blob_bytes: uint,          // the chunks' total length
}
manifest digest = digest("dmsg/device-transfer/manifest/v1", [Entry, ...])
```

Entries are sorted by `key`, each once. The receiver shows the total size,
lets the person leave out categories, and keeps the manifest sealed under its
local data key with the session, so a broken connection resumes: the next
connection uses a new `K` and the same `session_id`, `have` names the kept
manifest, and the receiver asks only for what it lacks.

### Categories and what stays

Only names on this list move; any other name stays on the sending device.

| Category | Records | Chunks |
| --- | --- | --- |
| `library` | `lib-folder:`, `lib-file:`, `lib-version:` | A version's sealed chunks and thumbnail, when they are all on the sending device; a version without them is offered as cloud-only. |

Stays: the device's keys and identity, its wallet key, software cosigner keys
and Secure Enclave handles, the local data key, logins, the daemon's owner
credential, applications' and MCP grants, approvals and attestations of this
device, opened plain-text copies and import staging, cloud copies (read again
from the account), share links, backup jobs, wallet jobs, quotes and drafts,
payer and checkout journals, channel and inbox state (synced through the
relay), and any synced object the cloud already holds. Wallet jobs and payer
journals are settled on the old device before it is revoked.

## 5. Committing

- The receiver asks for a record's chunks first and commits the record only
  when every chunk it references is stored and its SHA-256 matches. Folders
  come first, then versions, then files.
- It only adds: a record it already has stays as it is. A file gains the
  versions that arrived; its name, place, tags and trash state stay the
  receiver's. Nothing trashed is restored and nothing is deleted. A record
  whose bytes no longer match the manifest (changed on the sender meanwhile)
  is left out. A new file none of whose versions arrived is left out.
- After its final check of both devices, the receiver sends `done` with the
  categories it chose and their entries' manifest digest; the sender
  recomputes it from its own manifest, checks both devices again and answers
  `ack`. The receiver then marks the session finished and drops the kept
  manifest.
- A write failure, such as a full disk, stops the transfer before any record
  whose chunks did not all arrive; the session resumes after space is freed.

## 6. Vectors

[transfer-v1.json](../../tests/vectors/transfer/transfer-v1.json): with
test-only keys, the advertised name, two manifest entries and their digest,
the HPKE info, the hello (header CBOR, signed digest, signature and the
framed bytes) and one frame of every body with its AAD and sealed bytes.
The HPKE ephemeral key comes from the recorded `ekm`, so the file is
reproducible. Readers rebuild every encoding and digest, verify the
signature, open `K` with the receiver's HPKE seed and open every frame.
