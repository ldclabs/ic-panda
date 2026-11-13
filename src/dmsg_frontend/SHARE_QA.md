# File sharing and reading validation — 2026-11-13

This change was developed on `ic-panda@f8624292`. It adds filename/type metadata before content retrieval, inline text/Markdown reading, raster image previews, a white full-screen preview dialog, and a shared file-sharing/file-request invitation in all 23 locales. Extensions are used for format detection but are not shown as a separate field.

The browser decrypts only the manifest initially. `GET /public/shares/:share/metadata` authorizes no content chunks and consumes no download session. Reading or downloading explicitly starts the existing session; subsequent saves use the same verified Blob. Password links open metadata only after unlock. No plaintext or external preview service is introduced. The public contract is in [cloud.md](../../docs/protocol/cloud.md).

Text preview is limited to 2 MiB; PNG/JPEG/GIF/WebP/AVIF preview to 20 MiB. Larger and unsupported files retain download. UTF-8 and BOM-marked UTF-16 are supported; invalid/binary text offers download. HTML/SVG are displayed as text, never active documents. Markdown disables raw HTML and embedded image requests; external links use a new tab with no opener/referrer.

## Checks actually run

- Frontend `pnpm check`: no errors or warnings from svelte-check.
- Frontend `pnpm test`: 156 Vitest tests passed, two environment-gated live tests skipped; seven Node tests passed. The repository-pinned pnpm 10.15.1 was used from its installed executable.
- Frontend `pnpm build`: passed, including public content generation and checks. Existing dependency sourcemap and Vite deprecated-option/build notices remain.
- Tests cover metadata-only reads and bounded renewal, URL/digest validation, session reuse, password retries, no rendering before full-file verification, unmount cleanup, full-screen/source state and focus restoration, inactive Markdown rendering, preview limits, and complete locale catalogs.
- Browser verification used the production build in the Codex in-app browser with a local fixture server, real AES-GCM/CBOR file encryption and Argon2 password wrapping. Test data and keys were synthetic; this was not a production service check.
- Before reading, fixture counters showed one metadata read and zero sessions/content reads. Reading and then saving showed one session and one content read. The saved Markdown was 1,920 bytes; the saved PNG was byte-for-byte identical to the 14,239-byte source fixture.
- Checked full filename/type display, Markdown rendering, plain text with literal HTML, password failure/retry, image decoding, full-screen text/images, download inside full-screen, Escape exit and focus restoration. Inline/full-screen reading backgrounds were confirmed white.
- Inspected Chinese desktop (1280×900) and Chinese/Arabic full-screen at 320×800; the German request invitation was inspected at 320×800. No horizontal overflow was observed. Native viewport screenshots were used to recheck a malformed full-page capture; the actual request card was 288px wide in the 320px viewport.
- The request-page invitation was checked in its unavailable-service state. Existing request upload behavior was not changed.

## Limits and release order

No production deployment, real R2/browser integration, live file-request upload, Safari/Firefox or mobile-device run was performed. The website retains its existing light theme; desktop Electron behavior was not changed or tested.

Deploy the metadata endpoint before the updated website. A metadata failure shows retry and never silently uses a counted download. Existing encrypted links need no format migration. Private service implementation and validation remain in its own repository.

## Progressive loading follow-up — 2026-11-13

Developed on `294bb2aa`. Both link pages render their shared invitation immediately. The share page starts the public descriptor and metadata-URL authorization concurrently, shows size/expiry/count as soon as the descriptor arrives, and adds the full filename/type only after the encrypted manifest verifies and decrypts. The prefetched grant is reused rather than requested again; a password delay that outlives its URL causes a normal uncounted renewal. Password protection, session counting and the server contract are unchanged.

Validation: `pnpm check` passed with zero Svelte errors/warnings; `pnpm test` passed 160 Vitest and seven Node tests, with two gated live tests skipped; production build and public-content checks passed. New regressions cover pending descriptor/authorization phases, early rejected authorization, prefetched grant reuse and expiry during password entry. No backend source or contract changed, so backend checks were not rerun.

The production build was checked in the in-app browser against synthetic encrypted fixtures with independently held network responses. Descriptor and metadata authorization reached the fixture server before either response was released (the same millisecond in this run). The invitation remained visible with both pending; releasing only the descriptor displayed public facts while the filename/read buttons remained hidden. After metadata completed, the name/type appeared with zero download sessions/content reads. The request page also displayed its invitation while its status request was held. A deliberately prolonged wait exercised the timeout/retry path. Screenshots were inspected; this verifies request overlap and progressive display, not a measured production latency reduction. No deployment or live R2 integration was performed.

## Optional sharer profile — 2026-11-13

Developed on `06e24ecb`. Desktop, iOS and CLI link creation can explicitly opt in to a public profile. The default is anonymous; existing encrypted records, commands and projections with no choice remain anonymous. The service derives the profile account from authenticated creation. The page immediately offers the profile link and independently loads the existing certified profile reader's name/avatar. It does not describe the file as safe or the creator as its author.

Checks actually run:

- Frontend `pnpm check`, production build and public-content checks passed. Tests passed: 164 Vitest and seven Node; two environment-gated live tests skipped. New regressions cover delayed/failed profile reads, reading and saving while pending, anonymous links, invalid account targets, and aborting on navigation.
- Rust formatting and CLI/mobile compilation passed. Client tests: 625 passed, five ignored; CLI: 123 passed, six ignored. The new record/command test verifies explicit opt-in survives recovery while anonymous commands retain their shape.
- Desktop check/build and all 200 tests passed. Isolated mock-daemon Electron transfers smoke passed, including default-off, Space-key selection, disabled-during-creation, retained choice after failure and the submitted flag. Screenshots were inspected in Chinese light/dark, French long copy and Arabic RTL, including the 760px minimum width.
- iOS release bridge/xcframework regenerated and the app compiled. Twelve file-library unit tests and two local/cloud link UI tests passed on the SE-sized iOS 18.4 simulator. The UI tests initially tapped the SwiftUI row's label whitespace; targeting the actual switch fixed the driver. Final terms and failure/retry screenshots were inspected; all 23 locale catalogs passed validation.
- In-app browser production-build checks used real encrypted synthetic files and a local mock of the already-verified profile worker result. Metadata and reading worked while profile resolution was held; name/avatar appeared when a fixture result was available. Anonymous navigation showed no profile row and caused no extra profile query. Chinese screenshots were inspected. The browser download-event listener timed out, so this follow-up does not claim a newly verified saved file; saving while a profile is pending is covered by component tests.

No production deployment or live service/R2/ICP-profile integration was performed. iOS physical devices, iPad, other OS versions and the full iOS suite were not run. Private service checks and implementation remain in its own repository. Deploy service support for the optional creation field before updated clients; no existing link is backfilled or relabeled.
