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
review date and verify live terms before changing prices. Backup file limits
follow the public cloud contract (100 MiB for Free/Plus/Pro, 1000 MiB for Max)
and appear in both the interactive and generated static comparisons; this policy
update does not change the recorded pricing review date. Desktop and CLI links follow
the latest release without duplicating its version. The iPhone/iPad app links to its
public App Store listing using a region-neutral URL. Keep the interactive download
links, bilingual guides and generated structured data in sync. `scripts/public-content.mjs`
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

## File request page

`/r/[account]/[id]` takes files from people with no account and nothing else: it shows the requester's verified profile and the link's title and instructions, encrypts each file in the browser and uploads it through the relay's `/public/requests` routes (`src/lib/request`). The cookie recognizes this browser on a return visit; records open only with both the cookie and the link's fragment, so nobody reads back a file here. A chunk keeps retrying through a broken connection, and replacing uploads the new file before withdrawing the old one. `live.test.ts` runs the same code against a live relay from the desktop end-to-end test `desktop_file_request`; without `DMSG_REQUEST_LIVE` it is skipped. See the public [cloud protocol](../../docs/protocol/cloud.md).

## SEO / GEO review and URL notifications

Reviewed on 2026-11-10. The existing public HTML, bilingual guides, source links,
content hashes and explicit product limitations are the foundation for search
and answer-engine discovery. Generated reading pages now have unique section
fragments, plain-text descriptions, reciprocal language alternatives with an
English `x-default`, and page-specific Open Graph metadata. Homepage canonical
and Open Graph URLs match the sitemap's trailing slash. File-request URLs (`/r/`)
join share and authentication routes in robots exclusions; robots rules are
crawl guidance, not access control or a guarantee of removal from an index.

The owner-provided `static/6eb632e3a1a444e9a68ec57d3d650e21.txt` is an **IndexNow
host-verification file**, not a Bing Webmaster account API credential. It is
published as plain text at the domain root with revalidation headers. When
rotating it, replace the file and update its asset-header rule and build check.

After building and deploying the same `dist` directory:

```sh
pnpm indexnow                          # preview payload only; no network calls
pnpm indexnow --submit                 # notify the three public sitemap URLs
pnpm indexnow --submit https://dmsg.net/_/about.zh.html  # one changed page
```

The command discovers the deployed build's key file and reads canonical HTML
URLs from its sitemap. Explicit URLs must be sitemap entries without queries or
fragments. Before any POST, it fetches the live key and selected HTML pages and
compares their contents with this build, rejecting redirects, stale deployments
and SPA fallback responses. Deploy the build unchanged; HTML rewriting by a CDN
will also fail this check. It reports HTTP 202 as pending key validation, HTTP
200 as received, and other statuses as failures. There are no automatic retries
or notifications during build/test. Notify only after actual content changes;
this helper does not handle deleted URLs (use the documented API after serving
404/410 at those URLs).

Remaining deployment work:

- Verify the site in Bing Webmaster Tools and submit `https://dmsg.net/sitemap.xml`.
  Use URL Inspection to confirm crawlability and monitor indexing separately from
  successful URL submission. No Webmaster account verification was performed here.
- Dynamic profiles, legal/support routes and unknown paths still receive the
  generic homepage SPA shell without JavaScript. Profiles need route-specific
  server HTML and canonicals to become useful search results; unknown URLs need
  real HTTP 404 responses. Fix this at the serving/routing layer before adding
  such routes to the sitemap. A client-side metadata change cannot fix the
  initial response. The shared fallback also duplicates the English reading
  guide; keep monitoring canonical selection in URL Inspection.
- `llms.txt` helps readers discover references but does not guarantee AI citations.
  Do not manufacture review dates, ratings, FAQ rich-result claims or sitemap
  `lastmod` timestamps from the current build time. Measure crawl/index status
  and citations after deployment; local checks cannot establish ranking gains.

Official references: [Bing URL submission](https://www.bing.com/webmasters/help/URL-Submission-62f2860b),
[IndexNow protocol and response codes](https://www.indexnow.org/documentation),
[Bing setup and indexing limits](https://www2.bing.com/indexnow/getstarted), and
[Bing sitemap guidance](https://www2.bing.com/webmasters/help/sitemaps-3b5cf6ed).
