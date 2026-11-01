# QR device authorization (`dmsg-device-invite/2`)

English | [简体中文](device_pairing_zh.md)

Both directions end in the existing device approval flow:

- **New device scans old desktop:** under Settings → Devices → Authorize another
  device, the desktop displays a short-lived invitation. The new iOS device
  signs in to the same account, scans it, and returns its encrypted pairing
  request through the configured relay. The desktop decrypts and verifies it,
  shows the complete device ID, role and capabilities, and opens the approval
  window when the person chooses Review. The devices can use different networks.
- **Old device scans new device:** the new device displays its existing
  `dmsg-device-request/2` QR code. The authorized device scans that request
  directly and reviews it. Desktop supports a camera, a selected image,
  clipboard images and pasted text; iOS supports scanning and pasting. This
  direction needs no invitation mailbox or request return trip.

A scan or relay receipt never enrolls a device or binds a login identity.
The normal `AddDevice` checks and content-root rotation remain authoritative.
The new device checks its account's certified state to finish joining.

## Invitation and encryption

Invitation text is `dmsg-device-invite/2:` plus unpadded base64url of a canonical
CBOR map:

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
five-minute ceiling. The phone must match the invitation's home/account to its
own authenticated account and the relay to its own configuration. A QR code
cannot redirect it to a different endpoint.

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
administrator. Only one ciphertext is accepted: the same bytes return the same
`{digest: lowercase_hex_sha256}` receipt; different bytes conflict. Submission
bodies are capped at 8192 bytes. No encryption key or raw pairing proof is sent
to the relay.

Create returns `{id, expires_at}`; reads add `sealed`, either null or base64url.
Replaying an identical create is idempotent and does not extend expiry. A
cancelled invitation cannot be revived by replay. There are at most four active
invitations and ten creations per account per minute. Cancellation removes the
ciphertext and prevents creation replay until expiry; automatic expiry discards
the mailbox metadata. Reads do not refresh TTLs.

The desktop keeps its decryption key only in memory, bound to its original
unlocked session. Closing the dialog cancels locally before contacting the
relay. If offline or already locked, relay cancellation can fail; the old local
session still cannot read it and the relay deadline remains in force.

## Client interfaces and validation

Owner daemon methods are `device/pair_invite`, `device/pair_invite_status {id}`,
`device/pair_invite_cancel {id}`, `device/pair_inspect {token}` and
`device/pair_scan {image}` (base64 image bytes, decoded locally). The main process
owns image selection and clipboard access; renderers never supply file paths.
Camera capture stops on success, cancellation, navigation or lock. Both scan
directions use the existing `device/approve {token, device}` to create an
approval; the main renderer cannot decide it. Mobile exposes
`join_device_invitation(invitation)` for the relay direction and retains its
direct request/inspection APIs.

The public [fixed vector](../../tests/vectors/client/device-invite-v2.json)
contains public test keys, a QR, encrypted payload and canonical registration /
submission bodies. Core tests exercise decryption/proof validation, the HTTP
transport, credential/key separation, expiry, changed sessions and image QR
reading. Electron tests use a daemon fixture and synthetic video; iOS UI tests
use a simulator. These do not establish physical-camera or production
login/enrollment compatibility. Deploy the compatible relay before releasing
clients that generate these invitations; no deployment is part of these tests.
