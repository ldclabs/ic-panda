# dmsg-nearby-share/1: Face-to-Face Nearby Share

English | [简体中文](nearby_share_zh.md)

The core protocol lives in `dmsg_client::nearby`, and its wire bytes are pinned by
[nearby-v1.json](../../tests/vectors/nearby/nearby-v1.json).

Two physically nearby dMsg devices (different accounts, same account, or offline Standalone workspaces) transfer selected library files or a `dmsg-secret-package/1` Vault snapshot over an end-to-end encrypted channel.

## 1. Transport and Discovery

- **QR-only by default**: Opening Receive listens locally and displays a `dmsg-nearby/1:` QR code (and copyable offer text) without broadcasting mDNS.
- **Opt-in local discovery**: When the receiver enables discoverability, it advertises `_dmsg-share._tcp` with a 16-hex ephemeral instance name and TXT keys `v=1`, `n=<alias <= 32 UTF-8 bytes>` and `p=<4-byte ephemeral pk hint>`. No `AccountId`, handle or permanent device ID is advertised.
- **Parallel racing with blind relay fallback**: Online logged-in receivers asynchronously open an ephemeral blind relay room (zero R2 storage, a join deadline equal to the offer's 270 s — the relay accepts at most 5 minutes, leaving 30 s for clock skew — and 100 MiB / 60 s idle / 15-minute transfer ceiling). The receiver waits in the room until a sender joins or the deadline passes, not for the 10 s handshake timeout. Senders dial private LAN candidates and mDNS/AWDL immediately, launching the relay after 1.8 s in parallel; the first completed handshake wins.
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
- In browse mode both devices display the SAS as soon as the handshake completes. The receiver can accept after the sender confirms and sends the offer. Acceptance closes competing connections. An unused receive window closes at the advertised deadline across all transports.

## 3. Framing, Credit Window and Payloads

- Post-handshake frames use AES-256-GCM with `nonce = direction || 0x000000 || seq_be` and `aad = CBOR(["dmsg/nearby-share/frame/v1", sid, direction, seq])`, max plaintext `1 MiB + 64 KiB`.
- Receivers grant initial chunk credits (`W = 4`) in `Accept` and replenish every 2 chunks via `Credit`.
- **Files**: No high-risk approval; received through `library::import` (`free_name`, fresh `FileKey`, bounded by `files_max_bytes`, default 4 GiB, max 10 GiB / 64 files per session), committed and acknowledged per file (`FileAck`).
- **Vault items**: Reuses `dmsg-secret-package/1` (max 100 MiB, 8 MiB on mobile, <= 64 fields, <= 20 attachments, populated fields/notes and locally available attachments pre-selected with opt-out, `dmsg-` reserved entries forbidden). Only after the receiver confirms the 6-digit SAS and accepts does the sender create and publish the nearby snapshot job and stream it directly without approval; a timeout or cancellation stops the share and releases its staged data. The receiver previews the verified package and imports the confirmed selection atomically without approval.

## 4. Ephemeral Blind Relay (`dmsg-nearby-relay/1`)

- `POST /v1/accounts/:account/nearby-relays` (Device PoP + `dmsg/nearby-relay/create/v1`, max 2 active rooms, 6 creates/minute per account)
- `POST /v1/accounts/:account/nearby-relays/:rid/cancel` (Creator Device PoP + `dmsg/nearby-relay/cancel/v1`)
- `GET /v1/nearby-relays/:rid/ws` (Rejects `Origin`, authenticates either peer via `Authorization: Bearer <base64url(token)>`, relays opaque binary frames with zero R2 persistence)
