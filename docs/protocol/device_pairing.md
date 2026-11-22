# QR device authorization

English | [简体中文](device_pairing_zh.md)

Desktop, iOS and Android devices can request to join. Any of them can approve another device when authorized as an administrator.

- **The administrator shows the code.** Under Devices → Authorize another device, the administrator displays a short-lived invitation. The new device chooses **Open an existing account** and scans or pastes it. The devices can use different networks. The administrator compares the codes on both devices and reviews the login and device permissions.
- **The new device shows the code.** A phone administrator scans the request directly. A linked login shows only the device request. For a new login, the administrator reviews the login binding first and then approves the device. A desktop administrator can also use a camera, QR image file, clipboard image or pasted text.

A scan or relay receipt never enrolls a device or binds a login identity.
The normal `AddDevice` checks and content-root rotation remain authoritative.
The new device checks its account's certified state to finish joining.

The scanner also accepts a signed `dmsg-approval-key/1` enrollment request from an authorized device. The client verifies the account, device and proof of key possession and shows the fingerprints for review. A device without the required approval key can pass the original join request to an eligible administrator or create a separate enrollment request for itself. Key registration changes the account version, so the joining device must refresh its signed request. Direct scans and forwarding create no server-side approval queue.

Device authorization commits before the new content root is distributed. During the same unlocked session the joining device checks certified device status and fetches the new root without another local hardware prompt.

## Joining with a different login

An administrator on desktop, iOS or Android can show a `dmsg-device-invite/3:` invitation. The joining device scans or pastes it from **Open an existing account** and checks the destination. An existing login for that account proceeds directly to device approval. With the owner's consent, an unbound login first asks to become a login for the account. A login already used by another account can choose a different login and keep its existing workspace.

To approve a new desktop from a phone, open **Authorize another device** on the phone. The desktop can scan the phone’s invitation, read its QR image or paste its text. To use the phone’s camera instead, the desktop can show a request QR after entering the phone’s account ID. The phone scans the login binding request and compares the login identity. Once the desktop accepts the binding it shows the fresh device request for the phone to scan and approve. An already-bound login goes directly to the device QR. This direction uses local QR codes.

The v3 CBOR map adds `binding: {invite_id, key, submit_token}` to the v2 fields. Each additional field contains 32 independently random bytes. The two mailboxes share the home, account, relay and deadline, with independent v2 encryption, submission and cancellation. The device mailbox takes the original device token. The login mailbox takes a base64url canonical CBOR record `{token, device, proof}`. Its token is a `dmsg-auth-request/1`, device is the full requested DeviceInput, and proof is its Ed25519 signature over `digest("dmsg/invited-login/v1", (device mailbox ID, token, device))`. The comparison code is the first four bytes of `digest("dmsg/join-comparison/v1", (home, account, device mailbox ID, new login Principal or null, device))`, displayed as two groups of four hexadecimal digits.

The administrator reviews separate login and device approvals. Once the new login accepts with its own IC identity, the joining device signs a request at the updated account version. The issuer matches the complete device keys, role and capabilities across replies and checks that the login is now bound. The joining workspace encrypts the confirmed invitation and exact ciphertext for resumption and idempotent retries. Waiting for the root reuses the authenticated local session. Older v2 invitations and direct device requests remain supported. See the [account control contract](account_root.md) for the temporary binding and durable permissions.

## Invitation and encryption

The v2 device invitation starts with `dmsg-device-invite/2:` and the v3 account invitation starts with `dmsg-device-invite/3:`. Both encode a canonical CBOR map using unpadded base64url. They share the fields below. V3 adds the `binding` field described above.

| Field | Encoding |
| --- | --- |
| `home` | User-home Principal bytes |
| `account` | 12-byte account ID |
| `relay` | Configured relay origin as text, without a trailing slash |
| `invite_id` | Random 32-byte mailbox identifier |
| `expires_at` | Unix milliseconds; at most five minutes ahead |
| `key` | Independent random 32-byte AES key |
| `submit_token` | Independent random 32-byte write credential |

The producer uses a 270-second deadline to leave clock-skew margin within the
five-minute ceiling. The joining device verifies the configured relay and target account's home. Its current login must belong to that account before it sends a device request.

The request plaintext is the original pairing token's UTF-8 bytes, at most
4096 bytes. A sealed request is a random 12-byte nonce followed by AES-256-GCM
ciphertext and its 16-byte tag, at most 4124 bytes total. AAD is canonical CBOR
`["dmsg-device-invite/2:", home, account, relay, invite_id, expires_at]`.
The encryption key only travels in the QR/copy transport. The relay receives
SHA-256 of the independent submit credential when the mailbox is registered,
and the credential with a submitted ciphertext; neither lets it decrypt.

## Relay contract

All routes use the configured HTTPS origin (loopback HTTP only in local mode),
canonical CBOR bodies and normal `{ok,data}` / `{ok:false,error}` JSON responses.
They refuse browser Origin headers and use `Cache-Control: no-store`.

| Route | Authentication and body |
| --- | --- |
| `POST /v1/accounts/:account/device-invites` | Device HTTP PoP and signed `dmsg/device-invite/create/v2`, payload `{invite_id: bstr32, submit_hash: bstr32, expires_at: uint}` |
| `GET /v1/accounts/:account/device-invites/:invite` | Creating device's HTTP PoP |
| `POST /v1/accounts/:account/device-invites/:invite/cancel` | Creating device's HTTP PoP and signed `dmsg/device-invite/cancel/v2`, payload `{invite_id: bstr32}` |
| `POST /v1/device-invites/:account/:invite` | One-time capability, body `{submit_token: bstr32, sealed: bstr}`; does not require an already enrolled device |

Creation and reading require a currently authorized administrator with
`RootManage`. Another administrator cannot read or cancel the creator's mailbox.
Submission also checks that its creator remains an authorized root
administrator. Each mailbox accepts one ciphertext. Identical bytes return the same
`{digest: lowercase_hex_sha256}` receipt. Different bytes conflict. Submission
bodies are capped at 8192 bytes. No encryption key or raw pairing proof is sent
to the relay.

Create returns `{id, expires_at}`; reads add `sealed`, either null or base64url.
Replaying an identical create is idempotent and does not extend expiry. A
cancelled mailbox cannot be revived by replay. Each account can have up to four
active mailboxes and create ten per minute. A v3 invitation uses two. Cancellation removes the
ciphertext and prevents creation replay until expiry; automatic expiry discards
the mailbox metadata. Reads do not refresh TTLs.

The issuing device holds its decryption key in its original unlocked session's
memory. Closing the invitation cancels locally before contacting the
relay. If offline or already locked, relay cancellation can fail; the old local
session still cannot read it and the relay deadline remains in force.

## Client interfaces and validation

Owner daemon methods are `device/pair_invite`, `device/pair_invite_status {id}`,
`device/pair_invite_cancel {id}`, `device/pair_inspect {token}` and
`device/pair_scan {image}` (base64 image bytes, decoded locally). The main process
owns image selection and clipboard access; renderers never supply file paths.
Camera capture stops on success, cancellation, navigation or lock. Desktop issues account invitations with `device/pair_invite {with_login: true}`, joins through `device/join_invitation` and resumes through `device/saved_invitation`. The login review uses `account/bind_approve` and the device review uses `device/approve {token, device}`. Decisions go through the approval window.
Mobile exposes `start_account_invitation`, `account_invitation` and `saved_account_invitation`, alongside the v2 and direct scanning interfaces.

The public [fixed vector](../../tests/vectors/client/device-invite-v2.json)
contains public test keys, a QR, encrypted payload and canonical registration /
submission bodies. Core tests exercise decryption/proof validation, the HTTP
transport, credential/key separation, expiry, changed sessions and image QR
reading. Electron tests use a daemon fixture and synthetic video; iOS UI tests
use a simulator. These do not establish physical-camera or production
login/enrollment compatibility. Deploy the compatible relay before releasing
clients that generate these invitations; no deployment is part of these tests.
