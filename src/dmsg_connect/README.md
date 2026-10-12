# @dmsg/connect

The bridge page's core for [dmsg-connect/1](../../docs/protocol/connect-v1_zh.md),
without a window: pairing a browser with an account, the checks a request must
pass before it leaves the browser, sealing it to the account's approving
devices, and opening and verifying their replies. Work package W4b builds the
`connect.dmsg.net` page (`/v1/`, `/v1/pair`) on it; until then it drives the
desktop end-to-end tests as a headless bridge.

| Module | Role |
| --- | --- |
| `codec.ts` | Canonical CBOR (cborg), hex, base64 and base64url as the protocol spells them |
| `crypto.ts` | WebCrypto P-256 (non-extractable pairing and page keys), ES256 COSE_Sign1, HMAC, AES-256-GCM; HPKE with native X25519 (`@hpke/core`): base mode to devices, auth mode from devices |
| `protocol.ts` | Commands, the proof and operation digests, pairing IDs and MACs, the sealing contexts, invitations and replies |
| `certified.ts` | Certified reads under a pinned IC root: the registry's user homes, an account's devices from its home, an application's registration from commerce |
| `relay.ts` | The relay's `/v1/connect/*` routes with a Connect PoP |
| `bridge.ts` | Pairing and grant checks, `call` for the eight page methods, reply verification and pinning, recovery without a local record |

The bridge trusts only what is signed by the page's session key or certified
by the IC; the relay is a mailbox. Replies open only with HPKE in auth mode
under a device key from the certified set, and once a device holds an
operation, only that device's replies count.

`scripts/headless.ts` plays one browser (the application page and the bridge
page) over JSON lines on stdin/stdout. It trusts the origin its driver names,
which a real page takes from `MessageEvent.origin`, so it is test-only.

```sh
pnpm --dir packages/dmsg-sdk build   # the bridge imports dmsg-sdk/ic
pnpm --dir src/dmsg_connect test     # connect-v1 vectors, both directions
pnpm --dir src/dmsg_connect check
```

The end-to-end scenarios (pairing and phishing, locked devices, approval on
another device, cancel and authorize races, two browsers on one operation,
lost records, crashes and revocation) are in
`tests/dmsg_integration/tests/desktop.rs` (`desktop_web_connect`).
