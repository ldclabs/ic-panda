# Landing and legacy archive integration

The marketing route is isolated from legacy authentication and token-price initialization. It preserves the original IC asset canister `index.html` SPA fallback so existing `/[handle]`, `/_/[channel]`, profile and authentication URLs remain routable.

The root route loads the initial language catalog before rendering, so SvelteKit can locate fragment targets and restore scroll. The static HTML includes an English title, description and share image; the client localizes the existing description in place and supplies default/page titles without duplicate metadata.

## What the page claims

The page follows `DESIGN.md`. It describes dMsg as one local program, `dmsg`: a daemon that holds the keys and the approval queue, reached by the desktop app, the command line, AI agents over MCP and websites through `connect.dmsg.net`. There is no browser extension. The desktop app and command line are in development with no public release: the install commands are shown under a note that they work only once the first `dmsg-v*` release is published, and the platforms are those of the release scripts (macOS Apple Silicon first; Windows x64; the command line on Linux). `/legacy` remains the read-only entry for old messages.

Facts on the page come from the public code and its READMEs: the 56 tools of `dmsg mcp` (33 standard Agent Protocols tools and 23 `dmsg_*`), the 15 agent capabilities (only public profiles without a grant), the capabilities no agent can hold, the `dmsg` commands and output shapes, the approval sources `gui`/`cli`/`agent:`/`connect:`, the second click before a secret is shown or copied, the 15-minute idle lock, recovery by a bound login after a one-to-seven-day disputable delay, and the dmsg-connect/1 moves and trust point. No audit, release, user count, signature, payment or successful backup is claimed.

## Figures

Every figure runs in the browser with sample data and says so in its caption. None contacts a service.

- **Fig. 1, the gate** (`GateFigure.svelte`): a canvas simulation. Everyday requests travel from the four sources through the daemon and leave as scrambling ciphertext for the cloud; a request that needs a person waits at the gate with a 15-second expiry and a card with Reject and Approve. Only the visitor's approval sends a signature to ICP; “Signed without you” stays 0. Clicking a source sends a request. The loop runs only while the figure is on screen and the tab is visible.
- **Fig. 2, desktop and terminal** (`DeviceDuo.svelte`): one simulated daemon behind a desktop window and a terminal that accepts a few `dmsg` commands (`status`, `vault list|show|add`, `lock`, `unlock`, `attest file`, `approvals list`, `help`). `attest file` opens the approval sheet in the window; either side can decide. Locking in one pane is enforced in the other.
- **Fig. 3, agents** (`AgentPlayground.svelte`): capability switches, a scripted MCP run whose calls succeed or fail with `FORBIDDEN` according to the switches, and an audit record of grants and calls.
- **Fig. 4, websites** (`ConnectFlow.svelte`): the dmsg-connect/1 sequence, which stops at the approval window until the visitor approves or rejects.
- **Fig. 5, privacy** (`CipherLanes.svelte`): the visitor's text is encrypted in the browser with a throwaway AES-GCM key to show what the cloud holds, including its size; the note says dMsg's own format differs.

With `prefers-reduced-motion: reduce` the figures show still scenes that remain interactive, the headline and sections appear without motion, and smooth scrolling is off.

## Automated checks

Run from `src/dmsg_frontend`:

- `npm run check` (`svelte-kit sync`, then `svelte-check --fail-on-warnings`): 1615 files, 0 errors, 0 warnings.
- `npm test`: 41 tests in 10 files, including catalog completeness and identical parameters in all six languages, and the source coverage test (no unregistered visible English, no literal accessibility labels).
- `npm run build`.

## Browser checks

Headless Chrome for Testing 1243 (Playwright) against `vite dev`:

- No horizontal overflow in any of the six languages at 320, 360, 390, 481, 600, 768, 1000, 1280 and 1440px.
- No page or console errors in English at 1440 and 390px, Chinese, Russian and Arabic at 1440px, French at 390px, and with reduced motion.
- Section captures in English at 1440 and 390px and Arabic at 1440px; hero captures in Chinese and Russian. Arabic mirrors the layout while diagrams, commands and hashes stay left to right, and Arabic copy is never set in monospace or tracked.
- Reduced motion: every revealed element is visible, the waiting request can be approved and the next one appears, and the terminal prints its introduction.

In the Claude desktop browser pane: approving and expiring gate requests, running `dmsg attest file` and approving in the window, the agent run with a refused capability, the website sequence through approval, and typing in the privacy figure (2 bytes of text sealed to 30 bytes).

Not run: Firefox and WebKit, real devices, and screen readers. The legal pages were checked only for unchanged class usage after the shared stylesheet was split (`site.css` for site chrome, `home.css` for the landing page).

Local legacy service calls require a running replica. Without one, the entry page still renders; authenticated history and real attachment recovery cannot be verified in that environment.
