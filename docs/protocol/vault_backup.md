# Portable vault backup

Desktop, iOS and Android expose **Backup and restore** in Vault tools. Choose **Export encrypted backup**, select the items and linked files, set a password and approve the export on this device. Save the verified file with the system document picker. The local workspace can be offline. Only saved current entries are included. Trash, historical revisions, conflict branches, drafts, dMsg-managed keys and system permissions stay outside the file.

Choose **Import from backup** in an unlocked destination workspace. Enter the original password and review the source, destination and proposed additions. Equal entries are skipped. Changed or deleted entries require explicit selection to create independent copies. Import preserves user-entered wallet phrases and private keys, ordered fields, OTP configuration, timestamps and shared attachment references. Files receive new local keys and IDs. Entries follow normal synchronization after local commit. Deleting a device or changing an account recovery policy cannot revoke copies already saved.

## Encoding

The MIME type is `application/vnd.dmsg.vault-backup+cbor`. All structures use definite-length deterministic CBOR. The outer array has exactly three elements, the header, a byte string containing the encrypted backup key and an array of byte strings containing encrypted blocks.

| Field | Version 1 |
| --- | --- |
| Header H | Array `["dmsg-vault-backup/1", 1, salt, L]`. The salt is a fresh 16-byte byte string. The positive integer L is the encoded plaintext length. |
| Password | NFC, then UTF-8 with case and whitespace preserved. Create with at least six extended grapheme clusters and at most 256 UTF-8 bytes. The reader applies encoding and size limits only. |
| Key encryption | Argon2id v1.3, 65,536 KiB, three iterations, four lanes, 32 output bytes. This key wraps a fresh random 32-byte archive key with a random 12-byte IV. External AAD is the CBOR encoding of `["dmsg/vault-backup/wrap/1", H]`. |
| Content encryption | Split the encoded plaintext into consecutive 1 MiB blocks. For zero-based index i, use the archive key, IV `ASCII("dMsg") || u64be(i)` and the CBOR encoding of `["dmsg/vault-backup/chunk/1", H, i]` as external AAD. |
| COSE profile | Untagged Encrypt0, protected header bytes `a1 01 03`, unprotected header `{5: IV}`, embedded ciphertext with a 16-byte AES-256-GCM tag. Require the exact header profile, block count, lengths, order and EOF. |
| Inner array | `[meta, entries, files]`. Each file is `[metadata, bytes]`, verified against its length and SHA-256. Each entry refers to a file by its zero-based index, or null for a deliberately omitted attachment. |

The inner maps use text keys and Unix millisecond timestamps.

| Map | Fields |
| --- | --- |
| Archive metadata | `archiveId` (16 random bytes), `createdAt`, `environment`, `account`, `withAttachments`, `itemCount`, `fileCount`, `omittedRefs` |
| Entry | `id`, `revision`, `type`, `title`, `tags`, `notes`, ordered `fields`, `favorite`, `createdAt`, `updatedAt`, ordered `attachmentRefs` |
| Entry field | `id`, `type`, `label`, `section`, `value` |
| File metadata | `name`, `mime`, `size`, `sha256` (32 bytes) |

Limits are 10,000 entries, 4,096 files, 100 MiB per file, 256 MiB of attachments, 16 MiB of metadata and 320 MiB per archive. The source account in an archive is descriptive. Authority is checked in the destination workspace. A workspace upgraded from a version which did not record verified write permission may need to sync once before its first import. A missing attachment must be retrieved or explicitly omitted before export.

The synthetic interoperability fixture uses the password `123456`. The two entries share a small attachment. The Rust compatibility test decrypts its envelopes with both COSE implementations and reproduces their exact bytes. [Test archive](../../tests/vectors/client/vault-backup-v1.dmsgvault) · [Core implementation](../../src/dmsg_client/src/vault/backup/mod.rs)
