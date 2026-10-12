# dmsg_connect

`connect.dmsg.net`, the bridge page of
[dmsg-connect/1](../../docs/protocol/connect-v1_zh.md): an application page
opens it in a popup, and it hands the page's requests to the user's dMsg
devices through the relay. It runs on its own asset canister (`dmsg_connect`
in `icp.yaml`) and domain, has no login and never touches Internet Identity.

| Path | Role |
| --- | --- |
| `/v1/` | The bridge window: the hello from `window.opener`, the origin the browser reports, one nonce and proof per command, results to that origin only |
| `/v1/pair#…` | The page a device opens to connect this browser to an account; the fragment is cleared as soon as it is read |

## Modules

| Module | Role |
| --- | --- |
| `src/codec.ts` | Canonical CBOR (cborg), hex, base64 and base64url as the protocol spells them |
| `src/crypto.ts` | WebCrypto P-256 (non-extractable pairing keys), ES256 COSE_Sign1, HMAC, AES-256-GCM; HPKE with native X25519 (`@hpke/core`): base mode to devices, auth mode from devices; non-extractable result keys |
| `src/protocol.ts` | Commands, the proof and operation digests, pairing IDs and MACs, the sealing contexts, invitations and replies |
| `src/certified.ts` | Certified reads under a pinned IC root: the registry's user homes, an account's devices from its home, an application's registration from commerce (checked with `dmsg-sdk`) |
| `src/relay.ts` | The relay's `/v1/connect/*` routes with a Connect PoP |
| `src/bridge.ts` | Pairing and grant checks, the eight methods with the §4.5 checks (`dmsg-sdk` validators), sealing, reply verification and pinning, recovery without a local record |
| `src/store.ts` | Pairings and operations in IndexedDB `dmsg-connect:<environment>` (keys stay CryptoKey objects), or in memory |
| `src/page.ts` | The window's frames (§4.2–§4.3), without DOM |
| `web/` | The two pages (`shared.ts`, `bridge.ts`, `pair.ts`), plain DOM, no inline script or style |

The bridge trusts only what is signed by the page's session key or certified
by the IC; the relay is a mailbox. Replies open only with HPKE in auth mode
under a device key from the certified set, and once a device holds an
operation, only that device's replies count.

## Build

The environment is inlined at build time from a JSON file,
`DMSG_CONNECT_CONFIG` (default `connect.config.json`):

```json
{
  "environment": "Production",
  "icHost": "https://icp-api.io",
  "apiOrigin": "https://<relay API origin>",
  "canisters": { "handle": "<dmsg_handle>", "commerce": "<dmsg_commerce>" },
  "domains": ["connect.dmsg.net"]
}
```

Staging and Production builds refuse missing or non-HTTPS values; only a Local
build takes `rootKey` (the replica's root key DER, hex) and loopback HTTP.
The build writes `dist/.ic-assets.json5` with the headers of connect-v1 §2.2
(the CSP's `connect-src` is exactly `icHost` and `apiOrigin`, COOP
`unsafe-none`, no COEP, `frame-ancestors 'none'`, `Referrer-Policy:
no-referrer`; the asset canister's own security policy is disabled so it
injects nothing that conflicts) and `dist/.well-known/ic-domains` from
`domains`. `DMSG_CONNECT_OUT` changes the output directory.

```sh
pnpm --dir packages/dmsg-sdk build           # the bridge imports dmsg-sdk
DMSG_CONNECT_CONFIG=connect.config.json pnpm --dir src/dmsg_connect build
pnpm --dir src/dmsg_connect serve            # dist with the canister's headers and aliases
icp deploy dmsg_connect -e ic                # uploads dist; record the asset hashes
```

The relay must list the page's origin as `CONNECT_ORIGIN`, and daemons use it
as `connect_url` (default `https://connect.dmsg.net`).

## Tests

```sh
pnpm --dir src/dmsg_connect test     # connect-v1 vectors; the window's frames (DT20, DT21)
pnpm --dir src/dmsg_connect check
```

`scripts/headless.ts` runs the bridge in Node over JSON lines (it trusts the
origin its driver names, so it is test-only); `scripts/browser.ts` drives the
real pages and the test application (`e2e/app`) in Chromium, Firefox or
WebKit through Playwright. Both are driven from
`tests/dmsg_integration/tests/desktop.rs`: `desktop_web_connect` (pairing and
phishing, locked devices, approval on another device, cancel and authorize
races, two browsers on one operation, lost records, crashes and revocation)
and `desktop_browser_connect` (headers, pairing, an unregistered origin, the
handshake, a signature approved on a device, acknowledgement and a closed
window, per browser in `DMSG_TEST_BROWSERS`, default `chromium`). Install the
browsers with `pnpm --dir src/dmsg_connect exec playwright install chromium
firefox webkit`, or point `DMSG_TEST_CHROME` at an installed Chrome for
Testing.
