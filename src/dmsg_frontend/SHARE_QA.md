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
