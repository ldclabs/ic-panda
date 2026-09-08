# Landing and legacy archive integration

The marketing route is isolated from legacy authentication and token-price initialization. It preserves the original IC asset canister `index.html` SPA fallback so existing `/[handle]`, `/_/[channel]`, profile and authentication URLs remain routable.

The root route loads the initial language catalog before rendering, so SvelteKit can locate fragment targets and restore scroll. The static HTML includes an English title and description; the client localizes the existing description in place and supplies default/page titles without duplicate metadata.

The website follows `DESIGN.md` and the supplied landing prototype. Product copy describes the Chrome extension and integrations as in development, uses local sample data, and offers `/legacy` as a read-only entry. No extension-store ID, installation detection, audit, real signature, payment, migration, or successful backup is fabricated.

## Archive scope

- `AuthAgent.call` denies business writes before the network; authentication, historical IV/key derivation and authorized download-token updates remain allowed. Queries and local authentication/cache storage remain usable.
- No new message/upload, read receipt, profile change, account registration, transfer or payment is offered by the archive UI.
- Existing Local/ECDH keys can unlock even for COSE-enabled accounts. Remote reads can retrieve an already-existing VetKey; missing remote material is not initialized. Old namespace lookups do not migrate or rewrite settings.
- Password unlock does not reset or replace keys or automatically migrate them. Existing login providers and name-account switching are retained.
- **This is a client restriction, not a deployed server freeze.** Canister-side business-write enforcement and authorized Local/ECDH/VetKey production recovery samples still need migration/release validation. This change does not deploy canisters or change production key configuration.

## Automated checks

Run from `src/dmsg_frontend`:

- `npm run build`
- `npm test`: 25 tests cover rejection before network submission, allowed authentication/recovery/download updates, historical namespace fallback, absent/error key states, synthetic Local/ECDH decryption without initialization, locale loading, catalog completeness, interpolation, and localized display data. The root workspace test run passes 41 tests across both frontends.
- `npx svelte-check --tsconfig ./tsconfig.json`: 0 errors; 24 component warnings remain, primarily initial prop captures.
- `git diff --check`

Tests use synthetic keys and mocked transport/storage; they do not use a user's account or contact deployed services. The COSE package is inlined in Vitest because its published ESM uses extensionless internal imports. Its missing upstream source maps produce non-failing test warnings.

## Browser checks

Checked the built page at 320px, 390px, 768px and 1440px: no horizontal overflow in the landing content. Verified capability tabs, secret show/hide, lock/unlock with focus restoration, signature rejection and simulated approval, contact-rule changes, mobile navigation with Escape, dialog Escape/focus restoration, the legacy entry, and its login chooser at 320px. No real account sign-in, signature or payment was performed.

The six-language regression pass checks landing header/content bounds at 320, 360, 361, 390, 481, 600, 601, 768, 1000, 1001 and 1280px. The legacy entry header is checked at 320, 390, 600, 601 and 768px. Russian navigation folds into a menu at 768px, while narrow legacy headers put the return link on its own row. Bounds are compared with the document's client width, including the space used by the scrollbar.

Fresh visits to `/#workspace`, `/#privacy`, `/#developers` and `/#release` position the requested section below the sticky header. Check after smooth scrolling has settled. All six languages retain exactly one title and one description after switching; the production fallback HTML also contains both before JavaScript runs. The OG image remains 1200 × 630. Chinese privacy and integration copy describes separate permissions and trust boundaries without claiming physical isolation or absolute privacy.

Local legacy service calls require a running replica. Without one, the entry page still renders; authenticated history and real attachment recovery cannot be verified in that environment.
