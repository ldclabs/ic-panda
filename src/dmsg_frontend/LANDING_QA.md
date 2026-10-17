# Landing and legacy archive integration

The marketing route is isolated from legacy authentication and token-price initialization. It preserves the original IC asset canister `index.html` SPA fallback so existing `/[handle]`, `/_/[channel]`, profile and authentication URLs remain routable.

The root route loads the initial language catalog before rendering, so SvelteKit can locate fragment targets and restore scroll. The static HTML includes an English title, description and share image; the client localizes the existing description in place and supplies default/page titles without duplicate metadata.

## What the page claims

The page follows `DESIGN.md`. It describes dMsg as one local program, `dmsg`: a daemon that holds the keys and the approval queue, reached by the desktop app, the command line, AI agents over MCP and websites through `connect.dmsg.net`. There is no browser extension. The desktop app and command line are released. The platforms are those of the `dmsg-v*` release assets: the desktop app for macOS on Apple Silicon and Windows x64, the command line also on Linux x86_64 and arm64; Windows is marked less tested because the release workflow does not verify Windows builds yet. The download buttons point at `releases/latest/download/dMsg-mac-arm64.dmg` and `dMsg-win-x64.exe`, the repository's latest release is always a `dmsg-v*` release, and the install commands install the latest release, so the copy names no version and a new release needs no page change. `/legacy` remains the read-only entry for old messages; migration starts in the desktop app under Settings › Legacy, which the FAQ, the release section and the support page say.

Facts on the page come from the public code and its READMEs: the 65 tools of `dmsg mcp` (33 standard Agent Protocols tools and 32 `dmsg_*`, `src/dmsg_agent/src/tools.rs`), the 26 agent capabilities (`CAPABILITIES` in `src/dmsg_client/src/agents.rs`; public profiles need no grant), the capabilities no agent can hold, the `dmsg` commands and output shapes, the approval sources `gui`/`cli`/`agent:`/`connect:`, the second click before a secret is shown or copied, the 15-minute idle lock, recovery by a bound login after a one-to-seven-day disputable delay, and the dmsg-connect/1 moves and trust point.

The vault holds notes, logins, API keys and keys; files live in the file library (`src/dmsg_cli/README.md`, Files, Backups, Links, Mount): files stay on the device, up to 4 GiB each; only Back up uploads, as `kind=backup` objects under random IDs and at most 100 MiB a version; deleting on one side never reaches the other; the trash keeps 30 days; a share to a channel or by link is a fresh copy under its own key; an opened copy goes after 30 minutes or on lock; the library can be mounted as a disk; agents get `files.read|write:<folder>`, and no tool backs up, shares or deletes. Team wallets follow `docs/protocol/wallet_zh.md` and `src/dmsg_wallet/README.md`: one canister per team; ICP and ICRC transfers by the canister and threshold signatures for Bitcoin, EVM chains and Solana; spending tiers by amount with approvals and a 24-hour delay any approver can veto; an address book whose new entries wait; agents propose and execute (`wallet.propose`, `wallet.execute`) and no capability approves; cosigned addresses (Secure Enclave on a Mac) and dual-control Taproot addresses; creation needs a paid plan and a one-time ICP activation. The page says wallets are in development and not audited. The file library and team wallets landed on main after `dmsg-v0.1.0`; by choice the page shows them without a version. No audit, user count, signature, payment, backup or wallet deployment is claimed.

## Figures

Every figure runs in the browser with sample data and says so in its caption. None contacts a service.

- **Fig. 1, the gate** (`GateFigure.svelte`): a canvas simulation. Everyday requests travel from the four sources through the daemon and leave as scrambling ciphertext for the cloud; a request that needs a person waits at the gate with a 15-second expiry and a card with Reject and Approve. Only the visitor's approval sends a signature to ICP; “Signed without you” stays 0. Clicking a source sends a request. The loop runs only while the figure is on screen and the tab is visible.
- **Fig. 2, desktop and terminal** (`DeviceDuo.svelte`): one simulated daemon behind a desktop window and a terminal that accepts a few `dmsg` commands (`status`, `vault list|show|add`, `lock`, `unlock`, `attest file`, `approvals list`, `help`). The window's sidebar lists the desktop's views, Files and Team wallets included; the vault has no file items. `attest file` opens the approval sheet in the window; either side can decide. Locking in one pane is enforced in the other.
- **Fig. 3, files** (`FileLibrary.svelte`): the library on this device beside what the cloud holds. Back up asks first with the room it takes, then uploads under a random ID; a trashed file keeps its cloud copy; deleting a cloud copy takes a second click and leaves the file; a backup from another device gets its name only when fetched. “Uploaded without you” and “Deletions that crossed over” stay 0.
- **Fig. 4, team wallets** (`TeamWallet.svelte`): a BTC transfer an agent proposed for a member. The amount picks the tier (1 of 2 approvals, or 2 of 2 and a 24-hour delay); the visitor approves or rejects, plays the second approver, vetoes or skips the delay, and executes: the wallet signs, the device broadcasts, the proposal settles, and every step lands in a hash-linked audit chain. A rejection turns it down only once too few approvers remain. Nothing happens without a click.
- **Fig. 5, agents** (`AgentPlayground.svelte`): capability switches, a scripted MCP run whose calls succeed or fail with `FORBIDDEN` according to the switches (a file-library write among them), and an audit record of grants and calls.
- **Fig. 6, websites** (`ConnectFlow.svelte`): the dmsg-connect/1 sequence, which stops at the approval window until the visitor approves or rejects.
- **Fig. 7, privacy** (`CipherLanes.svelte`): the visitor's text is encrypted in the browser with a throwaway AES-GCM key to show what the cloud holds, including its size; the note says dMsg's own format differs.

With `prefers-reduced-motion: reduce` the figures show still scenes that remain interactive, the headline and sections appear without motion, and smooth scrolling is off.

## Automated checks

Run from `src/dmsg_frontend`:

- `npm run check` (`svelte-kit sync`, then `svelte-check --fail-on-warnings`): 1627 files, 0 errors, 0 warnings.
- `npm test`: 44 tests in 11 files (one file skipped), including catalog completeness and identical parameters in all six languages, and the source coverage test (no unregistered visible English, no literal accessibility labels).
- `npm run build`.

## Browser checks

Headless Chrome for Testing 1243 (Playwright) against `vite dev`:

- No horizontal overflow in any of the six languages at 320, 360, 390, 481, 600, 768, 1000, 1280 and 1440px. On 2026-10-17 the header and the Desktop, Files, Team wallets and Agents sections were checked again in all six languages at 320, 360, 390, 481, 600, 768, 1000, 1001, 1100, 1280 and 1440px, with a cloud copy's delete armed (its longest label) and a wallet approval given.
- No page or console errors in English at 1440 and 390px, Chinese, Russian and Arabic at 1440px, French at 390px, and with reduced motion.
- Section captures in English at 1440 and 390px and Arabic at 1440px; hero captures in Chinese and Russian. Arabic mirrors the layout while diagrams, commands and hashes stay left to right, and Arabic copy is never set in monospace or tracked.
- Reduced motion: every revealed element is visible, the waiting request can be approved and the next one appears, and the terminal prints its introduction.
- Fig. 3 and Fig. 4, in English at 1440px with and without reduced motion: Back up asks first, then the copy appears in the cloud under a random ID; a trashed file keeps its cloud copy; a fetched backup gets its name; a cloud copy goes only on the second click and its file stays; the wallet's large transfer is delayed after two approvals, the delay can be vetoed or skipped, and execution ends settled with eight audit events; with the small amount one rejection leaves the proposal open, with the large one it turns it down. Captures of both figures in English at 1440 and 390px, Chinese at 1440px and Arabic at 1440px, where amounts and sizes keep their left-to-right order.

- Release (2026-10-17): both `releases/latest/download/` links answered 200 with the `dmsg-v0.1.0` assets; the Homebrew cask and formula are at 0.1.0. The hero, the release section and the trust note were captured in English at 1440 and 390px, Chinese and Arabic at 1440px; the header, hero, Files, Team wallets, release, trust and FAQ sections show no horizontal overflow or page errors in all six languages at 320, 390, 600, 768, 1001, 1280 and 1440px. `og-image.png`'s corner label was repainted from “In development” to “Available now” with the label's own font settings (fitted against the first line; the redrawn old line differed from the original by 0.3% of its ink).

- Header and brand: the bar is 64px (56px at 760px and below) on the landing page at 1440, 1000 and 390px, in Chinese and Arabic at 1440px and Russian at 760px; the legacy entry and the privacy policy wrap their navigation below the brand on phones. The Gate mark keeps its orientation in Arabic, its diamond turns a quarter on hover (instantly with reduced motion), and `icon.svg` switches to Mint in a dark color scheme.

In the Claude desktop browser pane: approving and expiring gate requests, running `dmsg attest file` and approving in the window, the agent run with a refused capability, the website sequence through approval, and typing in the privacy figure (2 bytes of text sealed to 30 bytes).

Not run: Firefox and WebKit, real devices, and screen readers. The legal pages were checked only for unchanged class usage after the shared stylesheet was split (`site.css` for site chrome, `home.css` for the landing page).

Local legacy service calls require a running replica. Without one, the entry page still renders; authenticated history and real attachment recovery cannot be verified in that environment.
