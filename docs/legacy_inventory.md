# Read-only legacy deployment inventory

English | [简体中文](legacy_inventory_zh.md)

`src/dmsg_legacy/scripts/legacy-inventory.mjs` reads the configured canister IDs and discovers deployed instances through public message, channel, profile and OSS cluster references. It always uses an anonymous identity, the mainnet certificate root and query-signature verification. It sends only `read_state` and allowlisted queries, reads no local identity keys and sends no updates.

Run from the public repository root, using the workspace's installed Node modules:

```sh
node src/dmsg_legacy/scripts/legacy-inventory.mjs \
  --ids .icp/data/mappings/ic.ids.json \
  --output misc/legacy-inventory/new-observation
node --test src/dmsg_legacy/scripts/legacy-inventory.test.mjs
```

Use a new output directory. Evidence files are created exclusively to avoid overwriting existing records; inside the public checkout, output is allowed only in Git-ignored directories. `--host` defaults to `https://icp-api.io`, accepts HTTPS only and never fetches a custom root key. `--max-canisters` defaults to 64 and is capped at 256 to bound discovery; the tool does not recursively scan a controller's other services.

The output includes:

- Each instance's module hash, controllers, published Candid and BLS read_state certificate, plus SHA-256 hashes of the evidence files.
- Node-signature-verified service counts, routing, roles and configuration projections; unrelated fields such as latest_usernames in get_state are omitted.
- COSE cached public-key queries without derivation input, and SHA-256 fingerprints of the raw public-key bytes. These are not RFC 9679 COSE_Key thumbprints and do not prove that the configured key name matches the management canister's current derivation root.
- Clusters/buckets referenced by Channel/Profile, cluster-published bucket lists and deployment summaries. Deployment argument bytes are not included in the report.
- Failed calls, missing fields and the collection time range. A null optional field means unavailable or omitted, not that the configuration/key is absent on chain.

A certified module hash is not evidence of a reproducible source build. To identify a release, separately download candidate release artifacts, compare their actual byte hashes, and record the tag commit and corresponding Candid. Neither the download version in icp.yaml nor the current checkout establishes the live version.

Service-state queries are node-signed observations, not certified frozen snapshots of all user data. Collection across instances is not an atomic cross-canister snapshot. Runtime memory, cycles, stable-state exports, complete ACLs, in-flight accounting, unfinished uploads and users' ability to decrypt still require their respective permissions and checks. The tool does not request tokens, execute vetKD/ECDH, log users in or export user content.

Actual instance inventories, governance roles, runtime counts, sample authorization records and recovery material belong in private operations storage or controlled local directories, not the public repository. This tool performs no freeze, upgrade, transfer or service shutdown.
