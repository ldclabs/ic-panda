# dmsg-nearby-share/1: Face-to-Face Nearby Share

English | [简体中文](nearby_share_zh.md)

The core protocol lives in `dmsg_client::nearby`, and its wire bytes are pinned by
[nearby-v1.json](../../tests/vectors/nearby/nearby-v1.json).

Two physically nearby dMsg devices (different accounts, same account, or offline Standalone workspaces) transfer selected library files or a `dmsg-secret-package/1` Vault snapshot over an end-to-end encrypted channel.

## 1. Transport and Discovery

- **QR-only by default**: Opening Receive listens locally and displays a `dmsg-nearby/1:` QR code (and copyable offer text) without broadcasting mDNS.
- **Opt-in local discovery**: When the receiver enables discoverability, it advertises `_dmsg-share._tcp` with a 16-hex ephemeral instance name and TXT keys `v=1`, `n=<alias <= 32 UTF-8 bytes>` and `p=<4-byte ephemeral pk hint>`. No `AccountId`, handle or permanent device ID is advertised.
- **Parallel racing with blind relay fallback**: Online logged-in receivers asynchronously open an ephemeral blind relay room (zero R2 storage, 5-minute join deadline, 100 MiB / 60 s idle / 15-minute transfer ceiling). Senders dial private LAN candidates and mDNS/AWDL immediately, launching the relay after 1.8 s in parallel; the first completed handshake wins.
- **Address guardrails**: `addrs` accepts only private (RFC 1918 / RFC 4193 ULA) and link-local literals; `relay.env` must match the client's configured environment and never steers to an arbitrary URL.

## 2. Commitment-then-Reveal Handshake

```text
1. Sender -> Receiver: ClientHello  = { v: 1, commit }
     commit = SHA-256(CBOR(["dmsg/nearby-share/commit/v1", s_pk, s_nonce]))
2. Receiver -> Sender: ServerHello  = { v: 1, sid, r_pk, r_nonce }
     (In QR mode, the sender pins r_pk == offer.pk before revealing.)
3. Sender -> Receiver: ClientReveal = { s_pk, s_nonce, ? qr_proof }
     transcript = SHA-256(CBOR(["dmsg/nearby-share/handshake/v1", 1, sid, commit, s_pk, s_nonce, r_pk, r_nonce]))
     qr_proof   = SHA-256(CBOR(["dmsg/nearby-share/qr-proof/v1", qrs, transcript]))
```

- Both peers run X25519 (rejecting all-zero outputs) and derive `K_tx` and a 6-digit SAS via HKDF-SHA-256.
- Receivers check the 6-digit SAS in every mode; browse-mode senders confirm the SAS before transmitting `OfferMeta`. Optional signed `Identity` frames are off by default and exchanged only after confirmation.
- The receiver locks its single active session slot only when the user accepts a peer.

## 3. Framing, Credit Window and Payloads

- Post-handshake frames use AES-256-GCM with `nonce = direction || 0x000000 || seq_be` and `aad = CBOR(["dmsg/nearby-share/frame/v1", sid, direction, seq])`, max plaintext `1 MiB + 64 KiB`.
- Receivers grant initial chunk credits (`W = 4`) in `Accept` and replenish every 2 chunks via `Credit`.
- **Files**: No high-risk approval; received through `library::import` (`free_name`, fresh `FileKey`, bounded by `files_max_bytes`, default 4 GiB, max 10 GiB / 64 files per session), committed and acknowledged per file (`FileAck`).
- **Vault items**: Reuses `dmsg-secret-package/1` (max 100 MiB, <= 64 fields, <= 20 attachments, `dmsg-` reserved entries forbidden), with `Action::SecretShare` on the sender and verified preview + `Action::SecretPackageImport` on the receiver.

## 4. Ephemeral Blind Relay (`dmsg-nearby-relay/1`)

- `POST /v1/accounts/:account/nearby-relays` (Device PoP + `dmsg/nearby-relay/create/v1`, max 2 active rooms, 6 creates/minute per account)
- `POST /v1/accounts/:account/nearby-relays/:rid/cancel` (Creator Device PoP + `dmsg/nearby-relay/cancel/v1`)
- `GET /v1/nearby-relays/:rid/ws` (Rejects `Origin`, authenticates either peer via `Authorization: Bearer <base64url(token)>`, relays opaque binary frames with zero R2 persistence)
