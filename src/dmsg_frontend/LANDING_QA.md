# Landing and legacy archive integration

The marketing route is isolated from legacy authentication and token-price initialization. It preserves the original IC asset canister `index.html` SPA fallback so existing `/[handle]`, `/_/[channel]`, profile and authentication URLs remain routable.

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
- `npm test`: 8 tests cover rejection before network submission, allowed authentication/recovery/download updates, historical namespace fallback, absent/error key states, and synthetic Local/ECDH decryption without initialization.
- `npx svelte-check --tsconfig ./tsconfig.json`: 0 errors; 26 existing component warnings remain, primarily initial prop captures.
- `git diff --check`

Tests use synthetic keys and mocked transport/storage; they do not use a user's account or contact deployed services. The COSE package is inlined in Vitest because its published ESM uses extensionless internal imports. Its missing upstream source maps produce non-failing test warnings.

## Browser checks

Checked the built page at 320px, 390px, 768px and 1440px: no horizontal overflow in the landing content. Verified capability tabs, secret show/hide, lock/unlock with focus restoration, signature rejection and simulated approval, contact-rule changes, mobile navigation with Escape, dialog Escape/focus restoration, the legacy entry, and its login chooser at 320px. No real account sign-in, signature or payment was performed.

Local legacy service calls require a running replica. Without one, the entry page still renders; authenticated history and real attachment recovery cannot be verified in that environment.
