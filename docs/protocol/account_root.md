# Account control and root-envelope contract

English | [简体中文](account_root_zh.md)

Reviewed 2026-10-24 against public source `9fd70dca`. This document describes desktop authentication bindings, account-separated workspaces and recovery after every device is lost. Release artifacts and acceptance of specific deployments are checked separately.
It describes the public client implementation, not acceptance of production keys or deployment.

## Account control

`dmsg_client::account` directly uses command types from `dmsg_types`. Client and canister use the same `dmsg_protocol` deterministic CBOR: Principal, AccountId and fixed-byte fields are byte strings, and unit variants are text. Account creation uses `dmsg/create-account/v1`; when the home configures an admission public key, it also requires a cloud ticket for `dmsg/account-admission/v1` binding home, caller and expiry. Account changes use `dmsg/device-approval/v2`, `dmsg/account/v2` and `dmsg/account-operation/v2`; document attestation uses `dmsg/attest/v1`, root derivation `dmsg/derive-root/v1`, recovery-device PoP `dmsg/recovery-device/v1`, and controller-registration PoP `dmsg/controller-pop/v1`. Signatures cover the actual home/caller, account, command, version, device sequence, request ID and deadline.

The client does not fix one user home. It reads `dmsg_handle.get_handle_config()` for `user_homes`, the authoritative list (`config.toml`'s `user_homes` are optional pins that must all be listed), and `registration_homes`, the currently accepting subset from which registration picks randomly. After login, it calls `my_account` on all listed homes in parallel. Once all succeed it selects the first matching account in list order; any failed query means unknown, never fallback registration. An account's `home_user` is fixed for its lifetime.

Before network access, the client writes the full request into the workspace's encrypted journal. A lost reply is reconciled through `my_account` / `get_operation` for the original operation. Successful authentication or an account ID is not treated as device approval. IC certificates first authenticate the current security leaf (schema 4), complete device map, versions and root digests; an older reply cannot overwrite a higher observed version or installed root.

A new-device request binds the target account, home, device public keys, role, capabilities, expected version and request ID for administrator review. Members default to ContentSign/VaultUnlock; administrators additionally get RootManage, but not FormalApprove/PaymentOffer by default. Root bundles wrap only for active devices with `VaultUnlock`: a new device with it can read the root after administrator rotation; devices without it receive no envelope and do not attempt opening. The cloud likewise requires `VaultUnlock` for account ciphertext reads. Initial devices and login-recovery replacement devices require both `RootManage` and `VaultUnlock`. Authentication binding has three steps: the new login creates a local nonce and sends home/account/Principal/nonce to an administrator device; approval of `BindAuth { principal, nonce }` records a pending entry valid for 10 minutes; the new login calls `accept_auth_binding` with that nonce to complete binding. Login identity is distinct from device authorization.

Client authentication binding (`dmsg_client::security`) has two routes, both requiring administrator approval through the approval center:

- **Across devices:** a new login without an account creates a token, base64url deterministic CBOR `{format: "dmsg-auth-request/1", home, account, principal, nonce, expires_at}`, valid for 5 minutes. The nonce stays in that login's workspace. The administrator checks home, account and expiry, and a person enters the login Principal displayed on the other side before creating approval. Once approved, the new login calls `accept_auth_binding` and selects the account workspace: an already-bound local workspace unlocks directly; otherwise pairing or recovery follows.
- **On one device:** the unlocked device starts a login for binding. When the daemon receives the second login's delegation, it does not switch accounts; it verifies that the identity has no account, generates a nonce and creates approval. Approval submits `BindAuth`, then calls `accept_auth_binding` as the second login. That identity remains in memory for at most 10 minutes.

`RemoveAuth` (which cannot remove the current login), `SetPolicy` (freeze, daily executions and allowed purposes), `SetRecoveryDelay` and `DisputeRecovery` also go through the approval center. Approval records store login Principals as text, converted to commands only before signing.

## Local workspaces

Each account has its own invisible-to-other-accounts workspace: `<home>/<environment>/accounts/<account>/` contains `workspace.sqlite` and Agent connector state `agents.sqlite`. A login without an account uses `logins/<principal>/`; once it creates or binds an account, the directory is renamed to `accounts/<account>/`. Login locates the account with `my_account` and selects its workspace, or the login's own workspace if there is none. Different logins for the same account share a workspace. `current` stores the last selected account for daemon restart and fast unlock. Switching accounts locks first and discards resident keys. Each workspace has its own device ID and keys, so different accounts on the same computer are distinct devices.

## Local unlock

There is no local unlock password, recovery code or service-independent offline recovery package; file export is not account recovery. The local data key LDK is wrapped only under these two keys:

```text
name        = "dmsg:" + environment + ":" + account + ":" + deviceId
LUK         = HKDF(unlock_secret, ["dmsg/local-unlock/2", name])
wrappedKey  = E(LUK, LDK, ["dmsg/local-key/2", name])
fastWrapped = E(K_fast, LDK, ["dmsg/local-key/2", "fast:" + name])   // optional
```

- `unlock_secret(account_id, device_id)` is an authenticated user-home query: the caller must be an account login Principal and the device must exist and not be revoked. It returns the 32-byte `digest("dmsg/unlock-secret/v1", (master_secret, account_id, device_id))`, where `digest(domain, value) = SHA256(CBOR([1, domain, value]))`. The home's initialization timer generates `master_secret` once with `raw_rand`, preserving it across upgrades. Revoked devices no longer receive unlock material.
- Before account binding, a new workspace wraps LDK under a temporary random key stored in plaintext. It contains only newly generated device keys, no content, and refuses content writes. After account creation or pairing approval, the client obtains `unlock_secret`, rewraps LDK under LUK and deletes the temporary key.
- Ordinary unlock: II login through the dmsg.net login page ([dmsg-login/1](login-v1.md), a 30-day delegation with the session private key held by the daemon) → `unlock_secret` → LUK → LDK → Bundle, all in memory. `login_unlocked_at` records the latest login unlock.
- Fast unlock defaults on after login unlock and stays off if the person disables it; `secure_storage = "file"` has no fast unlock. `K_fast` wraps only a second LDK copy. On macOS, Secure Enclave key agreement recovers it, requiring Touch ID or the Mac login password each time and restoring the LDK-sealed 30-day login. Other platforms store a random 32-byte key in the credential store without user-presence verification or saved login. More than 30 days after login unlock, fast unlock is refused. Once online after fast unlock, the certified device table is checked; a revoked device's local data, including the sealed login, is cleared.

## Immutable RootBundle v2

Bytes use deterministic CBOR:

```text
{ body: {
    format: "dmsg-root-bundle/2",
    context: { account, environment, generation, opId, securityEpoch },
    commitment: hex(SHA256(HKDF(VRK, ["dmsg/root-commitment/1"]))),
    envelopes: [ { device, enc, ciphertext } ... ],      // ascending device order; one per active VaultUnlock device
    recoveryKey: { homeCose, keyName, publicKey },        // COSE content-root vetKD public key
    recovery: base64url(IbeCiphertext),                   // IBE to identity CBOR([AccountId, generation])
    previous: null | { digest, uploadId, generation, envelope },
    device, signingPublic
  }, signature: bytes64 }
```

Public keys, COSE_Encrypt0 and HPKE envelope bytes use unpadded base64url; digests and device/operation/upload IDs use lowercase hex. The account is a canonical 20-character Xid, generations are positive safe integers, and `securityEpoch` is the reserved slot's security version.

- `signature` is the device's Ed25519 signature over `digest("dmsg/root-bundle/2", body)`. Mathematical validity alone does not prove device authority; reading the current root also requires the user's certified root commitment.
- Device envelope: `HPKE.Seal(device.hpke_pub, VRK, info = aad = CBOR(["dmsg/device-root/1", environment, account, generation, device_id]))`, using the same RFC 9180 X25519/HKDF-SHA256/AES-256-GCM primitives as channel epoch keys. After opening, readers check `commitment` so every device receives the same root.
- Recovery envelope: `ic-vetkeys`' `IbeCiphertext::encrypt(dpk, identity, VRK, seed)`. `dpk` is the 96-byte derived public key returned by COSE `root_public_key(account_id, generation)`; all accounts share the context public key. `identity = CBOR([AccountId bytes12, generation])`. The client checks home, environment, generation, derivation version 2, account, `SHA256(public_key)` fingerprint and key name (`key_1` in production).
- `previous.envelope` wraps the previous VRK under the new VRK with AAD `["dmsg/previous-root/1", account, generation, previous.generation, previous.digest]`. `previous.digest` is SHA-256 of the previous bundle's **bytes**, the cloud manifest digest, and `uploadId` locates it. Readers verify the digest and strictly decreasing generations, following at most 256 generations.
- On-chain `ContentRootRef { generation, suite: "dmsg-root-v2", recipients_digest, body_digest, bundle_digest }`: `recipients_digest = digest("dmsg/root-recipients/1", [sorted device_id bytes, generation])`, `body_digest = SHA256(CBOR(body))`, and `bundle_digest = digest("dmsg/root-bundle-digest/2", [recipients_digest, body_digest])`. `CommitRoot` recomputes recipients from currently active `VaultUnlock` devices and requires the approving device to be included. Any mismatch fails, excluding revoked, non-VaultUnlock and unapproved devices from new roots.

Each bundle is at most 64,000 bytes. It is stored as a cloud `kind=root` upload: the manifest is the raw bundle, and the required single chunk is the deterministic CBOR empty array `0x80`. The cloud ObjectRef digest is the manifest SHA-256; the client downloads by it and verifies the on-chain `bundle_digest`.

## Initialization and root rotation

1. Verify account and device capabilities; reserve a generation with a separate op_id (`ReserveRoot`). Abandoned/expired reservations can leave generation gaps.
2. Generate a random VRK locally. The rotation job and candidate root are LDK-sealed in `root-job` and reused on retries. Wrap for every active `VaultUnlock` device in the certified table; query and check the COSE recovery public key, create the IBE envelope and produce one immutable bundle encoding.
3. Persist the upload plan and upload_id. Retries query the same upload and reuse its plan/ciphertext. After upload and finalize, download the manifest and check SHA-256 again.
4. Perform the unique user `CommitRoot` CAS. Successful upload does not substitute for successful on-chain commit.
5. Enable the root after reading back the certified commitment. On initial binding the workspace then belongs to the account; before that it has no content. After rotation, old roots are read through the new bundle's `previous` chain.

This process makes no on-chain key call. Only changes to root recipients (adding/revoking a `VaultUnlock` device or changing that capability) and login recovery set `vault_write_state` to `RekeyRequired`; an administrator rotates the root. Authentication bindings and other capability changes only advance the security epoch. Other recipients download the current bundle and open their own envelopes.

## Recovery after losing every device

Client implementation is in `dmsg_client::recovery`. The request is sealed in the new device's workspace and resent unchanged after lost replies, without restarting the wait. Derivation approval and transport-key seed are sealed as `recover-root:<digest>` under the committed root digest. Results are queried by execution ID first; reapproval happens only when the original approval expired and certified time/device sequence prove it can no longer execute. `max_cycles` is fixed at 70 billion. The client opens the recovery envelope, follows `previous` for historical roots, then rotates.

1. Log in on the new device using a bound II identity and submit `request_recovery(account_id, RecoveryRequest { op_id, new_auth = caller, device, expires_at }, device_proof)` with PoP `dmsg/recovery-device/v1`. If II is lost too, recovery is impossible.
2. Wait `recovery_delay_ms`, default 3 days and configurable to 1–7 days with `SetRecoveryDelay`. Existing devices see `pending_recovery_digest` in the certified leaf. Any valid device can cancel with `DisputeRecovery { op_id }`; removing the requesting login with `RemoveAuth` also invalidates it. No additional confirmation follows.
3. After the delay, `complete_recovery(account_id, op_id)` replaces all devices and login bindings, increments `security_epoch`, marks the existing root `RekeyRequired` and records `recovered_device = (device_id, generation)`.
4. The recovering device obtains `unlock_secret` to bind locally, then calls `derive_root(DeriveRootRequest { generation, transport_public_key, max_cycles, approval })`. Only `recovered_device` may derive for the current generation; the same `request_id` returns the original result, and a new approval may derive again before rotation. COSE derives the identity's vetKey and encrypts it for the transport public key. The client decrypts and verifies the vetKey, opens the root bundle's IBE envelope, then rotates to the next generation wrapped only for itself. Rotation removes derivation authority.

Recovery approval uses `dmsg/derive-root/v1` with command `[generation, transport_public_key, max_cycles]`; the client commits to the cycles ceiling in its approval. Derivation and formal attestation share the account policy's daily execution limit (20 by default, at most 64 through `SetPolicy`) and 64-result retention window. A stolen II login with no device dispute during the delay exposes content; this is the tradeoff of removing recovery codes.
