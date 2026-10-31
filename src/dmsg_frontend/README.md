# dMsg web frontend

The public profile, login/connect entry points, verification pages and encrypted file share reader. Development commands are in package.json.

## Public reading and discovery

`pnpm build` generates a visible, JavaScript-independent product guide in the
SPA's `index.html`, English/Chinese reading pages at `/_/about.html` and
`/_/about.zh.html`, Markdown guides at `/index.md` and `/index.zh.md`,
`/llms.txt`, `/llms-full.txt`, `/robots.txt`, `/sitemap.xml`, and the build manifest
`/_/knowledge.json`. The footer and HTML link relations expose these entry points.

The existing asset canister serves `index.html` for dynamic links, including
profiles, login and legacy routes. Its bootstrap stays unchanged: the root layout
removes the visible static guide only after the client route mounts. Do not enable
SSR globally or replace the fallback with a prerendered homepage: locale state is
client-scoped and non-home routes still need the SPA. Static reading pages have no
client bootstrap. The readable fallback is English even when the browser chooses
another language for the app; the reading pages explicitly identify their language.

Edit `content/overview.md` and `content/overview.zh.md` together when product
behavior changes. Membership figures are shared with the landing page in
`src/lib/components/landing/membership.json`; retain the actual configuration
review date and verify live terms before changing prices. Installation links follow
the latest release without duplicating its version. `scripts/public-content.mjs`
copies only an explicit allowlist of public repository documents into `/_/docs/`;
links to other snapshotted documents resolve locally and other relative links
resolve against the public repository. No private repository is a build input.

The manifest records the base Git revision, whether knowledge inputs have local
changes, generation time, original-source hashes and published Markdown hashes.
These are provenance, not proof of production deployment or a fresh review of an
older document. Keep source documents' own review dates and limitations. Dynamic
profiles, shared files and login flows are not advertised in the sitemap.

The asset configuration gives HTML and discovery documents explicit MIME types
and revalidation headers. They are not precached by the service worker. Do not add
these mutable guides to its precache without preserving network freshness.

`pnpm build` ends with checks of the actual output: readable HTML without scripts,
links, shared prices, structured data, provenance, sitemap and response-header
configuration. CI runs this build. `pnpm preview` uses `vite.preview.config.js`
to serve the actual `dist` assets and SPA fallback (SvelteKit's default preview
serves its intermediate output). It does not emulate IC response headers. After the normal
asset deployment, verify `curl https://dmsg.net/`, `/llms.txt`, `/index.md` and
`/_/knowledge.json` as well as a direct login/profile link. Local build success does
not verify deployment or force search engines and agent providers to refresh
their indexes. `llms.txt` is supplementary discovery, not an indexing guarantee.

## Encrypted share downloads

The share page starts one random download session and fetches its short-lived S3 URLs without cookies or referrers. Lost start replies reuse the same session ID; renewal uses GET and does not consume another download count. URL expiry or a missing block triggers one authority recheck; corruption fails without saving a file. `src/lib/share/download.ts` validates the descriptor, size and digest; the decryptor checks AEAD and the whole file. The deployed CSP allows HTTPS R2 S3 connections. The bucket must allow the actual share-page origin through CORS. Session IDs and URLs stay in page memory and must not enter logs. See the public [cloud protocol](../../docs/protocol/cloud_zh.md) for expiry and the bounded revocation window.
