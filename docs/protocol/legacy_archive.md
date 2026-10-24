# Legacy archives and one-time migration files (v1)

English | [简体中文](legacy_archive_zh.md)

2026-10-13. This contract covers implemented read-only collection, client-encrypted handoff, desktop storage, offline reading and frozen comparison. It neither proves that old services are frozen nor grants name, device or shared-channel ownership. Recovery of real Local/ECDH/VetKey accounts still requires acceptance using authorized samples.

## Read boundary

`@dmsg/legacy` is independent of the old business stores. Calls restrict service class, method and inventoried canister together, and check the old Principal before and after every await. Reads and necessary recovery updates such as `my_iv`, legacy ECDH/vetKD and download-token retrieval are allowed. There are no create, setting-update, read-receipt, exchange-acknowledgement, PoL or initialization entry points.

The original site's IndexedDB is opened with read-only transactions: missing databases are not created and old databases are not upgraded. Archives preserve selected MK wraps, static ECDH wraps, channel KEKs, cached messages and upload records; cached password-derived values and download bearer tokens are not exported. A missing random Local KEK is reported as a missing key, never replaced with a newly generated key.

COSE settings fall back to the historical namespace-owned path only after explicit NotFound; permission/network errors do not trigger fallback. Original keyId CBOR, Principal AAD, actual ownership, namespace, version, myIV and existing wraps are preserved. Remote public-key verification and server derivation context are separate evidence; declared historical context/key configuration does not replace deployment or real-sample verification.

Legacy files use whole-file COSE_Encrypt0. OSS transport fragments are 256 KiB, with the actual length for the final fragment. Reads verify indices, lengths, gaps, available source hashes and metadata before/after copying, then perform whole-file AEAD after reassembly. These transport fragments must not be described as independently authenticated chunks of the new format.

## `dmsg-legacy-archive/1`

The format is bounded CBOR, with fields governed by the strict schema in `src/dmsg_legacy/src/archive.ts`:

- `inventory`: old Principal, message canister, explicit mode, `pre_migration` source state, objects, gaps and call records without arguments or tokens.
- `objects[]`: stable source key, kind, original observation encoding, SHA-256, observation time and evidence level such as `query_observation` / `local_cache`. Observation encoding preserves Candid Principal, integer, bytes, array and map types; ordinary queries are not presented as ICP-certified snapshots.
- `keys[]`: historical COSE_Key values and original parameters, restricted to the same old Principal. This field must always be protected by handoff HPKE or local content encryption.
- `checks[]`: fixed object digests and decryption/system-plaintext/deleted/ciphertext-only results. The importer recomputes verification offline rather than trusting the exporter's success labels.
- `createdAt`: the client's collection time, not a trusted timestamp.

Different bytes for the same source key cannot overwrite each other. Deletion holes, missing chunks, permissions, network, versions and missing keys are recorded separately; inaccessible does not mean absent. Old authors and times remain source claims; new device-author signatures are not fabricated.

## `dmsg-legacy-pair/2` and `dmsg-legacy-transfer/1`

The receiving device (`dmsg legacy pair` on the desktop) generates a random one-time receiving seed, X25519 public key and nonce. Its offer is `{version: "dmsg-legacy-pair/2", nonce, publicKey, origin, target, expiresAt}`: `origin` is `https://dmsg.net` or `https://panda.fans`, `target` is the receiving device ID (64 lowercase hex characters), and validity is one hour. The device opens `<origin>/legacy#pair=<base64url(CBOR(offer))>` in the system browser; the fragment never reaches the server and is removed from the address bar after reading. The page accepts only canonical CBOR, its own origin and an unexpired offer. Both ends display the offer CBOR's SHA-256 fingerprint, target device and old identity for the user to compare. The seed is encrypted under the device's local data key in its workspace and is never synced.

The handoff file uses RFC 9180 X25519/HKDF-SHA256/AES-256-GCM HPKE. Each chunk has at most 192 KiB of plaintext. HPKE info/AAD is CBOR `[header,index,previous]`; the header contains the format, complete offer, random transfer ID, chunk count and total length. `previous` is the previous frame encoding's SHA-256, empty for the first frame. The embedded offer must exactly match the saved offer. Missing, reordered, duplicate or substituted frames, wrong targets/origins/fingerprints, and expiry at first receipt are rejected.

Before storing content, the device persists this file's digest and receipt time. The offer can then resume only that file; reimporting it returns the existing archive. Archives are deduplicated by content identity, excluding collection time and transfer metadata; re-exporting the same content also returns the existing archive. The receiving seed is deleted on completion. The entire encrypted handoff file, including framing, is limited to 256 MiB; excess is rejected, not truncated.

Cross-language byte vectors are in `tests/vectors/client/legacy-v1.json`: the page library `@dmsg/legacy` generates and opens them, and the Rust desktop client checks the same file.

## Desktop storage and recovery

The importer verifies the old identity and object digests, and independently opens the owner's historical messages, attachments and avatars offline. Exporter checks are only comparison material; disagreement becomes a gap. Archives are encrypted account content: `migration_part` records of at most 32 MiB each, each carrying a file, plus a `migration` record containing archive digest, content identity, parts, old identity, gap count and frozen comparison. Internal splitting is a storage detail, not a multivolume backup. Historical keys occur only inside archive plaintext, never in ordinary file lists, cloud plaintext or device-authorization flows.

Missing keys, corruption or inaccessible content mark only the affected content; partial archives are still saved and queued for sync. The desktop channel page uses `legacy/channels` to group personal history by source channel; `legacy/report` can select a `channel` to read its available messages while retaining the unreadable count. An incomplete different archive does not block readable archives. Sync status covers the archive record and every part; a content gap does not itself mean sync failure. Official shared-channel inheritance was removed from desktop, CLI and the legacy website on 2026-10-22.

The desktop profile page can prefill a name and description from an imported archive, then publish through the existing public-profile operation after user review. Import itself neither overwrites nor publishes an existing public profile.

Cloud sync carries archives to approved devices of the account, which can also read them offline; pairing seeds and device private keys are excluded. There is no offline recovery package, so a new device still requires on-chain approval. Archive gaps and final-cutover state are recorded independently.

After freezing, the original site exports final historical proofs per channel: CBOR containing that channel's `ChannelAuthority` pages, all `Messages` pages and attachment-manifest pages. The device checks certificates against the reviewed freeze batch and lists changed, new or missing messages, channel-key changes and attachment differences per channel, recording results in `migration`. A channel without freeze proof counts as a difference. Existing freeze-proof verification remains, but the legacy website no longer provides shared-channel plans, approval or member claims.

## Deleting personal history

`legacy/delete_channel {id, channel}` removes one source channel from a specified archive: it re-encodes the archive without that channel's messages, attachments, historical channel keys or browser cache copies, preserving personal profile data and other channels. New parts are persisted before the archive switches to them. Old parts no longer referenced by another archive receive deletion records through ordinary account sync. The original imported file is not rewritten; retention of historical ciphertext and cloud collection follow the sync service's rules, with no promise of immediate physical erasure.

`channel/delete_legacy_draft` cleans up an inactive old inheritance draft only when it has no ledger, epoch, messages or pending activation job. It deletes and syncs the channel record without affecting personal legacy history.
